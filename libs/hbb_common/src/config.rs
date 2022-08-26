use crate::log;
use directories_next::ProjectDirs;
use rand::Rng;
use serde_derive::{Deserialize, Serialize};
use sodiumoxide::crypto::sign;
use std::{
    collections::HashMap,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::SystemTime,
};

pub const RENDEZVOUS_TIMEOUT: u64 = 12_000;
pub const CONNECT_TIMEOUT: u64 = 18_000;
pub const REG_INTERVAL: i64 = 12_000;
pub const COMPRESS_LEVEL: i32 = 3;
const SERIAL: i32 = 1;
// 128x128
#[cfg(target_os = "macos")] // 128x128 on 160x160 canvas, then shrink to 128, mac looks better with padding
pub const ICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAACACAMAAAD04JH5AAAABGdBTUEAAK/INwWK6QAAABl0RVh0U29mdHdhcmUAQWRvYmUgSW1hZ2VSZWFkeXHJZTwAAAMAUExURTI2QK+vroxxJ+2uBOapB6ChonNiMCw1SllROjs/STE4SE5SWTY6R6urq01JPmttclJVXG1eMoSGif+6AEFCQ6WAHvm2AEFETUVEQUlMVOutBeCmCZKSlHllLt6lCTI4R2JWNoJrKpl5IjA3SXt9gWZZNcGSEzc8RoVtKtqiC9WeDZSVlzc8RXZjL/SyAWdpblJMPLGIGjY6Q46OkMmYEK6GG1VOO76QFZmZmvKxAqN/HzxASZCRk/+9AG5wdPazAc2aD2JkavGwA+2uA9KdDkVIUKmpqOiqBjM4QKSkpTxASnJ0eDQ5SPKwAjo+Ri0xOD5ARFlcYl5hZzk9Rjs+RMSUEraLGLqOFnl7f7iMF6uFHH9oLLWLGDM4QeKoBzg+Rp59IFdaYTU6Q5J1JNegDIpwKHxnLWpdM+KnCNihC15UOGBkaTw/RLqOFWdaND5CSvWyAvKyAVhbYZZ3I36Ag0dLUzg7Rc2bEFtSOTg8Rj5CTFpdYz1BSjg8RZV3JIdvKYBpLEtIP0dGQTg9RTg9RjU5R/i1APe0ADk9RTk8Rve1APi0ADk8Rf24APu3AP+5ADc7RDI5SP65APq2APm1ADM5SDI4SPy3ADU5QjU6Rzc7RTY7RDQ4QjY6RDQ5QjY7RzU5QzQ6R/64APu2AP23AKysrPy4APq3AK2trTU7Rjc6RP+4AIyNj3BfMTg8RK2trK6uraqqqfa0AHV3e/25AEpHPzU6RsaVEjY7RsWWEmNma2NmbJOUloeJizo9RUNHT5+fn/m0APu4APCvA56foKOkpJeYmjo9RqCgoY+Rkj1CSjU6RK2srK6trbmOF3Fzd6Wlpqenp0VKUXZ4e7yPFqOjpEZJUqF+IOqsBjY5Q1ZZYK+HGtyjDIpvJ+OoB7aMGFdQO15VN2hbNHV3fMeXEjc6RaeCHfq1ANCbD/a0Afe0AU9LPVteZDU5RPi0ATU4Qo9zJpd4I/azAsiWETE4Sfy2ADI6SDk9R0VJUUZKUemsBemrBtmiCz9DTMeVEQAAAL0aAz0AAAEAdFJOU////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////wBT9wclAAASB0lEQVR42mL4P8AAIIAYBtoBAAFEgQOsOUEQCsg2BSCAyHGA//GO5g6dzmYw7KiFgAl+fn5eE0g3DCCASHaAX40fZwsIdEJhMBLw4+ysIdE8gAAiyQF+frXNIAgGEGZtBybwIyUgAAKIBAcEB1dCIRjAOVAGGECYfpxeRJsKEEAMRNsO8m5LbTAIggGEWdvSGQyCUKs7QbClp6Onw4/IuAAIIAYirYd7mJgQgEDOKmKMBgggBuKsB3m2EuZpgiEAE6olIhQAAoiwA/yC0fxPbAgAhWq/ixMyHiCACDoguAXq3c5akkMABNcRMB8ggAg4wLoSkucqUSCyUEdHLRIEpj4QRhIK/u6H1waAAMLvAD+UYgYraIFRyBAi1AnhVHbiswIggBjwlztAgOJDSLGLIlAJhWBQiQLhQniKBYAAwuMAYPw2UxQCCFgbjNMWgADC7YBYqCeoEQKVOqdxWQMQQDgdENvc0ky1EADCWBz2AAQQLgfMCa6kZgjUVrbMwW4RQADhcIBObXAzGOIGHbBCAepHjBAIRgsUHaw2AQQQVgd4TUDyBK4QqERzEIEQAMIJ2IpFgADC6oAJiEIPtdhDFsKWHlCjHdxuQebOxmIXQABhc8AMFE9gDQF8ybISd7KYh2kZQABhccACLN4lLgSC0SIfLQSAAkkYtgEEEKYDGJorCYRAMBEAV8bAsA8ggBgI+B9LCBAJsIUAUAi9agIIIHQHTJjdUosMgyE+D4ZBou0Hu6Gntgfo606Q33sgsAU9KwAEELoDjLBHOEl+R84WLeCAQM4YaAkRIIDQHDAHpKUZCRKd4IiSBjNRi0SAAEJ1QO0skFIskY3F97CwwVICYncCLGPMQikSAQII1QFJzYgQICGxIXsR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
";
#[cfg(not(target_os = "macos"))] // 128x128 no padding
pub const ICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAACACAMAAAD04JH5AAAABGdBTUEAAK/INwWK6QAAABl0RVh0U29mdHdhcmUAQWRvYmUgSW1hZ2VSZWFkeXHJZTwAAAMAUExURTI2QK+vroxxJ+2uBOapB6ChonNiMCw1SllROjs/STE4SE5SWTY6R6urq01JPmttclJVXG1eMoSGif+6AEFCQ6WAHvm2AEFETUVEQUlMVOutBeCmCZKSlHllLt6lCTI4R2JWNoJrKpl5IjA3SXt9gWZZNcGSEzc8RoVtKtqiC9WeDZSVlzc8RXZjL/SyAWdpblJMPLGIGjY6Q46OkMmYEK6GG1VOO76QFZmZmvKxAqN/HzxASZCRk/+9AG5wdPazAc2aD2JkavGwA+2uA9KdDkVIUKmpqOiqBjM4QKSkpTxASnJ0eDQ5SPKwAjo+Ri0xOD5ARFlcYl5hZzk9Rjs+RMSUEraLGLqOFnl7f7iMF6uFHH9oLLWLGDM4QeKoBzg+Rp59IFdaYTU6Q5J1JNegDIpwKHxnLWpdM+KnCNihC15UOGBkaTw/RLqOFWdaND5CSvWyAvKyAVhbYZZ3I36Ag0dLUzg7Rc2bEFtSOTg8Rj5CTFpdYz1BSjg8RZV3JIdvKYBpLEtIP0dGQTg9RTg9RjU5R/i1APe0ADk9RTk8Rve1APi0ADk8Rf24APu3AP+5ADc7RDI5SP65APq2APm1ADM5SDI4SPy3ADU5QjU6Rzc7RTY7RDQ4QjY6RDQ5QjY7RzU5QzQ6R/64APu2AP23AKysrPy4APq3AK2trTU7Rjc6RP+4AIyNj3BfMTg8RK2trK6uraqqqfa0AHV3e/25AEpHPzU6RsaVEjY7RsWWEmNma2NmbJOUloeJizo9RUNHT5+fn/m0APu4APCvA56foKOkpJeYmjo9RqCgoY+Rkj1CSjU6RK2srK6trbmOF3Fzd6Wlpqenp0VKUXZ4e7yPFqOjpEZJUqF+IOqsBjY5Q1ZZYK+HGtyjDIpvJ+OoB7aMGFdQO15VN2hbNHV3fMeXEjc6RaeCHfq1ANCbD/a0Afe0AU9LPVteZDU5RPi0ATU4Qo9zJpd4I/azAsiWETE4Sfy2ADI6SDk9R0VJUUZKUemsBemrBtmiCz9DTMeVEQAAAL0aAz0AAAEAdFJOU////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////wBT9wclAAASB0lEQVR42mL4P8AAIIAYBtoBAAFEgQOsOUEQCsg2BSCAyHGA//GO5g6dzmYw7KiFgAl+fn5eE0g3DCCASHaAX40fZwsIdEJhMBLw4+ysIdE8gAAiyQF+frXNIAgGEGZtBybwIyUgAAKIBAcEB1dCIRjAOVAGGECYfpxeRJsKEEAMRNsO8m5LbTAIggGEWdvSGQyCUKs7QbClp6Onw4/IuAAIIAYirYd7mJgQgEDOKmKMBgggBuKsB3m2EuZpgiEAE6olIhQAAoiwA/yC0fxPbAgAhWq/ixMyHiCACDoguAXq3c5akkMABNcRMB8ggAg4wLoSkucqUSCyUEdHLRIEpj4QRhIK/u6H1waAAMLvAD+UYgYraIFRyBAi1AnhVHbiswIggBjwlztAgOJDSLGLIlAJhWBQiQLhQniKBYAAwuMAYPw2UxQCCFgbjNMWgADC7YBYqCeoEQKVOqdxWQMQQDgdENvc0ky1EADCWBz2AAQQLgfMCa6kZgjUVrbMwW4RQADhcIBObXAzGOIGHbBCAepHjBAIRgsUHaw2AQQQVgd4TUDyBK4QqERzEIEQAMIJ2IpFgADC6oAJiEIPtdhDFsKWHlCjHdxuQebOxmIXQABhc8AMFE9gDQF8ybISd7KYh2kZQABhccACLN4lLgSC0SIfLQSAAkkYtgEEEKYDGJorCYRAMBEAV8bAsA8ggBgI+B9LCBAJsIUAUAi9agIIIHQHTJjdUosMgyE+D4ZBou0Hu6Gntgfo606Q33sgsAU9KwAEELoDjLBHOEl+R84WLeCAQM4YaAkRIIDQHDAHpKUZCRKd4IiSBjNRi0SAAEJ1QO0skFIskY3F97CwwVICYncCLGPMQikSAQII1QFJzYgQICGxIXsR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
";

