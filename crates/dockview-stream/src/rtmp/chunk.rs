//! The RTMP chunk stream: messages cut into chunks going out, chunks put back
//! together coming in.

use std::collections::HashMap;

/// What a server's default chunk size is until told otherwise.
pub const DEFAULT_CHUNK_SIZE: usize = 128;
/// Timestamps from here on go in an extra four bytes.
const EXTENDED: u32 = 0xFF_FFFF;

/// One complete message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub csid: u32,
    pub timestamp: u32,
    pub type_id: u8,
    pub stream_id: u32,
    pub payload: Vec<u8>,
}

/// Cuts messages into chunks.
#[derive(Debug)]
pub struct ChunkWriter {
    chunk_size: usize,
}

impl Default for ChunkWriter {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

impl ChunkWriter {
    /// Uses `size` from now on (after the Set Chunk Size message went out).
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size.max(1);
    }

    /// The bytes of one message, as a full header and then continuations.
    pub fn write(&self, m: &Message) -> Vec<u8> {
        let csid = (m.csid.clamp(2, 63)) as u8;
        let extended = m.timestamp >= EXTENDED;
        let ts = if extended { EXTENDED } else { m.timestamp };
        let mut out = Vec::with_capacity(m.payload.len() + 32);
        out.push(csid); // format 0
        out.extend(&ts.to_be_bytes()[1..]);
        out.extend(&(m.payload.len() as u32).to_be_bytes()[1..]);
        out.push(m.type_id);
        out.extend(m.stream_id.to_le_bytes());
        if extended {
            out.extend(m.timestamp.to_be_bytes());
        }
        let mut chunks = m.payload.chunks(self.chunk_size);
        if let Some(first) = chunks.next() {
            out.extend(first);
        }
        for rest in chunks {
            out.push(0xC0 | csid); // format 3: same header as before
            if extended {
                out.extend(m.timestamp.to_be_bytes());
            }
            out.extend(rest);
        }
        out
    }
}

#[derive(Debug, Default, Clone)]
struct Pending {
    timestamp: u32,
    delta: u32,
    length: usize,
    type_id: u8,
    stream_id: u32,
    extended: bool,
    payload: Vec<u8>,
}

/// Puts incoming chunks back together.
#[derive(Debug)]
pub struct ChunkReader {
    chunk_size: usize,
    buffer: Vec<u8>,
    streams: HashMap<u32, Pending>,
}

impl Default for ChunkReader {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            buffer: Vec::new(),
            streams: HashMap::new(),
        }
    }
}

