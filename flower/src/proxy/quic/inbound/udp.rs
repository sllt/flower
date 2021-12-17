use std::{
    ascii, fs, io, pin::Pin,
    net::SocketAddr,
    path::{self, Path, PathBuf},
    str,
    sync::Arc,
};
use std::str::FromStr;
use anyhow::{anyhow, Context};

use async_trait::async_trait;
use futures::stream::Stream;
use futures::{
    task::{Context as TaskContext, Poll},
    Future,
};
use quinn_proto::EndpointConfig;

use crate::{proxy::*, session::Session};

use super::QuicProxyStream;

struct Incoming {
    inner: quinn::Incoming,
    connectings: Vec<quinn::Connecting>,
    new_conns: Vec<quinn::NewConnection>,
    incoming_closed: bool,
}

impl Incoming {
    pub fn new(inner: quinn::Incoming) -> Self {
        Incoming {
            inner,
            connectings: Vec::new(),
            new_conns: Vec::new(),
            incoming_closed: false,
        }
    }
}

impl Stream for Incoming {
    type Item = AnyBaseInboundTransport;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        // FIXME don't iterate and poll all

        if !self.incoming_closed {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(connecting)) => {
                    self.connectings.push(connecting);
                }
                Poll::Ready(None) => {
                    self.incoming_closed = true;
                }
                Poll::Pending => (),
            }
        }

        let mut new_conns = Vec::new();
        let mut completed = Vec::new();
        for (idx, connecting) in self.connectings.iter_mut().enumerate() {
            match Pin::new(connecting).poll(cx) {
                Poll::Ready(Ok(new_conn)) => {
                    new_conns.push(new_conn);
                    completed.push(idx);
                }
                Poll::Ready(Err(e)) => {
                    log::debug!("quic connect failed: {}", e);
                    completed.push(idx);
                }
                Poll::Pending => (),
            }
        }
        if !new_conns.is_empty() {
            self.new_conns.append(&mut new_conns);
        }
        for idx in completed.iter().rev() {
            self.connectings.swap_remove(*idx);
        }

        let mut stream: Option<Self::Item> = None;
        let mut completed = Vec::new();
        for (idx, new_conn) in self.new_conns.iter_mut().enumerate() {
            match Pin::new(&mut new_conn.bi_streams).poll_next(cx) {
                Poll::Ready(Some(Ok((send, recv)))) => {
                    let mut sess = Session {
                        source: new_conn.connection.remote_address(),
                        ..Default::default()
                    };
                    // TODO Check whether the index suitable for this purpose.
                    sess.stream_id = Some(send.id().index());
                    stream.replace(AnyBaseInboundTransport::Stream(
                        Box::new(QuicProxyStream { recv, send }),
                        sess,
                    ));
                    break;
                }
                Poll::Ready(Some(Err(e))) => {
                    log::debug!("new quic bidirectional stream failed: {}", e);
                    completed.push(idx);
                }
                Poll::Ready(None) => {
                    // FIXME what?
                    log::warn!("quic bidirectional stream exhausted");
                    completed.push(idx);
                }
                Poll::Pending => (),
            }
        }
        for idx in completed.iter().rev() {
            self.new_conns.remove(*idx);
        }

        if let Some(stream) = stream.take() {
            Poll::Ready(Some(stream))
        } else if self.incoming_closed && self.connectings.is_empty() && self.new_conns.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

fn quic_err<E>(error: E) -> io::Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    io::Error::new(io::ErrorKind::Other, error)
}

pub struct Handler {
    certificate: String,
    certificate_key: String,
}

impl Handler {
    pub fn new(certificate: String, certificate_key: String) -> Self {
        Self {
            certificate,
            certificate_key,
        }
    }
}

#[async_trait]
impl UdpInboundHandler for Handler {
    type UStream = AnyStream;
    type UDatagram = AnyInboundDatagram;

    async fn handle<'a>(
        &'a self,
        socket: Self::UDatagram,
    ) -> io::Result<InboundTransport<Self::UStream, Self::UDatagram>> {
        let (cert, key) =
            fs::read(&self.certificate).and_then(|x| Ok((x, fs::read(&self.certificate_key)?)))?;

        let (certs, key) =  {
            let key = fs::read(&self.certificate_key).context("failed to read private key").unwrap();
            let key = if Path::new(&self.certificate_key).extension().map_or(false, |x| x == "der") {
                rustls::PrivateKey(key)
            } else {
                let pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut &*key).unwrap();
                match pkcs8.into_iter().next() {
                    Some(x) => rustls::PrivateKey(x),
                    None => {
                        let rsa = rustls_pemfile::rsa_private_keys(&mut &*key)
                            .context("malformed PKCS #1 private key").unwrap();
                        if let Some(x) = rsa.into_iter().next() {
                             rustls::PrivateKey(x)
                        } else {
                            rustls::PrivateKey(Vec::new()) // FIXME return errors
                        }
                    }
                }
            };
            let cert_chain = fs::read(&self.certificate).context("failed to read certificate chain").unwrap();
            let cert_chain = if Path::new(&self.certificate).extension().map_or(false, |x| x == "der") {
                vec![rustls::Certificate(cert_chain)]
            } else {
                rustls_pemfile::certs(&mut &*cert_chain)
                    .context("invalid PEM-encoded certificate")
                    .unwrap()
                    .into_iter()
                    .map(rustls::Certificate)
                    .collect()
            };

            (cert_chain, key)
        };

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
        // server_crypto.alpn_protocols = common::ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        let mut transport_config = quinn::TransportConfig::default();
        transport_config
            .max_concurrent_uni_streams(0_u8.into())
            .max_idle_timeout(Some(std::time::Duration::from_secs(300).try_into().unwrap()));
        server_config.transport = Arc::new(transport_config);

        let (endpoint, mut incoming) = quinn::Endpoint::new(EndpointConfig::default(),
                                                            Some(server_config),
                                                            socket.into_std().unwrap())?;

        debug!("listening on: {}",endpoint.local_addr()?);
        Ok(InboundTransport::Incoming(Box::new(Incoming::new(
            incoming,
        ))))
    }
}
