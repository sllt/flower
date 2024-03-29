pub mod crypto;
pub mod mutex;
pub mod net;
pub mod resolver;
pub mod sniff;
pub mod process;


#[cfg(target_os = "macos")]
pub mod cmd_macos;
#[cfg(target_os = "macos")]
pub use cmd_macos as cmd;

#[cfg(target_os = "linux")]
pub mod cmd_linux;

#[cfg(target_os = "linux")]
pub use cmd_linux as cmd;
