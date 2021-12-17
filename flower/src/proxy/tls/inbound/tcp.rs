use std::collections::hash_map::Keys;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use anyhow::Result;
#[cfg(feature = "openssl-tls")]
use openssl::ssl::{Ssl, SslMethod, SslAcceptor, SslFiletype};
#[cfg(feature = "openssl-tls")]
use tokio_openssl::SslStream;

#[cfg(feature = "rustls-tls")]
use {
    rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys},
    tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig},
    tokio_rustls::TlsAcceptor,
    tokio_rustls::rustls::server::NoClientAuth,
};

use crate::{proxy::*, session::Session};

pub struct Handler {
    #[cfg(feature = "rustls-tls")]
    acceptor: TlsAcceptor,
    #[cfg(feature = "openssl-tls")]
    ssl_acceptor: Arc<SslAcceptor>,
}

#[cfg(feature = "rustls-tls")]
fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    let bufs = certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .unwrap();
    let mut certs = Vec::<Certificate>::new();
    for buf in bufs {
        certs.push(Certificate(buf))
    }

    return Ok(certs)
}

#[cfg(feature = "rustls-tls")]
fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    let mut keys = pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))?;
    let mut keys2 = rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))?;
    keys.append(&mut keys2);
    let mut results = Vec::<PrivateKey>::new();
    for key in keys {
        results.push(PrivateKey(key))
    }
    Ok(results)
}

impl Handler {
    pub fn new(certificate: String, certificate_key: String) -> Result<Self> {
        #[cfg(feature = "rustls-tls")]
        {
            let certs = load_certs(Path::new(&certificate))?;
            let mut keys = load_keys(Path::new(&certificate_key))?;
            let config = ServerConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(certs, keys.remove(0))
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            // config
            //     .set_single_cert(certs, keys.remove(0))
            //     .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

            let acceptor = TlsAcceptor::from(Arc::new(config));
            Ok(Self { acceptor })
        }
        #[cfg(feature = "openssl-tls")]
        unimplemented!()
        // {
        //     let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        //     acceptor.set_private_key_file(certificate_key, SslFiletype::PEM).unwrap();
        //     acceptor.set_certificate_chain_file(certificate).unwrap();
        //     acceptor.check_private_key().unwrap();
        //     let acceptor = Arc::new(acceptor.build());
        //     Ok(Self {ssl_acceptor: acceptor.clone() })
        // }
    }
}

#[async_trait]
impl TcpInboundHandler for Handler {
    type TStream = AnyStream;
    type TDatagram = AnyInboundDatagram;

    async fn handle<'a>(
        &'a self,
        sess: Session,
        stream: Self::TStream,
    ) -> std::io::Result<InboundTransport<Self::TStream, Self::TDatagram>> {
        #[cfg(feature = "rustls-tls")]
        {
            Ok(InboundTransport::Stream(
                Box::new(self.acceptor.accept(stream).await?),
                sess,
            ))
        }

        #[cfg(feature = "openssl-tls")]
        unimplemented!()
        // {
        //     let ssl  = Ssl::new(self.ssl_acceptor.context()).unwrap();
        //     let mut steam = SslStream::new(ssl, stream).unwrap();
        //     steam.accept().await?;
        //     Ok(InboundTransport::Stream(
        //         Box::new(steam),
        //         sess,
        //     ))
        // }
    }
}
