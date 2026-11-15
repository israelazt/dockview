//! A fake RTMP server for tests: speaks just enough of the protocol to accept
//! a publisher, records what it sends, and can be told to misbehave.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use crate::rtmp::amf::{self, Value};
use crate::rtmp::chunk::{ChunkReader, ChunkWriter, Message};
use crate::transport::{Connector, Target, Transport};

const HANDSHAKE: usize = 1536;

/// How the fake server behaves.
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// Answer `publish` with this status code instead of `NetStream.Publish.Start`.
    pub publish_status: Option<String>,
    /// Never answer `connect`.
    pub mute: bool,
    /// Break the connection after receiving this many bytes in total.
    pub cut_after: Option<usize>,
}

#[derive(Debug, Default)]
pub struct Seen {
    pub connects: u32,
    pub keys: Vec<String>,
    pub video: Vec<Message>,
    pub audio: Vec<Message>,
    pub metadata: Vec<Message>,
    pub bytes: usize,
}

#[derive(Default)]
struct State {
    script: Script,
    seen: Seen,
    inbox: Vec<u8>,
    outbox: VecDeque<u8>,
    greeted: bool,
    c2_done: bool,
    from_client: ChunkReader,
    to_client: ChunkWriter,
    cut: bool,
}

/// The fake server; clone it to look at it from the test.
#[derive(Clone, Default)]
pub struct FakeServer(Arc<Mutex<State>>);

impl FakeServer {
    pub fn new(script: Script) -> Self {
        let s = Self::default();
        s.0.lock().unwrap().script = script;
        s
    }

    /// A connection to this server.
    pub fn transport(&self) -> Box<dyn Transport> {
        let mut st = self.0.lock().unwrap();
        *st = State {
            script: std::mem::take(&mut st.script),
            seen: std::mem::take(&mut st.seen),
            ..State::default()
        };
        drop(st);
        Box::new(Link(self.0.clone()))
    }

    /// Looks at what the server has seen.
    pub fn seen<R>(&self, look: impl FnOnce(&Seen) -> R) -> R {
        look(&self.0.lock().unwrap().seen)
    }

    pub fn set_script(&self, script: Script) {
        self.0.lock().unwrap().script = script;
    }
}

/// Opens connections to a [`FakeServer`], or refuses while `down` is set.
pub struct FakeConnector {
    pub server: FakeServer,
    pub down: Arc<Mutex<bool>>,
}

impl Connector for FakeConnector {
    fn connect(&self, _: &Target) -> io::Result<Box<dyn Transport>> {
        if *self.down.lock().unwrap() {
            return Err(io::ErrorKind::ConnectionRefused.into());
        }
        Ok(self.server.transport())
    }
}

struct Link(Arc<Mutex<State>>);

impl Transport for Link {
    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        let mut st = self.0.lock().unwrap();
        if st.cut {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        st.seen.bytes += data.len();
        if st.script.cut_after.is_some_and(|n| st.seen.bytes > n) {
            st.cut = true;
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        st.inbox.extend(data);
        st.step();
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut st = self.0.lock().unwrap();
        if st.cut {
            return Ok(0);
        }
        let n = buf.len().min(st.outbox.len());
        if n == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        for slot in buf.iter_mut().take(n) {
            *slot = st.outbox.pop_front().unwrap_or(0);
        }
        Ok(n)
    }
}

impl State {
    fn step(&mut self) {
        if !self.greeted {
            if self.inbox.len() < 1 + HANDSHAKE {
                return;
            }
            self.greeted = true;
            self.outbox.push_back(3);
            self.outbox.extend(vec![7u8; HANDSHAKE]); // S1
            self.outbox.extend(self.inbox[1..=HANDSHAKE].to_vec()); // S2 echoes C1
            self.inbox.drain(..1 + HANDSHAKE);
        }
        if !self.c2_done {
            if self.inbox.len() < HANDSHAKE {
                return;
            }
            self.inbox.drain(..HANDSHAKE);
            self.c2_done = true;
        }
        let bytes = std::mem::take(&mut self.inbox);
        for m in self.from_client.feed(&bytes).unwrap_or_default() {
            self.react(m);
        }
    }

    fn say(&mut self, type_id: u8, csid: u32, payload: Vec<u8>) {
        let bytes = self.to_client.write(&Message {
            csid,
            timestamp: 0,
            type_id,
            stream_id: 0,
            payload,
        });
        self.outbox.extend(bytes);
    }

    fn react(&mut self, m: Message) {
        match m.type_id {
            9 => self.seen.video.push(m),
            8 => self.seen.audio.push(m),
            18 => self.seen.metadata.push(m),
            20 => {
                let v = amf::decode_all(&m.payload).unwrap_or_default();
                let txn = v.get(1).and_then(Value::as_number).unwrap_or(0.0);
                match v.first().and_then(Value::as_str) {
                    Some("connect") => self.on_connect(txn),
                    Some("createStream") => {
                        let reply = [
                            Value::str("_result"),
                            Value::Number(txn),
                            Value::Null,
                            Value::Number(1.0),
                        ];
                        self.say(20, 3, amf::encode_all(&reply));
                    }
                    Some("publish") => self.on_publish(&v),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn on_connect(&mut self, txn: f64) {
        self.seen.connects += 1;
        if self.script.mute {
            return;
        }
        self.say(5, 2, 2_500_000u32.to_be_bytes().to_vec());
        self.say(1, 2, 4096u32.to_be_bytes().to_vec());
        self.to_client.set_chunk_size(4096);
        let reply = [
            Value::str("_result"),
            Value::Number(txn),
            Value::Null,
            Value::Object(vec![]),
        ];
        self.say(20, 3, amf::encode_all(&reply));
    }

    fn on_publish(&mut self, v: &[Value]) {
        self.seen
            .keys
            .push(v.get(3).and_then(Value::as_str).unwrap_or("").to_string());
        let code = self
            .script
            .publish_status
            .clone()
            .unwrap_or_else(|| "NetStream.Publish.Start".into());
        let level = if code.ends_with("Start") {
            "status"
        } else {
            "error"
        };
        let info = Value::Object(vec![
            ("level".into(), Value::str(level)),
            ("code".into(), Value::str(&code)),
        ]);
        let status = [
            Value::str("onStatus"),
            Value::Number(0.0),
            Value::Null,
            info,
        ];
        self.say(20, 5, amf::encode_all(&status));
    }
}
