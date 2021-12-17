use std::sync::Arc;
use std::time::Duration;

use futures::future::abortable;
use futures::FutureExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, ToSocketAddrs, UdpSocket};
use tokio::sync::RwLock;
use tokio::time::timeout;

use flower::proxy::*;

pub async fn run_tcp_echo_server<A: ToSocketAddrs>(addr: A) {
    let listener = TcpListener::bind(addr).await.unwrap();
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
            Err(e) => {
                panic!("accept tcp failed: {}", e);
            }
        }
    }
}

pub async fn run_udp_echo_server<A: ToSocketAddrs>(addr: A) {
    let socket = UdpSocket::bind(addr).await.unwrap();
    let mut buf = vec![0u8; 2 * 1024];
    loop {
        let (n, raddr) = socket.recv_from(&mut buf).await.unwrap();
        let _ = socket.send_to(&buf[..n], &raddr).await.unwrap();
    }
}

// Runs echo servers.
pub async fn run_echo_servers<A: ToSocketAddrs + 'static + Copy>(addr: A) {
    let tcp_task = run_tcp_echo_server(addr);
    let udp_task = run_udp_echo_server(addr);
    futures::future::join(tcp_task, udp_task).await;
}

// Runs multiple flower instances.
pub fn run_flower_instances(
    rt: &tokio::runtime::Runtime,
    configs: Vec<String>,
) -> Vec<flower::RuntimeId> {
    let mut flower_rt_ids = Vec::new();
    let mut rt_id = 0;
    for config in configs {
        let config = flower::config::json::from_string(&config).unwrap();
        let opts = flower::StartOptions {
            config: flower::Config::Internal(config),
            #[cfg(feature = "auto-reload")]
            auto_reload: false,
            runtime_opt: flower::RuntimeOption::SingleThread,
        };
        rt.spawn_blocking(move || {
            flower::start(rt_id, opts).unwrap();
        });
        flower_rt_ids.push(rt_id);
        rt_id += 1;
    }
    flower_rt_ids
}

// Runs multiple flower instances, thereafter a socks request will be sent to the
// given socks server to test the proxy chain. The proxy chain is expected to
// correctly handle the request to it's destination.
pub fn test_configs(configs: Vec<String>, socks_addr: &str, socks_port: u16) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Use an echo server as the destination of the socks request.
    let mut bg_tasks: Vec<flower::Runner> = Vec::new();
    let echo_server_task = run_echo_servers("127.0.0.1:3000");
    bg_tasks.push(Box::pin(echo_server_task));
    let (bg_task, bg_task_handle) = abortable(futures::future::join_all(bg_tasks));

    let flower_rt_ids = run_flower_instances(&rt, configs);

    // Simulates an application request.
    let app_task = async move {
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Make use of a socks outbound to initiate a socks request to a flower instance.
        let settings = flower::config::json::SocksOutboundSettings {
            address: Some(socks_addr.to_string()),
            port: Some(socks_port),
        };
        let settings_str = serde_json::to_string(&settings).unwrap();
        let raw_settings = serde_json::value::RawValue::from_string(settings_str).unwrap();
        let outbounds = vec![flower::config::json::Outbound {
            protocol: "socks".to_string(),
            tag: Some("socks".to_string()),
            settings: Some(raw_settings),
        }];
        let mut config = flower::config::json::Config {
            log: None,
            inbounds: None,
            outbounds: Some(outbounds),
            router: None,
            dns: None,
            api: None,
        };
        let config = flower::config::json::to_internal(&mut config).unwrap();
        let dns_client = Arc::new(RwLock::new(
            flower::app::dns_client::DnsClient::new(&config.dns).unwrap(),
        ));
        let outbound_manager =
            flower::app::outbound::manager::OutboundManager::new(&config.outbounds, dns_client)
                .unwrap();
        let handler = outbound_manager.get("socks").unwrap();
        let mut sess = flower::session::Session::default();
        sess.destination = flower::session::SocksAddr::Ip("127.0.0.1:3000".parse().unwrap());

        // Test TCP
        let stream = tokio::net::TcpStream::connect(format!("{}:{}", socks_addr, socks_port))
            .await
            .unwrap();
        let mut s = TcpOutboundHandler::handle(handler.as_ref(), &sess, Some(Box::new(stream)))
            .await
            .unwrap();
        s.write_all(b"abc").await.unwrap();
        let mut buf = Vec::new();
        let n = s.read_buf(&mut buf).await.unwrap();
        assert_eq!("abc".to_string(), String::from_utf8_lossy(&buf[..n]));

        // Test UDP
        let dgram = UdpOutboundHandler::handle(handler.as_ref(), &sess, None)
            .await
            .unwrap();
        let (mut r, mut s) = dgram.split();
        let msg = b"def";
        let n = s.send_to(&msg.to_vec(), &sess.destination).await.unwrap();
        assert_eq!(msg.len(), n);
        let mut buf = vec![0u8; 2 * 1024];
        let (n, raddr) = timeout(Duration::from_secs(1), r.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(msg, &buf[..n]);
        assert_eq!(&raddr, &sess.destination);

        // Test if we can handle a second UDP session. This can fail in stream
        // transports if the stream ID has not been correctly set.
        let dgram = UdpOutboundHandler::handle(handler.as_ref(), &sess, None)
            .await
            .unwrap();
        let (mut r, mut s) = dgram.split();
        let msg = b"ghi";
        let n = s.send_to(&msg.to_vec(), &sess.destination).await.unwrap();
        assert_eq!(msg.len(), n);
        let mut buf = vec![0u8; 2 * 1024];
        let (n, raddr) = timeout(Duration::from_secs(1), r.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(msg, &buf[..n]);
        assert_eq!(&raddr, &sess.destination);

        // Cancel the background task.
        bg_task_handle.abort();
    };
    rt.block_on(futures::future::join(bg_task, app_task).map(|_| ()));
    for id in flower_rt_ids.into_iter() {
        assert!(flower::shutdown(id));
    }
}
