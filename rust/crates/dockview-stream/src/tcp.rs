//! The real connection for plain RTMP: a TCP socket from the standard library,
//! the same on Windows and Linux. (RTMPS adds TLS on top; see STREAMER_PLAN.)

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::transport::{Connector, Target, Transport};

/// How long opening the socket may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a read waits before saying "nothing yet".
const READ_WAIT: Duration = Duration::from_millis(5);
/// A send that blocks this long means the network is stuck.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Opens TCP connections, with TLS on top for `rtmps://`.
pub struct TcpConnector {
    tls: Arc<ClientConfig>,
}

impl Default for TcpConnector {
    /// Trusts the usual public certificate authorities.
    fn default() -> Self {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        Self::with_tls(tls_config(roots))
    }
}

/// A client configuration that trusts `roots` (the ring crypto provider, so no
/// C toolchain is needed to build).
pub fn tls_config(roots: RootCertStore) -> ClientConfig {
    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

impl TcpConnector {
    /// Uses `config` for `rtmps://` (tests trust their own certificate).
    pub fn with_tls(config: ClientConfig) -> Self {
        Self {
            tls: Arc::new(config),
        }
    }

    fn open_socket(target: &Target) -> io::Result<TcpStream> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no address for the server");
        for address in (target.host.as_str(), target.port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(socket) => {
                    socket.set_nodelay(true)?;
                    socket.set_write_timeout(Some(WRITE_TIMEOUT))?;
                    return Ok(socket);
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn secure(&self, target: &Target, mut socket: TcpStream) -> io::Result<Box<dyn Transport>> {
        let name = ServerName::try_from(target.host.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut connection = ClientConnection::new(self.tls.clone(), name)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        // The handshake needs patient reads; the short wait comes after it.
        socket.set_read_timeout(Some(CONNECT_TIMEOUT))?;
        while connection.is_handshaking() {
            connection.complete_io(&mut socket)?;
        }
        socket.set_read_timeout(Some(READ_WAIT))?;
        Ok(Box::new(Link(StreamOwned::new(connection, socket))))
    }
}

impl Connector for TcpConnector {
    fn connect(&self, target: &Target) -> io::Result<Box<dyn Transport>> {
        let socket = Self::open_socket(target)?;
        if target.secure {
            return self.secure(target, socket);
        }
        socket.set_read_timeout(Some(READ_WAIT))?;
        Ok(Box::new(Link(socket)))
    }
}

/// Any byte stream is a transport: a socket, or a socket inside TLS.
struct Link<S: Read + Write + Send>(S);

impl<S: Read + Write + Send> Transport for Link<S> {
    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_all(data)?;
        self.0.flush()
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use super::*;
    use crate::output::{StreamConfig, StreamOutput};
    use crate::testing::FakeServer;
    use dockview_domain::broadcast::StreamState;
    use dockview_domain::encoded::EncodedFrame;
    use std::time::Instant;

    /// Wires a stream to a fake RTMP server until either side closes.
    fn bridge<S: Read + Write>(mut socket: S, server: &FakeServer) {
        let mut link = server.transport();
        let mut buf = [0u8; 8192];
        loop {
            match socket.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => {
                    if link.send(&buf[..n]).is_err() {
                        return;
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return,
            }
            while let Ok(n) = link.recv(&mut buf) {
                if n == 0 || socket.write_all(&buf[..n]).is_err() || socket.flush().is_err() {
                    return;
                }
            }
        }
    }

    /// A local server: every socket is wired to a fake RTMP server.
    fn serve(server: FakeServer) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((socket, _)) = listener.accept() {
                socket
                    .set_read_timeout(Some(Duration::from_millis(2)))
                    .unwrap();
                bridge(socket, &server);
            }
        });
        port
    }

    /// A client that trusts a fresh self-signed certificate for `localhost`,
    /// and the server configuration that holds it.
    fn test_tls() -> (ClientConfig, Arc<rustls::ServerConfig>) {
        let made = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let der = made.cert.der().clone();
        let mut roots = RootCertStore::empty();
        roots.add(der.clone()).unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(made.key_pair.serialize_der());
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![der], key.into())
        .unwrap();
        (tls_config(roots), Arc::new(server))
    }

    #[test]
    fn streams_over_a_real_socket() {
        let server = FakeServer::default();
        let port = serve(server.clone());
        let target = Target::parse(&format!("rtmp://127.0.0.1:{port}/live")).unwrap();
        let config = StreamConfig {
            width: 1280,
            height: 720,
            fps: 30,
            video_kbps: 3000,
            audio: None,
            max_attempts: 1,
        };
        let mut out = StreamOutput::new(Box::new(TcpConnector::default()), target, "key", config);
        let now = Instant::now();
        out.start(now);
        assert_eq!(
            out.stats().state,
            StreamState::Live,
            "{:?}",
            out.last_error()
        );
        let key = EncodedFrame {
            data: vec![
                0, 0, 0, 1, 0x67, 0x64, 0, 0x1F, 0, 0, 0, 1, 0x68, 0xEE, 0, 0, 0, 1, 0x65, 1, 2,
            ],
            timestamp: 0,
            duration: 0,
            keyframe: true,
        };
        out.push_video(&key, now);
        // Give the server thread a moment to read.
        for _ in 0..200 {
            if server.seen(|s| s.video.len()) >= 2 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(server.seen(|s| s.video.len()), 2);
        assert_eq!(server.seen(|s| s.keys.clone()), ["key"]);
    }

    #[test]
    fn streams_over_tls_when_the_certificate_is_trusted() {
        let (client, server_config) = test_tls();
        let server = FakeServer::default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let fake = server.clone();
        thread::spawn(move || {
            let Ok((socket, _)) = listener.accept() else {
                return;
            };
            socket
                .set_read_timeout(Some(Duration::from_millis(2)))
                .unwrap();
            let conn = rustls::ServerConnection::new(server_config).unwrap();
            bridge(StreamOwned::new(conn, socket), &fake);
        });
        let target = Target::parse(&format!("rtmps://localhost:{port}/live")).unwrap();
        let config = StreamConfig {
            width: 1280,
            height: 720,
            fps: 30,
            video_kbps: 3000,
            audio: None,
            max_attempts: 1,
        };
        let mut out = StreamOutput::new(
            Box::new(TcpConnector::with_tls(client)),
            target,
            "tlskey",
            config,
        );
        let now = Instant::now();
        out.start(now);
        assert_eq!(
            out.stats().state,
            StreamState::Live,
            "{:?}",
            out.last_error()
        );
        assert_eq!(server.seen(|s| s.keys.clone()), ["tlskey"]);
    }

    #[test]
    fn an_untrusted_certificate_is_refused() {
        let (_, server_config) = test_tls();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((socket, _)) = listener.accept() {
                let conn = rustls::ServerConnection::new(server_config).unwrap();
                let mut tls = StreamOwned::new(conn, socket);
                let _ = tls.read(&mut [0u8; 16]);
            }
        });
        let target = Target::parse(&format!("rtmps://localhost:{port}/live")).unwrap();
        // The default connector only trusts public authorities.
        assert!(TcpConnector::default().connect(&target).is_err());
    }

    #[test]
    fn a_closed_port_is_an_error_not_a_hang() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let target = Target::parse(&format!("rtmp://127.0.0.1:{port}/live")).unwrap();
        assert!(TcpConnector::default().connect(&target).is_err());
    }
}
