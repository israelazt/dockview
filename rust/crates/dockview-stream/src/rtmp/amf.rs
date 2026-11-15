//! AMF0, the little data format RTMP commands are written in.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
    String(String),
    /// Ordered key/value pairs (objects and ECMA arrays alike).
    Object(Vec<(String, Value)>),
    Null,
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::String(s.to_string())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// A property of an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Appends `value` in AMF0.
pub fn encode(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Number(n) => {
            out.push(0x00);
            out.extend(n.to_be_bytes());
        }
        Value::Bool(b) => out.extend([0x01, u8::from(*b)]),
        Value::String(s) => {
            out.push(0x02);
            put_str(s, out);
        }
        Value::Object(items) => {
            out.push(0x03);
            for (k, v) in items {
                put_str(k, out);
                encode(v, out);
            }
            out.extend([0, 0, 0x09]);
        }
        Value::Null => out.push(0x05),
    }
}

fn put_str(s: &str, out: &mut Vec<u8>) {
    out.extend((s.len() as u16).to_be_bytes());
    out.extend(s.as_bytes());
}

/// Encodes several values one after another (a command).
pub fn encode_all(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        encode(v, &mut out);
    }
    out
}

/// Reads every value of a command. `None` when the bytes are damaged or use a
/// type this client never receives.
pub fn decode_all(mut data: &[u8]) -> Option<Vec<Value>> {
    let mut values = Vec::new();
    while !data.is_empty() {
        values.push(decode(&mut data)?);
    }
    Some(values)
}

fn take<'a>(data: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    if data.len() < n {
        return None;
    }
    let (head, tail) = data.split_at(n);
    *data = tail;
    Some(head)
}

fn decode_str(data: &mut &[u8]) -> Option<String> {
    let len = take(data, 2)?;
    let bytes = take(data, usize::from(u16::from_be_bytes([len[0], len[1]])))?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn decode_pairs(data: &mut &[u8]) -> Option<Vec<(String, Value)>> {
    let mut items = Vec::new();
    loop {
        let key = decode_str(data)?;
        if key.is_empty() {
            return (take(data, 1)? == [0x09]).then_some(items);
        }
        items.push((key, decode(data)?));
    }
}

fn decode(data: &mut &[u8]) -> Option<Value> {
    let marker = take(data, 1)?[0];
    Some(match marker {
        0x00 => Value::Number(f64::from_be_bytes(take(data, 8)?.try_into().ok()?)),
        0x01 => Value::Bool(take(data, 1)?[0] != 0),
        0x02 => Value::String(decode_str(data)?),
        0x03 => Value::Object(decode_pairs(data)?),
        0x05 | 0x06 => Value::Null,
        0x08 => {
            take(data, 4)?; // the entry count, which the end marker makes redundant
            Value::Object(decode_pairs(data)?)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_survives_a_round_trip() {
        let command = vec![
            Value::str("connect"),
            Value::Number(1.0),
            Value::Object(vec![
                ("app".into(), Value::str("live")),
                ("fpad".into(), Value::Bool(false)),
            ]),
            Value::Null,
        ];
        assert_eq!(decode_all(&encode_all(&command)).unwrap(), command);
    }

    #[test]
    fn encodes_a_number_big_endian() {
        assert_eq!(
            encode_all(&[Value::Number(1.0)]),
            [0, 0x3F, 0xF0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn reads_an_ecma_array_and_undefined() {
        let mut d = vec![0x08, 0, 0, 0, 1, 0, 1, b'k', 0x02, 0, 1, b'v', 0, 0, 9];
        d.push(0x06);
        let v = decode_all(&d).unwrap();
        assert_eq!(v[0].get("k").and_then(Value::as_str), Some("v"));
        assert_eq!(v[1], Value::Null);
    }

    #[test]
    fn truncated_or_unknown_data_is_none() {
        assert!(decode_all(&[0x02, 0, 5, b'a']).is_none());
        assert!(decode_all(&[0x0A]).is_none());
        assert!(decode_all(&[0x03, 0, 1, b'k']).is_none());
        assert_eq!(decode_all(&[]).unwrap(), vec![]);
    }

    #[test]
    fn accessors_pick_the_right_type() {
        let o = Value::Object(vec![
            ("code".into(), Value::str("x")),
            ("n".into(), Value::Number(4.0)),
        ]);
        assert_eq!(o.get("n").and_then(Value::as_number), Some(4.0));
        assert_eq!(o.get("n").and_then(Value::as_str), None);
        assert!(o.get("missing").is_none());
    }
}
