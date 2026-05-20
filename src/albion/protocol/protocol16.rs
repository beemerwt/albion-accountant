use std::collections::BTreeMap;

use super::error::{DecodeError, DecodeResult};

#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolValue {
    UnsignedByte(u8),
    Byte(u8),
    UnsignedShort(u16),
    Short(i16),
    UnsignedInt(u32),
    Int(i32),
    UnsignedLong(u64),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    Bool(bool),
    ByteArray(Vec<u8>),
    Custom(u8, Box<ProtocolValue>),
    Object(Box<ProtocolValue>),
    Array(Vec<ProtocolValue>),
    Dictionary(BTreeMap<String, ProtocolValue>),
    Hashtable(BTreeMap<String, ProtocolValue>),
}

pub fn decode_typed_value(buf: &[u8], cursor: &mut usize) -> DecodeResult<ProtocolValue> {
    let ty = read_u8(buf, cursor)?;
    match ty {
        b'B' => Ok(ProtocolValue::UnsignedByte(read_u8(buf, cursor)?)),
        b'b' => Ok(ProtocolValue::Byte(read_u8(buf, cursor)?)),
        b'S' => Ok(ProtocolValue::UnsignedShort(read_u16(buf, cursor)?)),
        b's' => Ok(ProtocolValue::Short(read_i16(buf, cursor)?)),
        b'I' => Ok(ProtocolValue::UnsignedInt(read_u32(buf, cursor)?)),
        b'i' => Ok(ProtocolValue::Int(read_i32(buf, cursor)?)),
        b'L' => Ok(ProtocolValue::UnsignedLong(read_u64(buf, cursor)?)),
        b'l' => Ok(ProtocolValue::Long(read_i64(buf, cursor)?)),
        b'f' => Ok(ProtocolValue::Float(read_f32(buf, cursor)?)),
        b'g' => Ok(ProtocolValue::Double(read_f64(buf, cursor)?)),
        b't' => Ok(ProtocolValue::String(read_string(buf, cursor)?)),
        b'o' => Ok(ProtocolValue::Bool(read_u8(buf, cursor)? != 0)),
        b'x' => Ok(ProtocolValue::ByteArray(read_byte_array(buf, cursor)?)),
        b'c' => decode_custom(buf, cursor),
        b'w' => Ok(ProtocolValue::Object(Box::new(decode_typed_value(buf, cursor)?))),
        b'a' => decode_array(buf, cursor),
        b'd' => decode_map(buf, cursor).map(ProtocolValue::Dictionary),
        b'h' => decode_map(buf, cursor).map(ProtocolValue::Hashtable),
        _ => Err(DecodeError::Protocol16 {
            offset: *cursor - 1,
            reason: format!("unknown type tag '{}'", ty as char),
        }),
    }
}

fn decode_custom(buf: &[u8], cursor: &mut usize) -> DecodeResult<ProtocolValue> {
    let custom_ty = read_u8(buf, cursor)?;
    let wrapped = decode_typed_value(buf, cursor)?;
    Ok(ProtocolValue::Custom(custom_ty, Box::new(wrapped)))
}

fn read_byte_array(buf: &[u8], cursor: &mut usize) -> DecodeResult<Vec<u8>> {
    let len = read_u16(buf, cursor)? as usize;
    if *cursor + len > buf.len() {
        return Err(DecodeError::Protocol16 {
            offset: *cursor,
            reason: format!("byte array length {len} exceeds available {}", buf.len() - *cursor),
        });
    }
    let out = buf[*cursor..*cursor + len].to_vec();
    *cursor += len;
    Ok(out)
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
fn read_u32(buf: &[u8], cursor: &mut usize) -> DecodeResult<u32> {
    Ok(u32::from_be_bytes([
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
fn read_u64(buf: &[u8], cursor: &mut usize) -> DecodeResult<u64> {
    Ok(u64::from_be_bytes([
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
fn read_f32(buf: &[u8], cursor: &mut usize) -> DecodeResult<f32> {
    Ok(f32::from_bits(read_u32(buf, cursor)?))
}
fn read_f64(buf: &[u8], cursor: &mut usize) -> DecodeResult<f64> {
    Ok(f64::from_bits(read_u64(buf, cursor)?))
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
