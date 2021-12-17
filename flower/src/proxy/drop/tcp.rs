use std::io;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::{proxy::*, session::Session};

pub struct Handler;

#[async_trait]
impl TcpOutboundHandler for Handler {
    type Stream = AnyStream;

    fn connect_addr(&self) -> Option<OutboundConnect> {
        None
    }

    async fn handle<'a>(
        &'a self,
        _sess: &'a Session,
        stream: Option<Self::Stream>,
    ) -> io::Result<Self::Stream> {
        Err(io::Error::new(io::ErrorKind::Other, "dropped"))
    }
}
