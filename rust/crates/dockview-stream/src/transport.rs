//! The connection a stream goes over, as a port: the real one is TCP (RTMP) or
//! TCP with TLS (RTMPS); tests plug in a fake server.

use std::io;

/// An open, ordered byte connection.
pub trait Transport: Send {
    /// Sends all of `data`.
    fn send(&mut self, data: &[u8]) -> io::Result<()>;
    /// Reads up to `buf.len()` bytes, waiting for at least one. `Ok(0)` means
    /// the other end closed the connection.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

/// Opens connections.
pub trait Connector: Send {
    fn connect(&self, target: &Target) -> io::Result<Box<dyn Transport>>;
}

/// Where a stream goes, taken from a URL like `rtmp://live.twitch.tv/app`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    /// The application name (`app`), without slashes at the ends.
    pub app: String,
    /// The original URL, which RTMP sends as `tcUrl`.
    pub tc_url: String,
}

impl Target {
    /// Parses `rtmp://host[:port]/app` or `rtmps://…`. `None` when it is
    /// neither or has no host or no application.
    pub fn parse(url: &str) -> Option<Target> {
        let url = url.trim();
        let (secure, rest) = if let Some(r) = url.strip_prefix("rtmps://") {
            (true, r)
        } else {
            (false, url.strip_prefix("rtmp://")?)
        };
        let (authority, path) = rest.split_once('/')?;
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().ok()?),
            None => (authority, if secure { 443 } else { 1935 }),
        };
        let app = path.trim_matches('/');
        if host.is_empty() || app.is_empty() {
            return None;
        }
        Some(Target {
            secure,
            host: host.to_string(),
            port,
            app: app.to_string(),
            tc_url: url.trim_end_matches('/').to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_rtmp_url() {
        let t = Target::parse("rtmp://live.twitch.tv/app").unwrap();
        assert!(!t.secure);
        assert_eq!(
            (t.host.as_str(), t.port, t.app.as_str()),
            ("live.twitch.tv", 1935, "app")
        );
        assert_eq!(t.tc_url, "rtmp://live.twitch.tv/app");
    }

    #[test]
    fn rtmps_defaults_to_443_and_a_port_can_be_given() {
        assert_eq!(
            Target::parse("rtmps://a.rtmps.youtube.com/live2")
                .unwrap()
                .port,
            443
        );
        assert_eq!(
            Target::parse("rtmp://localhost:1936/live/").unwrap().port,
            1936
        );
    }

    #[test]
    fn keeps_a_multi_part_application() {
        assert_eq!(
            Target::parse("rtmp://h/live/extra").unwrap().app,
            "live/extra"
        );
    }

    #[test]
    fn rejects_what_is_not_an_rtmp_url() {
        for bad in [
            "http://h/app",
            "rtmp://h",
            "rtmp://h/",
            "rtmp:///app",
            "rtmp://h:x/app",
            "",
        ] {
            assert!(Target::parse(bad).is_none(), "{bad}");
        }
    }
}
