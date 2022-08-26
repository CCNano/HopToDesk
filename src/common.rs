#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use arboard::Clipboard as ClipboardContext;
use hbb_common::{
    allow_err,
	anyhow::bail,
    compress::{compress as compress_func, decompress},
    config::{self, Config, COMPRESS_LEVEL, RENDEZVOUS_TIMEOUT},
    get_version_number, log,
    message_proto::*,
    protobuf::Message as _,
    protobuf::ProtobufEnum,
    rendezvous_proto::*,
    sleep, socket_client, tokio, ResultType,
};
#[cfg(any(target_os = "android", target_os = "ios", feature = "cli"))]
use hbb_common::{config::RENDEZVOUS_PORT, futures::future::join_all};
use std::sync::{Arc, Mutex};

pub const CLIPBOARD_NAME: &'static str = "clipboard";
pub const CLIPBOARD_INTERVAL: u64 = 333;

lazy_static::lazy_static! {
    pub static ref CONTENT: Arc<Mutex<String>> = Default::default();
    pub static ref SOFTWARE_UPDATE_URL: Arc<Mutex<String>> = Default::default();
}

#[cfg(any(target_os = "android", target_os = "ios"))]
lazy_static::lazy_static! {
    pub static ref MOBILE_INFO1: Arc<Mutex<String>> = Default::default();
    pub static ref MOBILE_INFO2: Arc<Mutex<String>> = Default::default();
}

#[inline]
pub fn valid_for_numlock(evt: &KeyEvent) -> bool {
    if let Some(key_event::Union::control_key(ck)) = evt.union {
        let v = ck.value();
        (v >= ControlKey::Numpad0.value() && v <= ControlKey::Numpad9.value())
            || v == ControlKey::Decimal.value()
    } else {
        false
    }
}

