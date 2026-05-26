use crate::{
    error::Result,
    util::{bytes_value, hex_lower, hex_upper, json_key, params_to_json},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.pos)
            .ok_or("Unexpected end of Protocol18 stream")?;
        self.pos += 1;
        Ok(byte)
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.remaining() < count {
            return Err(format!("Expected {count} bytes, got {}", self.remaining()).into());
        }
        let start = self.pos;
        self.pos += count;
        Ok(&self.data[start..self.pos])
    }

    fn read_i16_le(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    fn read_u16_le(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    fn read_f32_le(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    fn read_f64_le(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    fn peek_hex(&self) -> String {
        let end = (self.pos + 16).min(self.data.len());
        let mut text = hex_upper(&self.data[self.pos..end]);
        if end < self.data.len() {
            text.push_str(" ...");
        }
        text
    }
}

pub struct Protocol18Deserializer;

impl Protocol18Deserializer {
    pub fn deserialize_operation_request(
        &self,
        payload: &[u8],
    ) -> Result<(u8, BTreeMap<u8, Value>)> {
        let mut reader = Reader::new(payload);
        let operation_code = reader.read_u8()?;
        Ok((
            operation_code,
            self.deserialize_parameter_table(&mut reader)?,
        ))
    }

    pub fn deserialize_operation_response(
        &self,
        payload: &[u8],
    ) -> Result<(u8, i16, String, BTreeMap<u8, Value>)> {
        let mut reader = Reader::new(payload);
        let operation_code = reader.read_u8()?;
        let return_code = reader.read_i16_le()?;
        let mut debug_message = String::new();
        if reader.remaining() > 0 {
            let type_code = reader.read_u8()?;
            if let Value::String(value) = self.deserialize(&mut reader, Some(type_code))? {
                debug_message = value;
            }
        }
        Ok((
            operation_code,
            return_code,
            debug_message,
            self.deserialize_parameter_table(&mut reader)?,
        ))
    }

    pub fn deserialize_event_data(&self, payload: &[u8]) -> Result<(u8, BTreeMap<u8, Value>)> {
        let mut reader = Reader::new(payload);
        let event_code = reader.read_u8()?;
        Ok((event_code, self.deserialize_parameter_table(&mut reader)?))
    }

    fn deserialize(&self, reader: &mut Reader<'_>, type_code: Option<u8>) -> Result<Value> {
        let type_code = match type_code {
            Some(value) => value,
            None => reader.read_u8()?,
        };

        if (0x80..=228).contains(&type_code) {
            return self.deserialize_custom(reader, type_code);
        }

        match type_code {
            0 | 8 => Ok(Value::Null),
            2 => Ok(Value::Bool(reader.read_u8()? != 0)),
            3 => Ok(json!(reader.read_u8()?)),
            4 => Ok(json!(reader.read_i16_le()?)),
            5 => Ok(json!(reader.read_f32_le()?)),
            6 => Ok(json!(reader.read_f64_le()?)),
            7 => self.read_string(reader).map(Value::String),
            9 => Ok(json!(self.read_compressed_i32(reader)?)),
            10 => Ok(json!(self.read_compressed_i64(reader)?)),
            11 => Ok(json!(reader.read_u8()?)),
            12 => Ok(json!(-(reader.read_u8()? as i32))),
            13 => Ok(json!(reader.read_u16_le()?)),
            14 => Ok(json!(-(reader.read_u16_le()? as i32))),
            15 => Ok(json!(reader.read_u8()?)),
            16 => Ok(json!(-(reader.read_u8()? as i64))),
            17 => Ok(json!(reader.read_u16_le()?)),
            18 => Ok(json!(-(reader.read_u16_le()? as i64))),
            19 => self.deserialize_custom(reader, 19),
            20 => self.deserialize_dictionary(reader),
            21 => self.deserialize_hashtable(reader),
            23 => self.deserialize_object_array(reader),
            24 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, params) = self.deserialize_operation_request(payload)?;
                Ok(json!([code, params_to_json(&params)]))
            }
            25 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, rc, msg, params) = self.deserialize_operation_response(payload)?;
                Ok(json!([code, rc, msg, params_to_json(&params)]))
            }
            26 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, params) = self.deserialize_event_data(payload)?;
                Ok(json!([code, params_to_json(&params)]))
            }
            27 => Ok(Value::Bool(false)),
            28 => Ok(Value::Bool(true)),
            29 | 30 | 31 | 34 => Ok(json!(0)),
            32 | 33 => Ok(json!(0.0)),
            0x40 => self.deserialize_array_in_array(reader),
            66 => self.deserialize_boolean_array(reader),
            67 => {
                let len = self.read_count(reader)?;
                Ok(bytes_value(reader.read_bytes(len)?))
            }
            68 => self.read_typed_array(reader, |_s, r| Ok(json!(r.read_i16_le()?))),
            69 => self.read_typed_array(reader, |_s, r| Ok(json!(r.read_f32_le()?))),
            70 => self.read_typed_array(reader, |_s, r| Ok(json!(r.read_f64_le()?))),
            71 => self.read_typed_array(reader, |s, r| s.read_string(r).map(Value::String)),
            73 => self.read_typed_array(reader, |s, r| Ok(json!(s.read_compressed_i32(r)?))),
            74 => self.read_typed_array(reader, |s, r| Ok(json!(s.read_compressed_i64(r)?))),
            83 => self.deserialize_custom_type_array(reader),
            84 => {
                let (key_type, value_type) = self.deserialize_dictionary_type(reader)?;
                self.read_typed_array(reader, |s, r| {
                    s.deserialize_dictionary_elements(r, key_type, value_type)
                })
            }
            85 => self.read_typed_array(reader, |s, r| s.deserialize_hashtable(r)),
            _ => Err(format!("Protocol18 type code {type_code} is not implemented").into()),
        }
    }

    fn deserialize_parameter_table(&self, reader: &mut Reader<'_>) -> Result<BTreeMap<u8, Value>> {
        let size = reader.read_u8()?;
        let mut params = BTreeMap::new();
        for index in 0..size {
            let start = reader.pos;
            let key = reader.read_u8()?;
            let value_type = reader.read_u8()?;
            match self.deserialize(reader, Some(value_type)) {
                Ok(value) => {
                    params.insert(key, value);
                }
                Err(err) => {
                    return Err(format!(
                        "Failed to deserialize parameter index={index} key={key} value_type=0x{value_type:02X} position={start} remaining={} next={}: {}",
                        reader.remaining(),
                        reader.peek_hex(),
                        err.0
                    )
                    .into());
                }
            }
        }
        Ok(params)
    }

    fn deserialize_dictionary(&self, reader: &mut Reader<'_>) -> Result<Value> {
        let (key_type, value_type) = self.deserialize_dictionary_type(reader)?;
        self.deserialize_dictionary_elements(reader, key_type, value_type)
    }

    fn deserialize_hashtable(&self, reader: &mut Reader<'_>) -> Result<Value> {
        let mut map = Map::new();
        for _ in 0..self.read_count(reader)? {
            let key = self.deserialize(reader, None)?;
            let value = self.deserialize(reader, None)?;
            if !key.is_null() {
                map.insert(json_key(&key), value);
            }
        }
        Ok(Value::Object(map))
    }

    fn deserialize_dictionary_type(&self, reader: &mut Reader<'_>) -> Result<(u8, u8)> {
        let key_type = reader.read_u8()?;
        let mut value_type = reader.read_u8()?;
        if value_type == 20 {
            let _ = self.deserialize_dictionary_type(reader)?;
        } else if value_type == 0x40 {
            self.consume_dictionary_array_type(reader)?;
            value_type = 0;
        }
        Ok((key_type, value_type))
    }

    fn consume_dictionary_array_type(&self, reader: &mut Reader<'_>) -> Result<()> {
        let mut type_code = reader.read_u8()?;
        while type_code == 0x40 {
            type_code = reader.read_u8()?;
        }
        Ok(())
    }

    fn deserialize_dictionary_elements(
        &self,
        reader: &mut Reader<'_>,
        key_type: u8,
        value_type: u8,
    ) -> Result<Value> {
        let mut map = Map::new();
        for _ in 0..self.read_count(reader)? {
            let key = if key_type == 0 {
                self.deserialize(reader, None)?
            } else {
                self.deserialize(reader, Some(key_type))?
            };
            let value = if value_type == 0 {
                self.deserialize(reader, None)?
            } else {
                self.deserialize(reader, Some(value_type))?
            };
            if !key.is_null() {
                map.insert(json_key(&key), value);
            }
        }
        Ok(Value::Object(map))
    }

    fn deserialize_object_array(&self, reader: &mut Reader<'_>) -> Result<Value> {
        self.read_typed_array(reader, |s, r| s.deserialize(r, None))
    }

    fn deserialize_array_in_array(&self, reader: &mut Reader<'_>) -> Result<Value> {
        self.read_typed_array(reader, |s, r| s.deserialize(r, None))
    }

    fn deserialize_boolean_array(&self, reader: &mut Reader<'_>) -> Result<Value> {
        let len = self.read_count(reader)?;
        let mut result = Vec::with_capacity(len);
        while result.len() < len {
            let value = reader.read_u8()?;
            for bit_index in 0..8 {
                if result.len() >= len {
                    break;
                }
                result.push(Value::Bool((value & (1 << bit_index)) != 0));
            }
        }
        Ok(Value::Array(result))
    }

    fn deserialize_custom_type_array(&self, reader: &mut Reader<'_>) -> Result<Value> {
        let len = self.read_count(reader)?;
        let type_code = reader.read_u8()?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.deserialize_custom_payload(reader, type_code)?);
        }
        Ok(Value::Array(values))
    }

    fn deserialize_custom(&self, reader: &mut Reader<'_>, gp_type: u8) -> Result<Value> {
        let type_code = if gp_type == 19 {
            reader.read_u8()?
        } else {
            gp_type - 0x80
        };
        self.deserialize_custom_payload(reader, type_code)
    }

    fn deserialize_custom_payload(&self, reader: &mut Reader<'_>, type_code: u8) -> Result<Value> {
        let len = self.read_count(reader)?;
        Ok(json!({"type_code": type_code, "data_hex": hex_lower(reader.read_bytes(len)?) }))
    }

    fn read_string(&self, reader: &mut Reader<'_>) -> Result<String> {
        let len = self.read_count(reader)?;
        Ok(String::from_utf8_lossy(reader.read_bytes(len)?).into_owned())
    }

    fn read_count(&self, reader: &mut Reader<'_>) -> Result<usize> {
        Ok(self.read_compressed_u32(reader)? as usize)
    }

    fn read_compressed_u32(&self, reader: &mut Reader<'_>) -> Result<u32> {
        let mut value = 0u32;
        let mut shift = 0;
        while shift < 35 {
            let current = reader.read_u8()? as u32;
            value |= (current & 0x7f) << shift;
            if (current & 0x80) == 0 {
                return Ok(value);
            }
            shift += 7;
        }
        Err("Compressed UInt32 is too large".into())
    }

    fn read_compressed_u64(&self, reader: &mut Reader<'_>) -> Result<u64> {
        let mut value = 0u64;
        let mut shift = 0;
        while shift < 70 {
            let current = reader.read_u8()? as u64;
            value |= (current & 0x7f) << shift;
            if (current & 0x80) == 0 {
                return Ok(value);
            }
            shift += 7;
        }
        Err("Compressed UInt64 is too large".into())
    }

    fn read_compressed_i32(&self, reader: &mut Reader<'_>) -> Result<i32> {
        let value = self.read_compressed_u32(reader)?;
        Ok(((value >> 1) as i32) ^ -((value & 1) as i32))
    }

    fn read_compressed_i64(&self, reader: &mut Reader<'_>) -> Result<i64> {
        let value = self.read_compressed_u64(reader)?;
        Ok(((value >> 1) as i64) ^ -((value & 1) as i64))
    }

    fn read_typed_array<F>(&self, reader: &mut Reader<'_>, mut f: F) -> Result<Value>
    where
        F: FnMut(&Self, &mut Reader<'_>) -> Result<Value>,
    {
        let len = self.read_count(reader)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(f(self, reader)?);
        }
        Ok(Value::Array(values))
    }
}