#[cfg(target_os = "macos")]
lazy_static::lazy_static! {
    pub static ref ORG: Arc<RwLock<String>> = Arc::new(RwLock::new("com.hoptodesk".to_owned()));
}

type Size = (i32, i32, i32, i32);

lazy_static::lazy_static! {
    static ref CONFIG: Arc<RwLock<Config>> = Arc::new(RwLock::new(Config::load()));
    static ref CONFIG2: Arc<RwLock<Config2>> = Arc::new(RwLock::new(Config2::load()));
    static ref LOCAL_CONFIG: Arc<RwLock<LocalConfig>> = Arc::new(RwLock::new(LocalConfig::load()));
    pub static ref ONLINE: Arc<Mutex<HashMap<String, i64>>> = Default::default();
    pub static ref APP_NAME: Arc<RwLock<String>> = Arc::new(RwLock::new("HopToDesk".to_owned()));
}
#[cfg(any(target_os = "android", target_os = "ios"))]
lazy_static::lazy_static! {
    pub static ref APP_DIR: Arc<RwLock<String>> = Default::default();
    pub static ref APP_HOME_DIR: Arc<RwLock<String>> = Default::default();
}
const CHARS: &'static [char] = &[
    '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k',
    'm', 'n', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

pub const RENDEZVOUS_PORT: i32 = 21116;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetworkType {
    Direct,
    ProxySocks,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    salt: String,
    #[serde(default)]
    pub key_pair: (Vec<u8>, Vec<u8>), // sk, pk
    #[serde(default)]
    key_confirmed: bool,
    #[serde(default)]
    keys_confirmed: HashMap<String, bool>,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Clone)]
