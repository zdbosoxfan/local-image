//! Real TLS/CONNECT fixtures. example.com resolves exclusively to loopback here.
use super::*;
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

const CERT: &[u8] = include_bytes!("testdata/cert.der");
const KEY: &[u8] = include_bytes!("testdata/key.pk8");
type TlsStream = StreamOwned<ServerConnection, TcpStream>;
#[derive(Debug)]
struct Loopback;
impl Resolver for Loopback {
    fn resolve(&self, uri: &ureq::http::Uri, _: &ureq::config::Config, _: NextTimeout) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
        assert!(matches!(uri.host(), Some("example.com" | "wrong.test" | "127.0.0.1")));
        let mut addresses = ResolvedSocketAddrs::from_fn(|_| SocketAddr::from(([127, 0, 0, 1], 0)));
        addresses.push(SocketAddr::from(([127, 0, 0, 1], uri.port_u16().unwrap())));
        Ok(addresses)
    }
}
fn tls_config() -> Arc<ServerConfig> {
    let config = ServerConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![rustls::pki_types::CertificateDer::from(CERT.to_vec())], rustls::pki_types::PrivateKeyDer::Pkcs8(KEY.to_vec().into()))
        .unwrap();
    Arc::new(config)
}
fn socket(stream: &TcpStream) {
    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
}
fn headers(stream: &mut impl Read) -> std::io::Result<String> {
    let mut data = Vec::new();
    while !data.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if stream.read(&mut byte)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        data.push(byte[0]);
        if data.len() > 16384 {
            return Err(std::io::Error::other("fixture headers too large"));
        }
    }
    Ok(String::from_utf8(data).unwrap())
}
fn tls_server(handler: impl FnOnce(TlsStream) + Send + 'static) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = tls_config();
    let worker = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        socket(&stream);
        let mut tls = StreamOwned::new(ServerConnection::new(config).unwrap(), stream);
        if headers(&mut tls).is_ok() {
            handler(tls);
        }
    });
    (port, worker)
}
fn config(trusted: bool, proxy: Option<ureq::Proxy>, body: Duration) -> ureq::config::Config {
    let tls = if trusted {
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
            .root_certs(vec![ureq::tls::Certificate::from_der(CERT)].into())
            .build()
    } else {
        crate::tls::config()
    };
    ureq::Agent::config_builder()
        .tls_config(tls)
        .proxy(proxy)
        .max_idle_connections(0)
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_resolve(Some(Duration::from_secs(3)))
        .timeout_send_request(Some(Duration::from_secs(3)))
        .timeout_recv_response(Some(Duration::from_secs(3)))
        .timeout_recv_body(Some(body))
        .build()
}

#[test]
fn tls_download_succeeds_with_trusted_chain_and_rejects_wrong_host_or_untrusted_root() {
    for (trusted, host, success) in [(true, "example.com", true), (false, "example.com", false), (true, "wrong.test", false)] {
        let (port, server) = tls_server(|mut tls| {
            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").unwrap();
            tls.flush().unwrap();
        });
        let agent = transport::agent_with_resolver(config(trusted, None, Duration::from_secs(3)), &JobControl::new(), Loopback);
        let result = agent.get(format!("https://{host}:{port}/model")).call();
        if success {
            assert_eq!(result.unwrap().body_mut().read_to_vec().unwrap(), b"hello");
        } else {
            assert!(result.is_err(), "TLS verification must still reject {host}, trusted={trusted}");
        }
        server.join().unwrap();
    }
}

#[test]
fn a_trickled_partial_tls_record_keeps_the_original_body_deadline() {
    let (port, server) = tls_server(|mut tls| {
        tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n").unwrap();
        tls.flush().unwrap();
        tls.conn.writer().write_all(b"hello").unwrap();
        let mut record = Vec::new();
        while tls.conn.wants_write() {
            tls.conn.write_tls(&mut record).unwrap();
        }
        assert!(record.len() > 20);
        for fragment in record.chunks(3) {
            thread::sleep(Duration::from_millis(45));
            if tls.sock.write_all(fragment).is_err() {
                break;
            }
        }
    });
    let agent = transport::agent_with_resolver(config(true, None, Duration::from_millis(140)), &JobControl::new(), Loopback);
    let mut response = agent.get(format!("https://example.com:{port}/model")).call().unwrap();
    let start = Instant::now();
    assert!(response.body_mut().read_to_vec().is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    drop(response);
    server.join().unwrap();
}

enum ProxyStream {
    Plain(TcpStream),
    Tls(Box<TlsStream>),
}
impl Read for ProxyStream {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(b),
            Self::Tls(s) => s.read(b),
        }
    }
}
impl Write for ProxyStream {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(b),
            Self::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}
