use crate::{
    rendezvous_messages::{self, ToJson},
    server::{check_zombie, new as new_server, ServerPtr},
    turn_client,
};
use futures::{SinkExt, StreamExt};
use hbb_common::{
    allow_err,
    anyhow::{anyhow, bail},
    config::{self, Config, CONNECT_TIMEOUT, REG_INTERVAL, RENDEZVOUS_PORT},
    futures::future::join_all,
    log,
    protobuf::Message as _,
    rendezvous_proto::*,
    sleep, socket_client,
    tokio::{
        self, select,
        time::{interval, Duration},
    },
    ResultType,
};
use soketto::{handshake::ServerResponse, Data};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Message = RendezvousMessage;

lazy_static::lazy_static! {
    static ref SOLVING_PK_MISMATCH: Arc<Mutex<String>> = Default::default();
}
static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct RendezvousMediator {
    addr: SocketAddr,
}

impl RendezvousMediator {
    pub fn restart() {
        SHOULD_EXIT.store(true, Ordering::SeqCst);
        log::info!("server restart");
    }

    pub async fn start_all() {
        check_zombie();
        let server = new_server();
        let server_cloned = server.clone();
        tokio::spawn(async move {
            direct_server(server_cloned).await;
        });
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if crate::platform::is_installed() {
            std::thread::spawn(move || {
                allow_err!(lan_discovery());
            });
        }
        loop {
            Config::reset_online();
            if Config::get_option("stop-service").is_empty() {
                let mut futs = Vec::new();
                if let Some(servers) = Config::get_rendezvous_servers().await {
                    SHOULD_EXIT.store(false, Ordering::SeqCst);
                    for host in servers.clone() {
                        let server = server.clone();
                        futs.push(tokio::spawn(async move {
                            allow_err!(Self::start(server, host).await);
                            // SHOULD_EXIT here is to ensure once one exits, the others also exit.
                            SHOULD_EXIT.store(true, Ordering::SeqCst);
                        }));
                    }
                    join_all(futs).await;
                }
            }
            sleep(1.).await;
        }
    }

