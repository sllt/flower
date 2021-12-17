use jni::{
    objects::{JClass, JString},
    JNIEnv,
};

/// No error.
pub const ERR_OK: i32 = 0;
/// Config path error.
pub const ERR_CONFIG_PATH: i32 = 1;
/// Config parsing error.
pub const ERR_CONFIG: i32 = 2;
/// IO error.
pub const ERR_IO: i32 = 3;
/// Config file watcher error.
pub const ERR_WATCHER: i32 = 4;
/// Async channel send error.
pub const ERR_ASYNC_CHANNEL_SEND: i32 = 5;
/// Sync channel receive error.
pub const ERR_SYNC_CHANNEL_RECV: i32 = 6;
/// Runtime manager error.
pub const ERR_RUNTIME_MANAGER: i32 = 7;
/// No associated config file.
pub const ERR_NO_CONFIG_FILE: i32 = 8;

fn to_errno(e: flower::Error) -> i32 {
    match e {
        flower::Error::Config(..) => ERR_CONFIG,
        flower::Error::NoConfigFile => ERR_NO_CONFIG_FILE,
        flower::Error::Io(..) => ERR_IO,
        #[cfg(feature = "auto-reload")]
        flower::Error::Watcher(..) => ERR_WATCHER,
        flower::Error::AsyncChannelSend(..) => ERR_ASYNC_CHANNEL_SEND,
        flower::Error::SyncChannelRecv(..) => ERR_SYNC_CHANNEL_RECV,
        flower::Error::RuntimeManager => ERR_RUNTIME_MANAGER,
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn Java_com_sllt_app_flower_SimpleVpnService_runFlower(
    env: JNIEnv,
    _: JClass,
    config_path: JString,
    protect_path: JString,
) -> i32 {
    let config_path = env
        .get_string(config_path)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let protect_path = env
        .get_string(protect_path)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    std::env::set_var("SOCKET_PROTECT_PATH", protect_path);

    let a = std::env::var("SOCKET_PROTECT_PATH").unwrap();
    println!("{}", a);
    println!("{}", "Hello World");

    let opts = flower::StartOptions {
        config: flower::Config::File(config_path),
        #[cfg(feature = "auto-reload")]
        auto_reload: false,
        runtime_opt: flower::RuntimeOption::SingleThread,
    };
    if let Err(e) = flower::start(0, opts) {
        return to_errno(e);
    } else {
        0
    }
}
