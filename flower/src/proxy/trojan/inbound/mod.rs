mod tcp;

use std::io;
pub use tcp::Handler as TcpHandler;
use crate::proxy::ProxyStream;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, split};
use log::*;

async fn copy_tcp<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    r: &mut R,
    w: &mut W,
) -> io::Result<()> {
    let mut buf = [0u8; 0x4000];
    loop {
        let len = r.read(&mut buf).await?;
        if len == 0 {
            break;
        }
        w.write(&buf[..len]).await?;
        w.flush().await?;
    }
    Ok(())
}

pub async fn relay_tcp<T: ProxyStream, U: ProxyStream>(a: T, b: U) {
    let (mut a_rx, mut a_tx) = split(a);
    let (mut b_rx, mut b_tx) = split(b);
    let t1 = copy_tcp(&mut a_rx, &mut b_tx);
    let t2 = copy_tcp(&mut b_rx, &mut a_tx);
    let e = tokio::select! {
        e = t1 => {e}
        e = t2 => {e}
    };
    if let Err(e) = e {
        debug!("relay_tcp err: {}", e)
    }
    let mut a = a_rx.unsplit(a_tx);
    let mut b = b_rx.unsplit(b_tx);
    let _ = a.shutdown().await;
    let _ = b.shutdown().await;
    info!("tcp session ends");
}
