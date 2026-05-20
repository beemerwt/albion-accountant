use std::collections::BTreeMap;

use super::error::{DecodeError, DecodeResult};

#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolValue {
    Byte(u8),
    Short(i16),
    Int(i32),
    Long(i64),
    String(String),
    Bool(bool),
    Array(Vec<ProtocolValue>),
    Dictionary(BTreeMap<String, ProtocolValue>),
    Hashtable(BTreeMap<String, ProtocolValue>),
}

pub fn decode_typed_value(buf: &[u8], cursor: &mut usize) -> DecodeResult<ProtocolValue> {
    let ty = read_u8(buf, cursor)?;
    match ty {
        b'b' => Ok(ProtocolValue::Byte(read_u8(buf, cursor)?)),
        b's' => Ok(ProtocolValue::Short(read_i16(buf, cursor)?)),
        b'i' => Ok(ProtocolValue::Int(read_i32(buf, cursor)?)),
        b'l' => Ok(ProtocolValue::Long(read_i64(buf, cursor)?)),
        b't' => Ok(ProtocolValue::String(read_string(buf, cursor)?)),
        b'o' => Ok(ProtocolValue::Bool(read_u8(buf, cursor)? != 0)),
        b'a' => decode_array(buf, cursor),
        b'd' => decode_map(buf, cursor).map(ProtocolValue::Dictionary),
        b'h' => decode_map(buf, cursor).map(ProtocolValue::Hashtable),
        _ => Err(DecodeError::Protocol16 {
            offset: *cursor - 1,
            reason: format!("unknown type tag '{}'", ty as char),
        }),
    }
}

fn decode_array(buf: &[u8], cursor: &mut usize) -> DecodeResult<ProtocolValue> {
    let count = read_u16(buf, cursor)? as usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(decode_typed_value(buf, cursor)?);
    }
    Ok(ProtocolValue::Array(items))
}

fn decode_map(buf: &[u8], cursor: &mut usize) -> DecodeResult<BTreeMap<String, ProtocolValue>> {
    let count = read_u16(buf, cursor)? as usize;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        let key = read_string(buf, cursor)?;
        let value = decode_typed_value(buf, cursor)?;
        out.insert(key, value);
    }
    Ok(out)
}

pub fn read_u8(buf: &[u8], cursor: &mut usize) -> DecodeResult<u8> {
    if *cursor >= buf.len() {
        return Err(DecodeError::Protocol16 {
            offset: *cursor,
            reason: "unexpected eof while reading u8".into(),
        });
    }
    let b = buf[*cursor];
    *cursor += 1;
    Ok(b)
}
fn read_u16(buf: &[u8], cursor: &mut usize) -> DecodeResult<u16> {
    Ok(u16::from_be_bytes([
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
    ]))
}
fn read_i16(buf: &[u8], cursor: &mut usize) -> DecodeResult<i16> {
    Ok(i16::from_be_bytes([
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
    ]))
}
fn read_i32(buf: &[u8], cursor: &mut usize) -> DecodeResult<i32> {
    Ok(i32::from_be_bytes([
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
    ]))
}
fn read_i64(buf: &[u8], cursor: &mut usize) -> DecodeResult<i64> {
    Ok(i64::from_be_bytes([
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
        read_u8(buf, cursor)?,
    ]))
}
fn read_string(buf: &[u8], cursor: &mut usize) -> DecodeResult<String> {
    let len = read_u16(buf, cursor)? as usize;
    if *cursor + len > buf.len() {
        return Err(DecodeError::Protocol16 {
            offset: *cursor,
            reason: format!(
                "string length {len} exceeds available {}",
                buf.len() - *cursor
            ),
        });
    }
    let text = std::str::from_utf8(&buf[*cursor..*cursor + len])
        .map_err(|e| DecodeError::Protocol16 {
            offset: *cursor,
            reason: e.to_string(),
        })?
        .to_string();
    *cursor += len;
    Ok(text)
}