impl ChunkReader {
    /// Sets the chunk size by hand (a Set Chunk Size message that arrives is
    /// applied by `feed` itself).
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size.max(1);
    }

    /// Adds received bytes and returns the messages they complete. Damaged
    /// input (a chunk that refers to a stream never described) is an error.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Message>, String> {
        let mut buffer = std::mem::take(&mut self.buffer);
        buffer.extend(bytes);
        let mut messages = Vec::new();
        let mut at = 0;
        loop {
            match self.chunk(&buffer[at..]) {
                Ok(Some((used, done))) => {
                    at += used;
                    messages.extend(done);
                }
                Ok(None) => break,
                Err(e) => return Err(e),
            }
        }
        buffer.drain(..at);
        self.buffer = buffer;
        Ok(messages)
    }

    /// Parses one chunk from the start of `data`: how many bytes it took and
    /// the message it completed, if any. `None` when `data` is incomplete.
    fn chunk(&mut self, data: &[u8]) -> Result<Option<(usize, Option<Message>)>, String> {
        let Some(&first) = data.first() else {
            return Ok(None);
        };
        let format = first >> 6;
        let (csid, mut at) = match first & 0x3F {
            0 => (data.get(1).map(|&b| u32::from(b) + 64), 2),
            1 => (
                data.get(2)
                    .map(|&b| u32::from(data[1]) + u32::from(b) * 256 + 64),
                3,
            ),
            n => (Some(u32::from(n)), 1),
        };
        let Some(csid) = csid else { return Ok(None) };
        let header_len = [11, 7, 3, 0][usize::from(format)];
        let Some(header) = data.get(at..at + header_len) else {
            return Ok(None);
        };
        let mut state = self.streams.get(&csid).cloned().unwrap_or_default();
        if format != 0 && !self.streams.contains_key(&csid) {
            return Err(format!("chunk for unknown stream {csid}"));
        }
        let mut raw_ts = None;
        if format <= 2 {
            raw_ts = Some(u32::from_be_bytes([0, header[0], header[1], header[2]]));
        }
        if format <= 1 {
            state.length = u32::from_be_bytes([0, header[3], header[4], header[5]]) as usize;
            state.type_id = header[6];
        }
        if format == 0 {
            state.stream_id = u32::from_le_bytes([header[7], header[8], header[9], header[10]]);
        }
        at += header_len;
        if let Some(ts) = raw_ts {
            state.extended = ts == EXTENDED;
        }
        let mut ts_value = raw_ts;
        if state.extended {
            let Some(ext) = data.get(at..at + 4) else {
                return Ok(None);
            };
            ts_value = Some(u32::from_be_bytes([ext[0], ext[1], ext[2], ext[3]]));
            at += 4;
        }
        // A new message starts when nothing of the previous one is pending.
        if state.payload.is_empty() {
            match (format, ts_value) {
                (0, Some(t)) => state.timestamp = t,
                (1 | 2, Some(t)) => {
                    state.delta = t;
                    state.timestamp = state.timestamp.wrapping_add(t);
                }
                _ => state.timestamp = state.timestamp.wrapping_add(state.delta),
            }
        }
        let want = (state.length - state.payload.len()).min(self.chunk_size);
        let Some(body) = data.get(at..at + want) else {
            return Ok(None);
        };
        state.payload.extend(body);
        at += want;
        let message = (state.payload.len() == state.length).then(|| Message {
            csid,
            timestamp: state.timestamp,
            type_id: state.type_id,
            stream_id: state.stream_id,
            payload: std::mem::take(&mut state.payload),
        });
        self.streams.insert(csid, state);
        // Set Chunk Size changes how the very next chunk is cut.
        if let Some(m) = message
            .as_ref()
            .filter(|m| m.type_id == 1 && m.payload.len() == 4)
        {
            let size = u32::from_be_bytes([m.payload[0], m.payload[1], m.payload[2], m.payload[3]]);
            self.chunk_size = (size as usize).max(1);
        }
        Ok(Some((at, message)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(len: usize, ts: u32) -> Message {
        Message {
            csid: 4,
            timestamp: ts,
            type_id: 9,
            stream_id: 1,
            payload: (0..len).map(|i| i as u8).collect(),
        }
    }

    #[test]
    fn a_small_message_is_one_chunk() {
        let bytes = ChunkWriter::default().write(&message(10, 1000));
        assert_eq!(bytes.len(), 1 + 11 + 10);
        assert_eq!(bytes[0], 4);
        assert_eq!(&bytes[1..4], [0, 0x03, 0xE8]);
    }

    #[test]
    fn a_big_message_is_cut_with_continuation_headers() {
        let bytes = ChunkWriter::default().write(&message(300, 0));
        // 12-byte first header + 128, then 1-byte headers: 128 and 44.
        assert_eq!(bytes.len(), 12 + 128 + 1 + 128 + 1 + 44);
        assert_eq!(bytes[12 + 128], 0xC4);
    }

    #[test]
    fn writer_and_reader_agree_even_byte_by_byte() {
        let mut w = ChunkWriter::default();
        w.set_chunk_size(4096);
        let mut r = ChunkReader::default();
        r.set_chunk_size(4096);
        let sent = [
            message(10, 5),
            message(9000, 70_000),
            message(3, 20_000_000),
        ];
        let mut wire = Vec::new();
        for m in &sent {
            wire.extend(w.write(m));
        }
        let mut got = Vec::new();
        for b in &wire {
            got.extend(r.feed(&[*b]).unwrap());
        }
        assert_eq!(got, sent);
    }

    #[test]
    fn reads_the_short_headers_a_server_uses() {
        let mut r = ChunkReader::default();
        let mut wire = vec![3, 0, 0, 10, 0, 0, 2, 20, 0, 0, 0, 0, 7, 7]; // fmt 0, 2 bytes
        wire.extend([0x43, 0, 0, 5, 0, 0, 1, 20, 9]); // fmt 1: delta 5, 1 byte
        wire.extend([0xC3, 8]); // fmt 3: same again
        let got = r.feed(&wire).unwrap();
        let times: Vec<u32> = got.iter().map(|m| m.timestamp).collect();
        assert_eq!(times, [10, 15, 20]);
        assert_eq!(got[2].payload, [8]);
    }

    #[test]
    fn a_continuation_for_an_unknown_stream_is_an_error() {
        assert!(ChunkReader::default().feed(&[0xC5, 1, 2]).is_err());
    }
}
