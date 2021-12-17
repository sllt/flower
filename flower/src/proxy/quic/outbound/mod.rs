mod tcp;
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
pub use tcp::Handler as TcpHandler;

use super::QuicProxyStream;
