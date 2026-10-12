//! Photoshop descriptor values with bounded recursion and counts.
use super::abr::Reader;
use serde_json::{Map, Value};

pub(super) fn decode(bytes: &[u8]) -> Result<Value, String> {
    let mut r = Reader::new(bytes);
    if r.u32()? != 16 {
        return Err("Unsupported ABR descriptor version".into());
    }
    object(&mut r, 0)
}
fn id(r: &mut Reader<'_>) -> Result<String, String> {
    let n = r.u32()? as usize;
    let n = if n == 0 { 4 } else { n };
    if n > 4096 {
        return Err("Excessive descriptor key".into());
    }
    Ok(String::from_utf8_lossy(r.take(n)?).into_owned())
}
fn object(r: &mut Reader<'_>, depth: usize) -> Result<Value, String> {
    if depth > 32 {
        return Err("Excessive brush descriptor nesting".into());
    }
    r.unicode()?;
    let class = id(r)?;
    let count = r.u32()? as usize;
    if count > 4096 {
        return Err("Excessive brush descriptor fields".into());
    }
    let mut map = Map::new();
    map.insert("$class".into(), Value::String(class));
    for _ in 0..count {
        let key = id(r)?;
        let kind = r.take(4)?;
        map.insert(key, value(r, kind, depth + 1)?);
    }
    Ok(Value::Object(map))
}
fn value(r: &mut Reader<'_>, kind: &[u8], depth: usize) -> Result<Value, String> {
    if depth > 32 {
        return Err("Excessive brush descriptor nesting".into());
    }
    Ok(match kind {
        b"Objc" | b"GlbO" => object(r, depth)?,
        b"VlLs" => {
            let count = r.u32()? as usize;
            if count > 8192 {
                return Err("Excessive brush descriptor list".into());
            }
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                let kind = r.take(4)?;
                list.push(value(r, kind, depth + 1)?);
            }
            Value::Array(list)
        }
        b"TEXT" => Value::String(r.unicode()?),
        b"UntF" | b"doub" => {
            if kind == b"UntF" {
                r.take(4)?;
            }
            let n = f64::from_be_bytes(r.take(8)?.try_into().unwrap());
            if !n.is_finite() {
                return Err("Invalid descriptor number".into());
            }
            Value::from(n)
        }
        b"long" => Value::from(r.i32()?),
        b"bool" => Value::Bool(r.u8()? != 0),
        b"enum" => {
            id(r)?;
            Value::String(id(r)?)
        }
        b"tdta" | b"alis" => {
            let n = r.u32()? as usize;
            r.take(n)?;
            Value::Null
        }
        b"type" | b"GlbC" => {
            r.unicode()?;
            Value::String(id(r)?)
        }
        b"comp" => {
            r.take(8)?;
            Value::Null
        }
        _ => {
            return Err(format!(
                "Unsupported descriptor type {}",
                String::from_utf8_lossy(kind)
            ));
        }
    })
}