pub struct Socks5Server {
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

// more variable configs
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Config2 {
    #[serde(default)]
    remote_id: String, // latest used one
    #[serde(default)]
    size: Size,
    #[serde(default)]
    rendezvous_server: String,
    #[serde(default)]
    nat_type: i32,
    #[serde(default)]
    serial: i32,

    #[serde(default)]
    socks: Option<Socks5Server>,

    // the other scalar value must before this
    #[serde(default)]
    pub options: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PeerConfig {
    #[serde(default)]
    pub password: Vec<u8>,
    #[serde(default)]
    pub size: Size,
    #[serde(default)]
    pub size_ft: Size,
    #[serde(default)]
    pub size_pf: Size,
    #[serde(default)]
    pub view_style: String, // original (default), scale
    #[serde(default)]
    pub image_quality: String,
    #[serde(default)]
    pub custom_image_quality: Vec<i32>,
    #[serde(default)]
    pub show_remote_cursor: bool,
    #[serde(default)]
    pub lock_after_session_end: bool,
    #[serde(default)]
    pub privacy_mode: bool,
    #[serde(default)]
    pub port_forwards: Vec<(i32, String, i32)>,
    #[serde(default)]
    pub direct_failures: i32,
    #[serde(default)]
    pub disable_audio: bool,
    #[serde(default)]
    pub disable_clipboard: bool,
    #[serde(default)]
    pub enable_file_transfer: bool,

    // the other scalar value must before this
    #[serde(default)]
    pub options: HashMap<String, String>,
    #[serde(default)]
    pub info: PeerInfoSerde,
    #[serde(default)]
    pub transfer: TransferSerde,
}

#[derive(Debug, PartialEq, Default, Serialize, Deserialize, Clone)]
pub struct PeerInfoSerde {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub mac_address: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct TransferSerde {
    #[serde(default)]
    pub write_jobs: Vec<String>,
    #[serde(default)]
    pub read_jobs: Vec<String>,
}

fn patch(path: PathBuf) -> PathBuf {
    if let Some(_tmp) = path.to_str() {
        #[cfg(windows)]
        return _tmp
            .replace(
                "system32\\config\\systemprofile",
                "ServiceProfiles\\LocalService",
            )
            .into();
        #[cfg(target_os = "macos")]
        return _tmp.replace("Application Support", "Preferences").into();
        #[cfg(target_os = "linux")]
        {
            if _tmp == "/root" {
                if let Ok(output) = std::process::Command::new("whoami").output() {
                    let user = String::from_utf8_lossy(&output.stdout)
                        .to_string()
                        .trim()
                        .to_owned();
                    if user != "root" {
                        return format!("/home/{}", user).into();
                    }
                }
            }
        }
    }
    path
}

impl Config2 {
    fn load() -> Config2 {
        Config::load_::<Config2>("2")
    }

    pub fn file() -> PathBuf {
        Config::file_("2")
    }

    fn store(&self) {
        Config::store_(self, "2");
    }

    pub fn get() -> Config2 {
        return CONFIG2.read().unwrap().clone();
    }

    pub fn set(cfg: Config2) -> bool {
        let mut lock = CONFIG2.write().unwrap();
        if *lock == cfg {
            return false;
        }
        *lock = cfg;
        lock.store();
        true
    }
}

pub fn load_path<T: serde::Serialize + serde::de::DeserializeOwned + Default + std::fmt::Debug>(
    file: PathBuf,
) -> T {
    let cfg = match confy::load_path(&file) {
        Ok(config) => config,
        Err(err) => {
            log::error!("Failed to load config: {}", err);
            T::default()
        }
    };
    cfg
}

impl Config {
    fn load_<T: serde::Serialize + serde::de::DeserializeOwned + Default + std::fmt::Debug>(
        suffix: &str,
    ) -> T {
        let file = Self::file_(suffix);
        log::debug!("Configuration path: {}", file.display());
        let cfg = match confy::load_path(&file) {
            Ok(config) => config,
            Err(err) => {
                log::error!("Failed to load config: {}", err);
                T::default()
            }
        };
        if suffix.is_empty() {
            log::debug!("{:?}", cfg);
        }
        cfg
    }

    fn store_<T: serde::Serialize>(config: &T, suffix: &str) {
        let file = Self::file_(suffix);
        if let Err(err) = confy::store_path(file, config) {
            log::error!("Failed to store config: {}", err);
        }
    }

    fn load() -> Config {
        Config::load_::<Config>("")
    }

    fn store(&self) {
        Config::store_(self, "");
    }

    pub fn file() -> PathBuf {
        Self::file_("")
    }

    fn file_(suffix: &str) -> PathBuf {
        let name = format!("{}{}", *APP_NAME.read().unwrap(), suffix);
        Self::path(name).with_extension("toml")
    }

    pub fn get_home() -> PathBuf {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        return Self::path(APP_HOME_DIR.read().unwrap().as_str());
        if let Some(path) = dirs_next::home_dir() {
            patch(path)
        } else if let Ok(path) = std::env::current_dir() {
            path
        } else {
            std::env::temp_dir()
        }
    }

    pub fn path<P: AsRef<Path>>(p: P) -> PathBuf {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let mut path: PathBuf = APP_DIR.read().unwrap().clone().into();
            path.push(p);
            return path;
        }
        #[cfg(not(target_os = "macos"))]
        let org = "";
        #[cfg(target_os = "macos")]
        let org = ORG.read().unwrap().clone();
        // /var/root for root
        if let Some(project) = ProjectDirs::from("", &org, &*APP_NAME.read().unwrap()) {
            let mut path = patch(project.config_dir().to_path_buf());
            path.push(p);
            return path;
        }
        return "".into();
    }

    #[allow(unreachable_code)]
    pub fn log_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(path) = dirs_next::home_dir().as_mut() {
                path.push(format!("Library/Logs/{}", *APP_NAME.read().unwrap()));
                return path.clone();
            }
        }
        #[cfg(target_os = "linux")]
        {
            let mut path = Self::get_home();
            path.push(format!(".local/share/logs/{}", *APP_NAME.read().unwrap()));
            std::fs::create_dir_all(&path).ok();
            return path;
        }
        if let Some(path) = Self::path("").parent() {
            let mut path: PathBuf = path.into();
            path.push("log");
            return path;
        }
        "".into()
    }

