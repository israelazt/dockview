//! Publishing to an RTMP server: the handshake, `connect`, `createStream` and
//! `publish`, then sending audio, video and metadata.

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::time::{Duration, Instant};

use super::amf::{self, Value};
use super::chunk::{ChunkReader, ChunkWriter, Message};
use crate::transport::{Target, Transport};

const HANDSHAKE_SIZE: usize = 1536;
const OUT_CHUNK_SIZE: usize = 4096;

const CSID_CONTROL: u32 = 2;
const CSID_COMMAND: u32 = 3;
const CSID_AUDIO: u32 = 4;
const CSID_VIDEO: u32 = 6;

const MSG_ACK: u8 = 3;
const MSG_USER_CONTROL: u8 = 4;
const MSG_WINDOW: u8 = 5;
const MSG_AUDIO: u8 = 8;
const MSG_VIDEO: u8 = 9;
const MSG_DATA: u8 = 18;
const MSG_COMMAND: u8 = 20;
const MSG_SET_CHUNK_SIZE: u8 = 1;

/// Why publishing failed.
#[derive(Debug)]
pub enum PublishError {
    /// The connection broke.
    Io(io::Error),
    /// The server did not answer in time.
    Timeout,
    /// The server said no (a wrong key, a taken name…); its own words.
    Refused(String),
    /// The server spoke nonsense.
    Protocol(String),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "connection: {e}"),
            Self::Timeout => write!(f, "the server did not answer"),
            Self::Refused(why) => write!(f, "refused: {why}"),
            Self::Protocol(why) => write!(f, "protocol: {why}"),
        }
    }
}

