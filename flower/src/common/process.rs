#[cfg(target_os = "windows")]
mod process_windows;
#[cfg(target_os = "linux")]
mod process_linux;
#[cfg(target_os = "macos")]
mod process_darwin;

use std::process::Command;
use crate::session::Network;

#[cfg(any(target_os = "macos"))]
pub fn get_command_name_by_socket(network: Network, addr: &str, port: u16) -> Option<String> {
    let pattern = match network {
        Network::Tcp => {
            format!("-i{}@{}:{}", "tcp", addr, port)
        }
        _ => {
            format!("-i{}:{}", "udp", port)
        }
    };
    let mut lsof = std::process::Command::new("lsof");
    lsof.arg("-c ^flower").arg("-n").arg("-P").arg("-Fc").arg(pattern);
    let out = lsof.output().expect("failed to execute process");
    let out_str  = String::from_utf8(out.stdout).as_ref().unwrap().clone();
    for line in out_str.split("\n").collect::<Vec<&str>>() {
        if line.len() > 0 && line.chars().nth(0).unwrap() == 'c' {
             return Some(line.split_at(1).1.to_owned());
        }
    }
    return None;
}