    pub fn ipc_path(postfix: &str) -> String {
        #[cfg(windows)]
        {
            // \\ServerName\pipe\PipeName
            // where ServerName is either the name of a remote computer or a period, to specify the local computer.
            // https://docs.microsoft.com/en-us/windows/win32/ipc/pipe-names
            format!(
                "\\\\.\\pipe\\{}\\query{}",
                *APP_NAME.read().unwrap(),
                postfix
            )
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            #[cfg(target_os = "android")]
            let mut path: PathBuf =
                format!("{}/{}", *APP_DIR.read().unwrap(), *APP_NAME.read().unwrap()).into();
            #[cfg(not(target_os = "android"))]
            let mut path: PathBuf = format!("/tmp/{}", *APP_NAME.read().unwrap()).into();
            fs::create_dir(&path).ok();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o0777)).ok();
            path.push(format!("ipc{}", postfix));
            path.to_str().unwrap_or("").to_owned()
        }
    }

    pub fn icon_path() -> PathBuf {
        let mut path = Self::path("icons");
        if fs::create_dir_all(&path).is_err() {
            path = std::env::temp_dir();
        }
        path
    }

    #[inline]
    pub fn get_any_listen_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
    }

    pub async fn get_rendezvous_server() -> Option<String> {
        let mut rendezvous_server = Self::get_option("custom-rendezvous-server");
        if rendezvous_server.is_empty() {
            rendezvous_server = CONFIG2.write().unwrap().rendezvous_server.clone();
        }
        if rendezvous_server.is_empty() {
            rendezvous_server = Self::get_rendezvous_servers()
                .await?
                .drain(..)
                .next()
                .unwrap_or("".to_owned());
        }
        Some(rendezvous_server)
    }

    pub async fn get_rendezvous_servers() -> Option<Vec<String>> {
        let s = Self::get_option("custom-rendezvous-server");
        if !s.is_empty() {
            return Some(vec![s]);
        }
        let serial_obsolute = CONFIG2.read().unwrap().serial > SERIAL;
        if serial_obsolute {
            let ss: Vec<String> = Self::get_option("rendezvous-servers")
                .split(",")
                .filter(|x| x.contains("."))
                .map(|x| x.to_owned())
                .collect();
            if !ss.is_empty() {
                return Some(ss);
            }
        }
        let map = crate::api::call_api().await.ok()?;
        let server_list = vec![
            ("wss".to_string(), map["rendezvousssl"].as_object()),
            ("ws".to_string(), map["rendezvous"].as_object()),
        ];

        Some(vec![server_list
            .iter()
            .flat_map(|(scheme, obj)| {
                if let Some(obj) = obj {
                    Some(format!(
                        "{}://{}:{}",
                        scheme,
                        obj["host"].as_str()?,
                        obj["port"].as_str()?
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
            .join(";")])
    }

    pub fn reset_online() {
        *ONLINE.lock().unwrap() = Default::default();
    }

    pub fn update_latency(host: &str, latency: i64) {
        ONLINE.lock().unwrap().insert(host.to_owned(), latency);
        let mut host = "".to_owned();
        let mut delay = i64::MAX;
        for (tmp_host, tmp_delay) in ONLINE.lock().unwrap().iter() {
            if tmp_delay > &0 && tmp_delay < &delay {
                delay = tmp_delay.clone();
                host = tmp_host.to_string();
            }
        }
        if !host.is_empty() {
            let mut config = CONFIG2.write().unwrap();
            if host != config.rendezvous_server {
                log::debug!("Update rendezvous_server in config to {}", host);
                log::debug!("{:?}", *ONLINE.lock().unwrap());
                config.rendezvous_server = host;
                config.store();
            }
        }
    }

    pub fn set_id(id: &str) {
        let mut config = CONFIG.write().unwrap();
        if id == config.id {
            return;
        }
        config.id = id.into();
        config.store();
    }

    pub fn set_nat_type(nat_type: i32) {
        let mut config = CONFIG2.write().unwrap();
        if nat_type == config.nat_type {
            return;
        }
        config.nat_type = nat_type;
        config.store();
    }

    pub fn get_nat_type() -> i32 {
        CONFIG2.read().unwrap().nat_type
    }

    pub fn set_serial(serial: i32) {
        let mut config = CONFIG2.write().unwrap();
        if serial == config.serial {
            return;
        }
        config.serial = serial;
        config.store();
    }

    pub fn get_serial() -> i32 {
        std::cmp::max(CONFIG2.read().unwrap().serial, SERIAL)
    }

    fn get_auto_id() -> Option<String> {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            return Some(
                rand::thread_rng()
                    .gen_range(1_000_000_000..2_000_000_000)
                    .to_string(),
            );
        }
        let mut id = 0u32;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Ok(Some(ma)) = mac_address::get_mac_address() {
            for x in &ma.bytes()[2..] {
                id = (id << 8) | (*x as u32);
            }
            id = id & 0x1FFFFFFF;
            if id < 1_000_000_000 {
                id = rand::thread_rng().gen_range(100_000_000..200_000_000)
            }
            Some(id.to_string())
        } else {
            None
        }
    }

    pub fn get_auto_password() -> String {
        let mut rng = rand::thread_rng();
        (0..6)
            .map(|_| CHARS[rng.gen::<usize>() % CHARS.len()])
            .collect()
    }

    pub fn get_key_confirmed() -> bool {
        CONFIG.read().unwrap().key_confirmed
    }

    pub fn set_key_confirmed(v: bool) {
        let mut config = CONFIG.write().unwrap();
        if config.key_confirmed == v {
            return;
        }
        config.key_confirmed = v;
        if !v {
            config.keys_confirmed = Default::default();
        }
        config.store();
    }

    pub fn get_host_key_confirmed(host: &str) -> bool {
        if let Some(true) = CONFIG.read().unwrap().keys_confirmed.get(host) {
            true
        } else {
            false
        }
    }

    pub fn set_host_key_confirmed(host: &str, v: bool) {
        if Self::get_host_key_confirmed(host) == v {
            return;
        }
        let mut config = CONFIG.write().unwrap();
        config.keys_confirmed.insert(host.to_owned(), v);
        config.store();
    }

    pub fn set_key_pair(pair: (Vec<u8>, Vec<u8>)) {
        let mut config = CONFIG.write().unwrap();
        if config.key_pair == pair {
            return;
        }
        config.key_pair = pair;
        config.store();
    }

    pub fn get_key_pair() -> (Vec<u8>, Vec<u8>) {
        // lock here to make sure no gen_keypair more than once
        let mut config = CONFIG.write().unwrap();
        if config.key_pair.0.is_empty() {
            let (pk, sk) = sign::gen_keypair();
            config.key_pair = (sk.0.to_vec(), pk.0.into());
            config.store();
        }
        config.key_pair.clone()
    }

    pub fn get_id() -> String {
        let mut id = CONFIG.read().unwrap().id.clone();
        if id.is_empty() {
            if let Some(tmp) = Config::get_auto_id() {
                id = tmp;
                Config::set_id(&id);
            }
        }
        id
    }

    pub fn get_id_or(b: String) -> String {
        let a = CONFIG.read().unwrap().id.clone();
        if a.is_empty() {
            b
        } else {
            a
        }
    }

    pub fn get_options() -> HashMap<String, String> {
        CONFIG2.read().unwrap().options.clone()
    }

    pub fn set_options(v: HashMap<String, String>) {
        let mut config = CONFIG2.write().unwrap();
        if config.options == v {
            return;
        }
        config.options = v;
        config.store();
    }

    pub fn get_option(k: &str) -> String {
        if let Some(v) = CONFIG2.read().unwrap().options.get(k) {
            v.clone()
        } else {
            "".to_owned()
        }
    }

    pub fn set_option(k: String, v: String) {
        let mut config = CONFIG2.write().unwrap();
        let v2 = if v.is_empty() { None } else { Some(&v) };
        if v2 != config.options.get(&k) {
            if v2.is_none() {
                config.options.remove(&k);
            } else {
                config.options.insert(k, v);
            }
            config.store();
        }
    }

    pub fn update_id() {
        // to-do: how about if one ip register a lot of ids?
        let id = Self::get_id();
        let mut rng = rand::thread_rng();
        let new_id = rng.gen_range(1_000_000_000..2_000_000_000).to_string();
        Config::set_id(&new_id);
        log::info!("id updated from {} to {}", id, new_id);
    }

    pub fn set_password(password: &str) {
        let mut config = CONFIG.write().unwrap();
        if password == config.password {
            return;
        }
        config.password = password.into();
        config.store();
    }

    pub fn get_password() -> String {
        let mut password = CONFIG.read().unwrap().password.clone();
        if password.is_empty() {
            password = Config::get_auto_password();
            Config::set_password(&password);
        }
        password
    }

    pub fn set_salt(salt: &str) {
        let mut config = CONFIG.write().unwrap();
        if salt == config.salt {
            return;
        }
        config.salt = salt.into();
        config.store();
    }

    pub fn get_salt() -> String {
        let mut salt = CONFIG.read().unwrap().salt.clone();
        if salt.is_empty() {
            salt = Config::get_auto_password();
            Config::set_salt(&salt);
        }
        salt
    }

    pub fn set_socks(socks: Option<Socks5Server>) {
        let mut config = CONFIG2.write().unwrap();
        if config.socks == socks {
            return;
        }
        config.socks = socks;
        config.store();
    }

    pub fn get_socks() -> Option<Socks5Server> {
        CONFIG2.read().unwrap().socks.clone()
    }

    pub fn get_network_type() -> NetworkType {
        match &CONFIG2.read().unwrap().socks {
            None => NetworkType::Direct,
            Some(_) => NetworkType::ProxySocks,
        }
    }

    pub fn get() -> Config {
        return CONFIG.read().unwrap().clone();
    }

    pub fn set(cfg: Config) -> bool {
        let mut lock = CONFIG.write().unwrap();
        if *lock == cfg {
            return false;
        }
        *lock = cfg;
        lock.store();
        true
    }

    fn with_extension(path: PathBuf) -> PathBuf {
        let ext = path.extension();
        if let Some(ext) = ext {
            let ext = format!("{}.toml", ext.to_string_lossy());
            path.with_extension(&ext)
        } else {
            path.with_extension("toml")
        }
    }
}

const PEERS: &str = "peers";

impl PeerConfig {
    pub fn load(id: &str) -> PeerConfig {
        let _ = CONFIG.read().unwrap(); // for lock
        match confy::load_path(&Self::path(id)) {
            Ok(config) => config,
            Err(err) => {
                log::error!("Failed to load config: {}", err);
                Default::default()
            }
        }
    }

    pub fn store(&self, id: &str) {
        let _ = CONFIG.read().unwrap(); // for lock
        if let Err(err) = confy::store_path(Self::path(id), self) {
            log::error!("Failed to store config: {}", err);
        }
    }

    pub fn remove(id: &str) {
        fs::remove_file(&Self::path(id)).ok();
    }

    fn path(id: &str) -> PathBuf {
        let path: PathBuf = [PEERS, id].iter().collect();
        Config::with_extension(Config::path(path))
    }

    pub fn peers() -> Vec<(String, SystemTime, PeerConfig)> {
        if let Ok(peers) = Config::path(PEERS).read_dir() {
            if let Ok(peers) = peers
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
            {
                let mut peers: Vec<_> = peers
                    .iter()
                    .filter(|p| {
                        p.is_file()
                            && p.extension().map(|p| p.to_str().unwrap_or("")) == Some("toml")
                    })
                    .map(|p| {
                        let t = crate::get_modified_time(&p);
                        let id = p
                            .file_stem()
                            .map(|p| p.to_str().unwrap_or(""))
                            .unwrap_or("")
                            .to_owned();
                        let c = PeerConfig::load(&id);
                        if c.info.platform.is_empty() {
                            fs::remove_file(&p).ok();
                        }
                        (id, t, c)
                    })
                    .filter(|p| !p.2.info.platform.is_empty())
                    .collect();
                peers.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                return peers;
            }
        }
        Default::default()
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct LocalConfig {
    #[serde(default)]
    remote_id: String, // latest used one
    #[serde(default)]
    size: Size,
    #[serde(default)]
    pub fav: Vec<String>,
    #[serde(default)]
    options: HashMap<String, String>,
}

impl LocalConfig {
    fn load() -> LocalConfig {
        Config::load_::<LocalConfig>("_local")
    }

    fn store(&self) {
        Config::store_(self, "_local");
    }

    pub fn get_size() -> Size {
        LOCAL_CONFIG.read().unwrap().size
    }

    pub fn set_size(x: i32, y: i32, w: i32, h: i32) {
        let mut config = LOCAL_CONFIG.write().unwrap();
        let size = (x, y, w, h);
        if size == config.size || size.2 < 300 || size.3 < 300 {
            return;
        }
        config.size = size;
        config.store();
    }

    pub fn set_remote_id(remote_id: &str) {
        let mut config = LOCAL_CONFIG.write().unwrap();
        if remote_id == config.remote_id {
            return;
        }
        config.remote_id = remote_id.into();
        config.store();
    }

    pub fn get_remote_id() -> String {
        LOCAL_CONFIG.read().unwrap().remote_id.clone()
    }

    pub fn set_fav(fav: Vec<String>) {
        let mut lock = LOCAL_CONFIG.write().unwrap();
        if lock.fav == fav {
            return;
        }
        lock.fav = fav;
        lock.store();
    }

    pub fn get_fav() -> Vec<String> {
        LOCAL_CONFIG.read().unwrap().fav.clone()
    }

    pub fn get_option(k: &str) -> String {
        if let Some(v) = LOCAL_CONFIG.read().unwrap().options.get(k) {
            v.clone()
        } else {
            "".to_owned()
        }
    }

    pub fn set_option(k: String, v: String) {
        let mut config = LOCAL_CONFIG.write().unwrap();
        let v2 = if v.is_empty() { None } else { Some(&v) };
        if v2 != config.options.get(&k) {
            if v2.is_none() {
                config.options.remove(&k);
            } else {
                config.options.insert(k, v);
            }
            config.store();
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct LanPeers {
    #[serde(default)]
    pub peers: String,
}

impl LanPeers {
    pub fn load() -> LanPeers {
        let _ = CONFIG.read().unwrap(); // for lock
        match confy::load_path(&Config::file_("_lan_peers")) {
            Ok(peers) => peers,
            Err(err) => {
                log::error!("Failed to load lan peers: {}", err);
                Default::default()
            }
        }
    }

    pub fn store(peers: String) {
        let f = LanPeers { peers };
        if let Err(err) = confy::store_path(Config::file_("_lan_peers"), f) {
            log::error!("Failed to store lan peers: {}", err);
        }
    }

    pub fn modify_time() -> crate::ResultType<u64> {
        let p = Config::file_("_lan_peers");
        Ok(fs::metadata(p)?
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_serialize() {
        let cfg: Config = Default::default();
        let res = toml::to_string_pretty(&cfg);
        assert!(res.is_ok());
        let cfg: PeerConfig = Default::default();
        let res = toml::to_string_pretty(&cfg);
        assert!(res.is_ok());
    }
}