impl ProxyStream {
    fn nonblocking(&self) {
        match self {
            Self::Plain(s) => s.set_nonblocking(true),
            Self::Tls(s) => s.sock.set_nonblocking(true),
        }
        .unwrap();
    }
}
fn write_pending(stream: &mut impl Write, bytes: &mut Vec<u8>) -> std::io::Result<()> {
    if !bytes.is_empty() {
        match stream.write(bytes) {
            Ok(n) => {
                bytes.drain(..n);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
    }
    match stream.flush() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        result => result,
    }
}
fn proxy(tls: bool, target: u16) -> (ureq::Proxy, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = tls_config();
    let worker = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        socket(&stream);
        let mut client =
            if tls { ProxyStream::Tls(Box::new(StreamOwned::new(ServerConnection::new(config).unwrap(), stream))) } else { ProxyStream::Plain(stream) };
        let request = headers(&mut client).unwrap();
        assert!(request.starts_with(&format!("CONNECT example.com:{target} HTTP/1.1")), "{request}");
        assert!(request.to_lowercase().contains("proxy-authorization: basic zml4dhvyzs11c2vyomzpehr1cmutcgfzcw=="));
        let mut upstream = TcpStream::connect(("127.0.0.1", target)).unwrap();
        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").unwrap();
        client.flush().unwrap();
        client.nonblocking();
        upstream.set_nonblocking(true).unwrap();
        let (mut to_client, mut to_upstream) = (Vec::new(), Vec::new());
        let mut target_eof = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buf = [0; 8192];
        while Instant::now() < deadline {
            match client.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => to_upstream.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
            if write_pending(&mut upstream, &mut to_upstream).is_err() {
                break;
            }
            if !target_eof {
                match upstream.read(&mut buf) {
                    Ok(0) => target_eof = true,
                    Ok(n) => to_client.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
            }
            if write_pending(&mut client, &mut to_client).is_err() {
                break;
            }
            if target_eof && to_client.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(Instant::now() < deadline, "proxy relay did not finish");
    });
    (ureq::Proxy::new(&format!("{}://fixture-user:fixture-pass@example.com:{port}", if tls { "https" } else { "http" })).unwrap(), worker)
}

#[test]
fn verified_https_downloads_through_http_and_https_connect_with_proxy_auth() {
    for encrypted_proxy in [false, true] {
        let (port, server) = tls_server(|mut tls| {
            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").unwrap();
            tls.flush().unwrap();
        });
        let (proxy, relay) = proxy(encrypted_proxy, port);
        let agent = transport::agent_with_resolver(config(true, Some(proxy), Duration::from_secs(3)), &JobControl::new(), Loopback);
        let mut response = agent.get(format!("https://example.com:{port}/model")).call().unwrap();
        assert_eq!(response.body_mut().read_to_vec().unwrap(), b"hello");
        drop(response);
        drop(agent);
        relay.join().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn no_proxy_bypasses_unsupported_socks_instead_of_rejecting_the_direct_route() {
    let (port, server) = tls_server(|mut tls| {
        tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx").unwrap();
        tls.flush().unwrap();
    });
    let proxy = ureq::Proxy::builder(ureq::ProxyProtocol::Socks5).host("127.0.0.1").port(1).no_proxy("example.com").build().unwrap();
    let agent = transport::agent_with_resolver(config(true, Some(proxy), Duration::from_secs(3)), &JobControl::new(), Loopback);
    assert_eq!(agent.get(format!("https://example.com:{port}/")).call().unwrap().body_mut().read_to_vec().unwrap(), b"x");
    server.join().unwrap();
}

#[test]
fn cancellation_interrupts_stalled_connect_responses_over_plain_and_tls_proxies() {
    use std::sync::mpsc;
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (ready, started) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            socket(&stream);
            let mut client = if encrypted {
                ProxyStream::Tls(Box::new(StreamOwned::new(ServerConnection::new(tls_config()).unwrap(), stream)))
            } else {
                ProxyStream::Plain(stream)
            };
            assert!(headers(&mut client).unwrap().starts_with("CONNECT "));
            ready.send(()).unwrap();
            let _ = held.recv_timeout(Duration::from_secs(4));
        });
        let proxy = ureq::Proxy::new(&format!("{}://example.com:{port}", if encrypted { "https" } else { "http" })).unwrap();
        let ctl = JobControl::new();
        let control = ctl.clone();
        let (done, result) = mpsc::channel();
        let worker = thread::spawn(move || {
            let agent = transport::agent_with_resolver(config(true, Some(proxy), Duration::from_secs(3)), &control, Loopback);
            done.send(agent.get("https://example.com:443/model").call().is_err()).unwrap();
        });
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        ctl.cancel();
        assert!(result.recv_timeout(Duration::from_secs(2)).expect("CONNECT response must observe cancellation"));
        worker.join().unwrap();
        release.send(()).unwrap();
        server.join().unwrap();
    }
}