    pub async fn start(server: ServerPtr, host_list: String) -> ResultType<()> {
        log::info!("start rendezvous mediator of {}", host_list);

        let public_addr = match turn_client::get_public_ip().await {
            Some(addr) => addr,
            None => bail!("Failed to retreive public IP address"),
        };

        let (local_ip, host, websocket_client) = create_websocket(&host_list).await?;

        let (mut sender, receiver) = websocket_client.split();
        Config::update_latency(&host, 200);
        Config::set_key_confirmed(true);
        Config::set_host_key_confirmed(&host, true);

        const TIMER_OUT: Duration = Duration::from_secs(1);
        let mut timer = interval(TIMER_OUT);
        let mut last_timer = SystemTime::UNIX_EPOCH;

        let socket_packets = futures::stream::unfold(receiver, move |mut receiver| async {
            match receiver.next().await {
                Some(Ok(msg)) => Some((Ok(msg), receiver)),
                Some(Err(err)) => Some((Err(err), receiver)),
                None => None,
            }
        });

        tokio::pin!(socket_packets);

        loop {
            use futures::StreamExt;

            select! {
                _ = timer.tick() => {
                    if SHOULD_EXIT.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = SystemTime::now();
                    if now.duration_since(last_timer).map(|d| d < TIMER_OUT).unwrap_or(false) {
                        // a workaround of tokio timer bug
                        continue;
                    }
                    last_timer = now;
                }
                Some(data) = socket_packets.next() => {
                    match data {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(msg)) => {
                        log::info!("redenzvous_mediator msg: {msg}");
                        if let Ok(connect_request) =
                            serde_json::from_str::<rendezvous_messages::ConnectRequest>(&msg)
                        {
                            let listener =
                                hbb_common::tcp::new_listener(SocketAddr::new(local_ip, 0), true)
                                    .await?;
                            if let Ok(_) = sender
                                .send(
                                    tokio_tungstenite::tungstenite::Message::Text(rendezvous_messages::Listening::new(
                                        connect_request.sender_id,
                                        listener.local_addr().unwrap(),
                                        public_addr,
                                        Config::get_key_pair().1,
                                    )
                                    .to_json()),
                                )
                                .await
                            {
                                let server_clone = server.clone();
                                tokio::spawn(async move {
                                    let _ = crate::accept(listener, server_clone, true).await;
                                });
                            }
                        } else if let Ok(relay_connection) =
                            serde_json::from_str::<rendezvous_messages::RelayConnection>(&msg)
                        {
                            if let Ok(stream) = socket_client::connect_tcp(
                                relay_connection.addr,
                                Config::get_any_listen_addr(),
                                CONNECT_TIMEOUT,
                            )
                            .await
                            {
                                let data = socket_packets.next().await;
                                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(msg))) =
                                    data
                                {
                                    if let Ok(_) = serde_json::from_str::<
                                        rendezvous_messages::RelayReady,
                                    >(&msg)
                                    {
                                        let server_clone = server.clone();
                                        let addr = relay_connection.addr;
                                        tokio::spawn(async move {
                                            let _ = crate::create_tcp_connection(
                                                server_clone,
                                                stream,
                                                addr,
                                                true,
                                            )
                                            .await;
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => bail!("Failed to receive next {}", e),
                    _ => bail!("Received binary message from rendezvous server"),
                }
                }
            }
        }
        Ok(())
    }
}

async fn create_websocket(
    host_list: &str,
) -> ResultType<(
    std::net::IpAddr,
    String,
    WebSocketStream<MaybeTlsStream<TcpStream>>,
)> {
    let hosts = host_list.split(';');
    for host in hosts {
        let ret = create_websocket_(host).await;
        allow_err!(&ret);

        if ret.is_ok() {
            return ret;
        }
    }

    bail!("Failed to connect any of the hosts in list");
}

async fn create_websocket_(
    host: &str,
) -> ResultType<(
    std::net::IpAddr,
    String,
    WebSocketStream<MaybeTlsStream<TcpStream>>,
)> {
    let mut split = host.split("://").collect::<Vec<&str>>();
    if split.len() < 1 {
        bail!("Uri must contain both scheme and host");
    } else if split.len() == 1 {
        // Use ws by default
        split.insert(0, "ws");
    }

    let scheme = split[0];
    let host = crate::check_port(split[1], RENDEZVOUS_PORT);

    log::info!("Trying to connect websocket to {}", host);
    let addr = host
        .to_socket_addrs()?
        .next()
        .ok_or(anyhow!("Cannot resolve dns for the host"))?;
    log::info!("Parsed addr: {:?}", &addr);

    let socket = TcpStream::connect(addr).await?;
    let local_ip = socket.local_addr().unwrap().ip();
    let uri = format!("{}://{}/?user={}", scheme, host, Config::get_id());

    let (websocket, _) = tokio_tungstenite::client_async_tls(&uri, socket).await?;

    log::info!("Websocket connected succesfully");
    return Ok((local_ip, host, websocket));
}

fn get_direct_port() -> i32 {
    let mut port = Config::get_option("direct-access-port")
        .parse::<i32>()
        .unwrap_or(0);
    if port <= 0 {
        port = RENDEZVOUS_PORT + 2;
    }
    port
}

async fn direct_server(server: ServerPtr) {
    let mut listener = None;
    let mut port = 0;
    loop {
        let disabled = Config::get_option("direct-server").is_empty();
        if !disabled && listener.is_none() {
            port = get_direct_port();
            let addr = format!("0.0.0.0:{}", port);
            match hbb_common::tcp::new_listener(&addr, false).await {
                Ok(l) => {
                    listener = Some(l);
                    log::info!(
                        "Direct server listening on: {:?}",
                        listener.as_ref().unwrap().local_addr()
                    );
                }
                Err(err) => {
                    // to-do: pass to ui
                    log::error!(
                        "Failed to start direct server on : {}, error: {}",
                        addr,
                        err
                    );
                    loop {
                        if port != get_direct_port() {
                            break;
                        }
                        sleep(1.).await;
                    }
                }
            }
        }
        if let Some(l) = listener.as_mut() {
            if disabled || port != get_direct_port() {
                log::info!("Exit direct access listen");
                listener = None;
                continue;
            }
            if let Ok(Ok((stream, addr))) = hbb_common::timeout(1000, l.accept()).await {
                stream.set_nodelay(true).ok();
                log::info!("direct access from {}", addr);
                let local_addr = stream.local_addr().unwrap_or(Config::get_any_listen_addr());
                let server = server.clone();
                tokio::spawn(async move {
                    allow_err!(
                        crate::server::create_tcp_connection(
                            server,
                            hbb_common::Stream::from(stream, local_addr),
                            addr,
                            false,
                        )
                        .await
                    );
                });
            } else {
                sleep(0.1).await;
            }
        } else {
            sleep(1.).await;
        }
    }
}

#[inline]
pub fn get_broadcast_port() -> u16 {
    (RENDEZVOUS_PORT + 3) as _
}

pub fn get_mac() -> String {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Ok(Some(mac)) = mac_address::get_mac_address() {
        mac.to_string()
    } else {
        "".to_owned()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    "".to_owned()
}

fn lan_discovery() -> ResultType<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], get_broadcast_port()));
    let socket = std::net::UdpSocket::bind(addr)?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(1000)))?;
    log::info!("lan discovery listener started");
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::peer_discovery(p)) => {
                        if p.cmd == "ping" {
                            let mut msg_out = Message::new();
                            let peer = PeerDiscovery {
                                cmd: "pong".to_owned(),
                                mac: get_mac(),
                                id: Config::get_id(),
                                hostname: whoami::hostname(),
                                username: crate::platform::get_active_username(),
                                platform: whoami::platform().to_string(),
                                ..Default::default()
                            };
                            msg_out.set_peer_discovery(peer);
                            socket.send_to(&msg_out.write_to_bytes()?, addr).ok();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub fn discover() -> ResultType<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 0));
    let socket = std::net::UdpSocket::bind(addr)?;
    socket.set_broadcast(true)?;
    let mut msg_out = Message::new();
    let peer = PeerDiscovery {
        cmd: "ping".to_owned(),
        ..Default::default()
    };
    msg_out.set_peer_discovery(peer);
    let maddr = SocketAddr::from(([255, 255, 255, 255], get_broadcast_port()));
    socket.send_to(&msg_out.write_to_bytes()?, maddr)?;
    log::info!("discover ping sent");
    let mut last_recv_time = Instant::now();
    let mut last_write_time = Instant::now();
    let mut last_write_n = 0;
    // to-do: load saved peers, and update incrementally (then we can see offline)
    let mut peers = Vec::new();
    let mac = get_mac();
    socket.set_read_timeout(Some(std::time::Duration::from_millis(10)))?;
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, _)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::peer_discovery(p)) => {
                        last_recv_time = Instant::now();
                        if p.cmd == "pong" {
                            if p.mac != mac {
                                peers.push((p.id, p.username, p.hostname, p.platform));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if last_write_time.elapsed().as_millis() > 300 && last_write_n != peers.len() {
            config::LanPeers::store(serde_json::to_string(&peers)?);
            last_write_time = Instant::now();
            last_write_n = peers.len();
        }
        if last_recv_time.elapsed().as_millis() > 3_000 {
            break;
        }
    }
    log::info!("discover ping done");
    config::LanPeers::store(serde_json::to_string(&peers)?);
    Ok(())
}