pub fn create_clipboard_msg(content: String) -> Message {
    let bytes = content.into_bytes();
    let compressed = compress_func(&bytes, COMPRESS_LEVEL);
    let compress = compressed.len() < bytes.len();
    let content = if compress { compressed } else { bytes };
    let mut msg = Message::new();
    msg.set_clipboard(Clipboard {
        compress,
        content,
        ..Default::default()
    });
    msg
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn check_clipboard(
    ctx: &mut ClipboardContext,
    old: Option<&Arc<Mutex<String>>>,
) -> Option<Message> {
    let side = if old.is_none() { "host" } else { "client" };
    let old = if let Some(old) = old { old } else { &CONTENT };
    if let Ok(content) = ctx.get_text() {
        if content.len() < 2_000_000 && !content.is_empty() {
            let changed = content != *old.lock().unwrap();
            if changed {
                log::info!("{} update found on {}", CLIPBOARD_NAME, side);
                *old.lock().unwrap() = content.clone();
                return Some(create_clipboard_msg(content));
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn update_clipboard(clipboard: Clipboard, old: Option<&Arc<Mutex<String>>>) {
    let content = if clipboard.compress {
        decompress(&clipboard.content)
    } else {
        clipboard.content
    };
    if let Ok(content) = String::from_utf8(content) {
        if content.is_empty() {
            // ctx.set_text may crash if content is empty
            return;
        }
        match ClipboardContext::new() {
            Ok(mut ctx) => {
                let side = if old.is_none() { "host" } else { "client" };
                let old = if let Some(old) = old { old } else { &CONTENT };
                *old.lock().unwrap() = content.clone();
                allow_err!(ctx.set_text(content));
                log::debug!("{} updated on {}", CLIPBOARD_NAME, side);
            }
            Err(err) => {
                log::error!("Failed to create clipboard context: {}", err);
            }
        }
    }
}

#[cfg(feature = "use_rubato")]
pub fn resample_channels(
    data: &[f32],
    sample_rate0: u32,
    sample_rate: u32,
    channels: u16,
) -> Vec<f32> {
    use rubato::{
        InterpolationParameters, InterpolationType, Resampler, SincFixedIn, WindowFunction,
    };
    let params = InterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: InterpolationType::Nearest,
        oversampling_factor: 160,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f64>::new(
        sample_rate as f64 / sample_rate0 as f64,
        params,
        data.len() / (channels as usize),
        channels as _,
    );
    let mut waves_in = Vec::new();
    if channels == 2 {
        waves_in.push(
            data.iter()
                .step_by(2)
                .map(|x| *x as f64)
                .collect::<Vec<_>>(),
        );
        waves_in.push(
            data.iter()
                .skip(1)
                .step_by(2)
                .map(|x| *x as f64)
                .collect::<Vec<_>>(),
        );
    } else {
        waves_in.push(data.iter().map(|x| *x as f64).collect::<Vec<_>>());
    }
    if let Ok(x) = resampler.process(&waves_in) {
        if x.is_empty() {
            Vec::new()
        } else if x.len() == 2 {
            x[0].chunks(1)
                .zip(x[1].chunks(1))
                .flat_map(|(a, b)| a.into_iter().chain(b))
                .map(|x| *x as f32)
                .collect()
        } else {
            x[0].iter().map(|x| *x as f32).collect()
        }
    } else {
        Vec::new()
    }
}

#[cfg(feature = "use_dasp")]
pub fn resample_channels(
    data: &[f32],
    sample_rate0: u32,
    sample_rate: u32,
    channels: u16,
) -> Vec<f32> {
    use dasp::{interpolate::linear::Linear, signal, Signal};
    let n = data.len() / (channels as usize);
    let n = n * sample_rate as usize / sample_rate0 as usize;
    if channels == 2 {
        let mut source = signal::from_interleaved_samples_iter::<_, [_; 2]>(data.iter().cloned());
        let a = source.next();
        let b = source.next();
        let interp = Linear::new(a, b);
        let mut data = Vec::with_capacity(n << 1);
        for x in source
            .from_hz_to_hz(interp, sample_rate0 as _, sample_rate as _)
            .take(n)
        {
            data.push(x[0]);
            data.push(x[1]);
        }
        data
    } else {
        let mut source = signal::from_iter(data.iter().cloned());
        let a = source.next();
        let b = source.next();
        let interp = Linear::new(a, b);
        source
            .from_hz_to_hz(interp, sample_rate0 as _, sample_rate as _)
            .take(n)
            .collect()
    }
}

#[cfg(feature = "use_samplerate")]
pub fn resample_channels(
    data: &[f32],
    sample_rate0: u32,
    sample_rate: u32,
    channels: u16,
) -> Vec<f32> {
    use samplerate::{convert, ConverterType};
    convert(
        sample_rate0 as _,
        sample_rate as _,
        channels as _,
        ConverterType::SincBestQuality,
        data,
    )
    .unwrap_or_default()
}

pub fn test_nat_type() {
    let mut i = 0;
    std::thread::spawn(move || loop {
        /*match test_nat_type_() {
            Ok(true) => break,
            Err(err) => {
                log::error!("test nat: {}", err);
            }
            _ => {}
        }*/
        if Config::get_nat_type() != 0 {
            break;
        }
        i = i * 2 + 1;
        if i > 300 {
            i = 300;
        }
        std::thread::sleep(std::time::Duration::from_secs(i));
    });
}


#[cfg(target_os = "ios")]
pub async fn get_rendezvous_server(ms_timeout: u64) -> Option<String> {
        Config::get_rendezvous_server().await
}

#[cfg(not(target_os = "ios"))]
pub async fn get_rendezvous_server(ms_timeout: u64) -> Option<String> {
    crate::ipc::get_rendezvous_server(ms_timeout).await
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn get_nat_type(_ms_timeout: u64) -> i32 {
    Config::get_nat_type()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn get_nat_type(ms_timeout: u64) -> i32 {
    crate::ipc::get_nat_type(ms_timeout).await
}

#[inline]
pub fn get_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0) as _
}

pub fn run_me<T: AsRef<std::ffi::OsStr>>(args: Vec<T>) -> std::io::Result<std::process::Child> {
    #[cfg(not(feature = "appimage"))]
    {
        let cmd = std::env::current_exe()?;
        return std::process::Command::new(cmd).args(&args).spawn();
    }
    #[cfg(feature = "appimage")]
    {
        let appdir = std::env::var("APPDIR").unwrap();
        let appimage_cmd = std::path::Path::new(&appdir).join("AppRun");
        log::info!("path: {:?}", appimage_cmd);
        return std::process::Command::new(appimage_cmd).args(&args).spawn();
    }
}

pub fn username() -> String {
    // fix bug of whoami
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return whoami::username().trim_end_matches('\0').to_owned();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return MOBILE_INFO2.lock().unwrap().clone();
}

#[inline]
pub fn check_port<T: std::string::ToString>(host: T, port: i32) -> String {
    let host = host.to_string();
    if !host.contains(":") {
        return format!("{}:{}", host, port);
    }
    return host;
}

pub const POSTFIX_SERVICE: &'static str = "_service";

#[inline]
pub fn is_control_key(evt: &KeyEvent, key: &ControlKey) -> bool {
    if let Some(key_event::Union::control_key(ck)) = evt.union {
        ck.value() == key.value()
    } else {
        false
    }
}

#[inline]
pub fn is_modifier(evt: &KeyEvent) -> bool {
    if let Some(key_event::Union::control_key(ck)) = evt.union {
        let v = ck.value();
        v == ControlKey::Alt.value()
            || v == ControlKey::Shift.value()
            || v == ControlKey::Control.value()
            || v == ControlKey::Meta.value()
            || v == ControlKey::RAlt.value()
            || v == ControlKey::RShift.value()
            || v == ControlKey::RControl.value()
            || v == ControlKey::RWin.value()
    } else {
        false
    }
}

pub fn check_software_update() {
    std::thread::spawn(move || allow_err!(_check_software_update()));
}

#[tokio::main(flavor = "current_thread")]
async fn _check_software_update() -> hbb_common::ResultType<()> {
    sleep(3.).await;

    let server = match get_rendezvous_server(1_000).await {
        Some(server) => server,
        None => bail!("Failed to retrieve rendez-vous server address")
    };
    let rendezvous_server = socket_client::get_target_addr(&server)?;
    let mut socket =
        socket_client::new_udp(Config::get_any_listen_addr(), RENDEZVOUS_TIMEOUT).await?;

    let mut msg_out = RendezvousMessage::new();
    msg_out.set_software_update(SoftwareUpdate {
        url: crate::VERSION.to_owned(),
        ..Default::default()
    });
    socket.send(&msg_out, rendezvous_server).await?;
    use hbb_common::protobuf::Message;
    if let Some(Ok((bytes, _))) = socket.next_timeout(30_000).await {
        if let Ok(msg_in) = RendezvousMessage::parse_from_bytes(&bytes) {
            if let Some(rendezvous_message::Union::software_update(su)) = msg_in.union {
                let version = hbb_common::get_version_from_url(&su.url);
                if get_version_number(&version) > get_version_number(crate::VERSION) {
                    *SOFTWARE_UPDATE_URL.lock().unwrap() = su.url;
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios", feature = "cli")))]
pub fn get_icon() -> String {
    hbb_common::config::ICON.to_owned()
}

pub fn get_app_name() -> String {
    hbb_common::config::APP_NAME.read().unwrap().clone()
}

#[cfg(target_os = "macos")]
pub fn get_full_name() -> String {
    format!(
        "{}.{}",
        hbb_common::config::ORG.read().unwrap(),
        hbb_common::config::APP_NAME.read().unwrap(),
    )
}

pub fn is_ip(id: &str) -> bool {
    hbb_common::regex::Regex::new(r"^\d+\.\d+\.\d+\.\d+(:\d+)?$")
        .unwrap()
        .is_match(id)
}

pub fn get_uuid() -> Vec<u8> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Ok(id) = machine_uid::get() {
        return id.into();
    }
    Config::get_key_pair().1
}

pub async fn post_request(url: String, body: String, header: &str) -> ResultType<String> {
    #[cfg(not(target_os = "linux"))]
    {
        let mut req = reqwest::Client::new().post(url);
        if !header.is_empty() {
            let tmp: Vec<&str> = header.split(": ").collect();
            if tmp.len() == 2 {
                req = req.header(tmp[0], tmp[1]);
            }
        }
        req = req.header("Content-Type", "application/json");
        let to = std::time::Duration::from_secs(12);
        Ok(req.body(body).timeout(to).send().await?.text().await?)
    }
    #[cfg(target_os = "linux")]
    {
        let mut data = vec![
            "curl",
            "-sS",
            "-X",
            "POST",
            &url,
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            "--connect-timeout",
            "12",
        ];
        if !header.is_empty() {
            data.push("-H");
            data.push(header);
        }
        let output = async_process::Command::new("curl")
            .args(&data)
            .output()
            .await?;
        let res = String::from_utf8_lossy(&output.stdout).to_string();
        if !res.is_empty() {
            return Ok(res);
        }
        bail!(String::from_utf8_lossy(&output.stderr).to_string());
    }
}

#[tokio::main(flavor = "current_thread")]
pub async fn post_request_sync(url: String, body: String, header: &str) -> ResultType<String> {
    post_request(url, body, header).await
}

#[inline]
pub fn make_privacy_mode_msg(state: back_notification::PrivacyModeState) -> Message {
    let mut misc = Misc::new();
    let mut back_notification = BackNotification::new();
    back_notification.set_privacy_mode_state(state);
    misc.set_back_notification(back_notification);
    let mut msg_out = Message::new();
    msg_out.set_misc(misc);
    msg_out
}

pub fn make_fd_to_json(fd: FileDirectory) -> String {
    use serde_json::json;
    let mut fd_json = serde_json::Map::new();
    fd_json.insert("id".into(), json!(fd.id));
    fd_json.insert("path".into(), json!(fd.path));

    let mut entries = vec![];
    for entry in fd.entries {
        let mut entry_map = serde_json::Map::new();
        entry_map.insert("entry_type".into(), json!(entry.entry_type.value()));
        entry_map.insert("name".into(), json!(entry.name));
        entry_map.insert("size".into(), json!(entry.size));
        entry_map.insert("modified_time".into(), json!(entry.modified_time));
        entries.push(entry_map);
    }
    fd_json.insert("entries".into(), json!(entries));
    serde_json::to_string(&fd_json).unwrap_or("".into())
}
