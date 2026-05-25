use std::collections::BTreeMap;
use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

pub struct Protocol18Deserializer;
impl Protocol18Deserializer {
    pub fn new() -> Self { Self }
    pub fn deserialize_operation_request(&self, p: &[u8]) -> Result<(u8, BTreeMap<i32, Value>)> { let mut c=Cursor::new(p); let op=c.read_u8()?; Ok((op, self.deserialize_parameter_table(&mut c)?)) }
    pub fn deserialize_operation_response(&self, p: &[u8]) -> Result<(u8, i16, String, BTreeMap<i32, Value>)> { let mut c=Cursor::new(p); let op=c.read_u8()?; let rc=c.read_i16_le()?; let mut dbg=String::new(); if c.remaining()>0 { let t=c.read_u8()?; if let Value::String(s)=self.deserialize(&mut c,Some(t))? { dbg=s; } } Ok((op, rc, dbg, self.deserialize_parameter_table(&mut c)?)) }
    pub fn deserialize_event_data(&self, p: &[u8]) -> Result<(u8, BTreeMap<i32, Value>)> { let mut c=Cursor::new(p); let ec=c.read_u8()?; Ok((ec, self.deserialize_parameter_table(&mut c)?)) }
    fn deserialize_parameter_table(&self, c:&mut Cursor) -> Result<BTreeMap<i32, Value>> { let mut m=BTreeMap::new(); for _ in 0..c.read_u8()? { let k=c.read_u8()? as i32; let t=c.read_u8()?; m.insert(k, self.deserialize(c, Some(t))?);} Ok(m)}
    pub fn deserialize(&self, c:&mut Cursor, t:Option<u8>) -> Result<Value> { let tc=t.unwrap_or(c.read_u8()?); match tc {0|8=>Ok(Value::Null),2=>Ok(Value::Bool(c.read_u8()?!=0)),3=>Ok(Value::from(c.read_u8()? as u64)),4=>Ok(Value::from(c.read_i16_le()? as i64)),7=>Ok(Value::String(c.read_string()?)),9=>Ok(Value::from(c.read_compressed_i32()? as i64)),19=>{let ty=c.read_u8()?; let n=c.read_count()?; let d=c.read_vec(n)?; let mut o=Map::new(); o.insert("type_code".into(), Value::from(ty)); o.insert("data_hex".into(), Value::String(to_hex(&d))); Ok(Value::Object(o))},23=>{let n=c.read_count()?; Ok(Value::Array((0..n).map(|_|self.deserialize(c,None).unwrap_or(Value::Null)).collect()))},27=>Ok(Value::Bool(false)),28=>Ok(Value::Bool(true)),29|30|31|34=>Ok(Value::from(0)),32|33=>Ok(Value::from(0.0)),67=>{let n=c.read_count()?; Ok(Value::String(to_hex(&c.read_vec(n)?)))},71=>{let n=c.read_count()?; Ok(Value::Array((0..n).map(|_|Value::String(c.read_string().unwrap_or_default())).collect()))},_=>Err(anyhow!("Protocol18 type code {} is not implemented",tc)) } }
}

pub struct Cursor<'a>{buf:&'a [u8],pos:usize}
impl<'a> Cursor<'a>{fn new(b:&'a [u8])->Self{Self{buf:b,pos:0}} fn remaining(&self)->usize{self.buf.len().saturating_sub(self.pos)} fn read_u8(&mut self)->Result<u8>{if self.pos>=self.buf.len(){return Err(anyhow!("EOF"))} let v=self.buf[self.pos]; self.pos+=1; Ok(v)} fn read_vec(&mut self,n:usize)->Result<Vec<u8>>{if self.pos+n>self.buf.len(){return Err(anyhow!("EOF"))} let v=self.buf[self.pos..self.pos+n].to_vec(); self.pos+=n; Ok(v)} fn read_i16_le(&mut self)->Result<i16>{let b=self.read_vec(2)?; Ok(i16::from_le_bytes([b[0],b[1]]))} fn read_count(&mut self)->Result<usize>{Ok(self.read_compressed_u32()? as usize)} fn read_string(&mut self)->Result<String>{let n=self.read_count()?; Ok(String::from_utf8_lossy(&self.read_vec(n)?).to_string())} fn read_compressed_u32(&mut self)->Result<u32>{let(mut v,mut s)=(0u32,0); while s<35 {let c=self.read_u8()?; v|=((c&0x7f) as u32)<<s; if c&0x80==0{return Ok(v)} s+=7;} Err(anyhow!("Compressed UInt32 too large"))} fn read_compressed_i32(&mut self)->Result<i32>{let v=self.read_compressed_u32()?; Ok(((v>>1) as i32)^(-((v&1) as i32)))} }

fn to_hex(data:&[u8])->String{data.iter().map(|b|format!("{:02x}",b)).collect()}