impl From<io::Error> for PublishError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn nothing_yet(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// A connection that is publishing.
pub struct Publisher {
    transport: Box<dyn Transport>,
    writer: ChunkWriter,
    reader: ChunkReader,
    /// Messages already read but not yet looked at.
    inbox: VecDeque<Message>,
    stream_id: u32,
    window: u64,
    received: u64,
    acknowledged: u64,
    sent: u64,
}

impl Publisher {
    /// Does the whole opening dance and returns when the server accepted the
    /// stream, or fails within about `patience`.
    pub fn connect(
        mut transport: Box<dyn Transport>,
        target: &Target,
        key: &str,
        patience: Duration,
    ) -> Result<Publisher, PublishError> {
        let deadline = Instant::now() + patience;
        let leftover = handshake(transport.as_mut(), deadline)?;
        let mut p = Publisher {
            transport,
            writer: ChunkWriter::default(),
            reader: ChunkReader::default(),
            inbox: VecDeque::new(),
            stream_id: 0,
            window: u64::from(u32::MAX),
            received: 0,
            acknowledged: 0,
            sent: 0,
        };
        p.take_in(&leftover)?;
        p.open(target, key, deadline)?;
        Ok(p)
    }

    /// Bytes handed to the connection so far.
    pub fn bytes_sent(&self) -> u64 {
        self.sent
    }

    /// Sends the stream's description (`onMetaData`).
    pub fn send_metadata(&mut self, info: &[(&str, Value)]) -> Result<(), PublishError> {
        let object = Value::Object(
            info.iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        );
        let payload = amf::encode_all(&[
            Value::str("@setDataFrame"),
            Value::str("onMetaData"),
            object,
        ]);
        self.send(CSID_VIDEO, MSG_DATA, 0, self.stream_id, payload)
    }

    /// Sends one video payload (see `flv`), stamped in milliseconds.
    pub fn send_video(&mut self, timestamp: u32, payload: Vec<u8>) -> Result<(), PublishError> {
        self.send(CSID_VIDEO, MSG_VIDEO, timestamp, self.stream_id, payload)
    }

    /// Sends one audio payload (see `flv`), stamped in milliseconds.
    pub fn send_audio(&mut self, timestamp: u32, payload: Vec<u8>) -> Result<(), PublishError> {
        self.send(CSID_AUDIO, MSG_AUDIO, timestamp, self.stream_id, payload)
    }

    /// Reads what the server has said (acknowledgements, pings) and answers.
    /// Call it now and then while sending, or the server may give up on us.
    pub fn poll(&mut self) -> Result<(), PublishError> {
        self.read_some()?;
        while let Some(m) = self.inbox.pop_front() {
            self.handle(&m)?;
        }
        Ok(())
    }

    fn send(
        &mut self,
        csid: u32,
        type_id: u8,
        timestamp: u32,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> Result<(), PublishError> {
        let bytes = self.writer.write(&Message {
            csid,
            timestamp,
            type_id,
            stream_id,
            payload,
        });
        self.transport.send(&bytes)?;
        self.sent += bytes.len() as u64;
        Ok(())
    }

    fn command(&mut self, stream_id: u32, csid: u32, values: &[Value]) -> Result<(), PublishError> {
        self.send(csid, MSG_COMMAND, 0, stream_id, amf::encode_all(values))
    }

    fn open(&mut self, target: &Target, key: &str, deadline: Instant) -> Result<(), PublishError> {
        self.send(
            CSID_CONTROL,
            MSG_SET_CHUNK_SIZE,
            0,
            0,
            (OUT_CHUNK_SIZE as u32).to_be_bytes().to_vec(),
        )?;
        self.writer.set_chunk_size(OUT_CHUNK_SIZE);
        let connect = Value::Object(vec![
            ("app".into(), Value::str(&target.app)),
            ("type".into(), Value::str("nonprivate")),
            (
                "flashVer".into(),
                Value::str("FMLE/3.0 (compatible; Dockview)"),
            ),
            ("tcUrl".into(), Value::str(&target.tc_url)),
        ]);
        self.command(
            0,
            CSID_COMMAND,
            &[Value::str("connect"), Value::Number(1.0), connect],
        )?;
        self.expect_result(1.0, deadline)?;
        self.command(
            0,
            CSID_COMMAND,
            &[Value::str("createStream"), Value::Number(2.0), Value::Null],
        )?;
        let reply = self.expect_result(2.0, deadline)?;
        self.stream_id =
            reply.get(3).and_then(Value::as_number).ok_or_else(|| {
                PublishError::Protocol("createStream gave no stream number".into())
            })? as u32;
        let publish = [
            Value::str("publish"),
            Value::Number(3.0),
            Value::Null,
            Value::str(key),
            Value::str("live"),
        ];
        self.command(self.stream_id, CSID_COMMAND, &publish)?;
        self.expect_publish_start(deadline)
    }

    /// Waits for `_result` of transaction `txn`.
    fn expect_result(&mut self, txn: f64, deadline: Instant) -> Result<Vec<Value>, PublishError> {
        loop {
            for m in self.next_messages(deadline)? {
                if m.type_id != MSG_COMMAND {
                    self.handle(&m)?;
                    continue;
                }
                let values = amf::decode_all(&m.payload)
                    .ok_or_else(|| PublishError::Protocol("unreadable command".into()))?;
                let name = values.first().and_then(Value::as_str).unwrap_or_default();
                let same = values.get(1).and_then(Value::as_number) == Some(txn);
                match (name, same) {
                    ("_result", true) => return Ok(values),
                    ("_error", true) => return Err(PublishError::Refused(describe(&values))),
                    _ => {}
                }
            }
        }
    }

    /// Waits for the `onStatus` that says the stream started (or was refused).
    fn expect_publish_start(&mut self, deadline: Instant) -> Result<(), PublishError> {
        loop {
            for m in self.next_messages(deadline)? {
                if m.type_id != MSG_COMMAND {
                    self.handle(&m)?;
                    continue;
                }
                let values = amf::decode_all(&m.payload).unwrap_or_default();
                if values.first().and_then(Value::as_str) != Some("onStatus") {
                    continue;
                }
                let info = values.get(3);
                let code = info
                    .and_then(|i| i.get("code"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let level = info
                    .and_then(|i| i.get("level"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if code == "NetStream.Publish.Start" {
                    return Ok(());
                }
                if level == "error" || code.contains("BadName") || code.contains("Unauthorized") {
                    return Err(PublishError::Refused(code.to_string()));
                }
            }
        }
    }

    /// Messages that arrive next, waiting until `deadline`.
    fn next_messages(&mut self, deadline: Instant) -> Result<Vec<Message>, PublishError> {
        loop {
            if !self.inbox.is_empty() {
                return Ok(self.inbox.drain(..).collect());
            }
            if !self.read_some()? {
                if Instant::now() >= deadline {
                    return Err(PublishError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// One read; whether any bytes came.
    fn read_some(&mut self) -> Result<bool, PublishError> {
        let mut buf = [0u8; 4096];
        match self.transport.recv(&mut buf) {
            Ok(0) => Err(PublishError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => {
                self.take_in(&buf[..n])?;
                Ok(true)
            }
            Err(e) if nothing_yet(&e) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn take_in(&mut self, bytes: &[u8]) -> Result<(), PublishError> {
        self.received += bytes.len() as u64;
        let messages = self.reader.feed(bytes).map_err(PublishError::Protocol)?;
        self.inbox.extend(messages);
        if self.received - self.acknowledged >= self.window {
            self.acknowledged = self.received;
            let ack = (self.received as u32).to_be_bytes().to_vec();
            self.send(CSID_CONTROL, MSG_ACK, 0, 0, ack)?;
        }
        Ok(())
    }

    /// Control messages: the window size and pings.
    fn handle(&mut self, m: &Message) -> Result<(), PublishError> {
        match (m.type_id, m.payload.as_slice()) {
            (MSG_WINDOW, [a, b, c, d, ..]) => {
                self.window = u64::from(u32::from_be_bytes([*a, *b, *c, *d])).max(1);
            }
            // A ping request (event 6) is answered with a ping response (7).
            (MSG_USER_CONTROL, [0, 6, a, b, c, d, ..]) => {
                let reply = vec![0, 7, *a, *b, *c, *d];
                self.send(CSID_CONTROL, MSG_USER_CONTROL, 0, 0, reply)?;
            }
            (MSG_COMMAND, payload) => {
                // A status that arrives while streaming: an error ends it.
                let values = amf::decode_all(payload).unwrap_or_default();
                let info = values.get(3);
                let level = info.and_then(|i| i.get("level")).and_then(Value::as_str);
                if values.first().and_then(Value::as_str) == Some("onStatus")
                    && level == Some("error")
                {
                    return Err(PublishError::Refused(describe(&values)));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// The server's own words from an `_error` or `onStatus`.
fn describe(values: &[Value]) -> String {
    values
        .iter()
        .find_map(|v| v.get("code").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "the server said no".to_string())
}

/// The simple handshake. Returns the bytes that came after it.
fn handshake(t: &mut dyn Transport, deadline: Instant) -> Result<Vec<u8>, PublishError> {
    let mut c1 = vec![0u8; HANDSHAKE_SIZE];
    for (i, b) in c1.iter_mut().enumerate().skip(8) {
        *b = (i * 31 % 251) as u8;
    }
    let mut out = vec![3u8];
    out.extend(&c1);
    t.send(&out)?;
    let mut got = Vec::new();
    let need = 1 + 2 * HANDSHAKE_SIZE;
    let mut buf = [0u8; 4096];
    let mut sent_c2 = false;
    while got.len() < need {
        match t.recv(&mut buf) {
            Ok(0) => return Err(PublishError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => got.extend(&buf[..n]),
            Err(e) if nothing_yet(&e) => {
                if Instant::now() >= deadline {
                    return Err(PublishError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => return Err(e.into()),
        }
        // C2 echoes S1, so it can go as soon as S1 is whole.
        if !sent_c2 && got.len() > HANDSHAKE_SIZE {
            if got[0] != 3 {
                return Err(PublishError::Protocol("unsupported RTMP version".into()));
            }
            t.send(&got[1..=HANDSHAKE_SIZE])?;
            sent_c2 = true;
        }
    }
    Ok(got.split_off(need))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeServer, Script};

    fn target() -> Target {
        Target::parse("rtmp://live.example/app").unwrap()
    }

    fn open(server: &FakeServer) -> Result<Publisher, PublishError> {
        Publisher::connect(
            server.transport(),
            &target(),
            "key123",
            Duration::from_millis(300),
        )
    }

    #[test]
    fn opens_a_stream_with_the_key() {
        let server = FakeServer::default();
        let p = open(&server).unwrap();
        assert!(p.bytes_sent() > 0);
        assert_eq!(server.seen(|s| s.connects), 1);
        assert_eq!(server.seen(|s| s.keys.clone()), ["key123"]);
    }

    #[test]
    fn media_arrives_whole_and_in_order() {
        let server = FakeServer::default();
        let mut p = open(&server).unwrap();
        p.send_metadata(&[("width", Value::Number(1920.0))])
            .unwrap();
        let big: Vec<u8> = (0..10_000).map(|i| i as u8).collect();
        p.send_video(0, big.clone()).unwrap();
        p.send_audio(5, vec![1, 2, 3]).unwrap();
        p.send_video(70_000, vec![9]).unwrap();
        p.poll().unwrap();
        assert_eq!(server.seen(|s| s.metadata.len()), 1);
        let video = server.seen(|s| s.video.clone());
        assert_eq!(video.len(), 2);
        assert_eq!(video[0].payload, big);
        assert_eq!(video[1].timestamp, 70_000);
        assert_eq!(server.seen(|s| s.audio[0].payload.clone()), [1, 2, 3]);
        assert_eq!(video[0].stream_id, 1);
    }

    #[test]
    fn a_refused_key_is_reported_with_the_servers_words() {
        let server = FakeServer::new(Script {
            publish_status: Some("NetStream.Publish.BadName".into()),
            ..Script::default()
        });
        match open(&server) {
            Err(PublishError::Refused(why)) => assert_eq!(why, "NetStream.Publish.BadName"),
            other => panic!("{:?}", other.err()),
        }
    }

    #[test]
    fn a_silent_server_times_out() {
        let server = FakeServer::new(Script {
            mute: true,
            ..Script::default()
        });
        assert!(matches!(open(&server), Err(PublishError::Timeout)));
    }

    #[test]
    fn a_dropped_connection_is_an_io_error() {
        let server = FakeServer::new(Script {
            cut_after: Some(4000),
            ..Script::default()
        });
        let mut p = open(&server).unwrap();
        let result = (0..10).try_for_each(|i| p.send_video(i, vec![0; 1000]));
        assert!(matches!(result, Err(PublishError::Io(_))));
    }
}
