//! The real connection for plain RTMP: a TCP socket from the standard library,
//! the same on Windows and Linux. (RTMPS adds TLS on top; see STREAMER_PLAN.)

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::transport::{Connector, Target, Transport};

/// How long opening the socket may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a read waits before saying "nothing yet".
const READ_WAIT: Duration = Duration::from_millis(5);
/// A send that blocks this long means the network is stuck.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Opens plain TCP connections.
pub struct TcpConnector;

impl Connector for TcpConnector {
    fn connect(&self, target: &Target) -> io::Result<Box<dyn Transport>> {
        if target.secure {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "rtmps:// needs TLS, which is not available yet",
            ));
        }
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no address for the server");
        for address in (target.host.as_str(), target.port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(socket) => {
                    socket.set_nodelay(true)?;
                    socket.set_read_timeout(Some(READ_WAIT))?;
                    socket.set_write_timeout(Some(WRITE_TIMEOUT))?;
                    return Ok(Box::new(Tcp(socket)));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

struct Tcp(TcpStream);

impl Transport for Tcp {
    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_all(data)
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

    /// A local server: every socket is wired to a fake RTMP server.
    fn serve(server: FakeServer) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut link = server.transport();
            socket
                .set_read_timeout(Some(Duration::from_millis(2)))
                .unwrap();
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
                    if n == 0 || socket.write_all(&buf[..n]).is_err() {
                        return;
                    }
                }
            }
        });
        port
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
        let mut out = StreamOutput::new(Box::new(TcpConnector), target, "key", config);
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
    fn rtmps_is_refused_until_tls_exists() {
        let target = Target::parse("rtmps://example.com/live").unwrap();
        let err = TcpConnector.connect(&target).err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn a_closed_port_is_an_error_not_a_hang() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let target = Target::parse(&format!("rtmp://127.0.0.1:{port}/live")).unwrap();
        assert!(TcpConnector.connect(&target).is_err());
    }
}
