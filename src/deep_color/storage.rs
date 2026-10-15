//! Fallible native storage with allocation lifetime accounting.
use crate::resources::{self, Allocation, CancellationCheck, Lease};
use std::{io::Write, sync::Arc};

pub fn check() -> Result<(), String> {
    Ok(())
}

pub fn count(w: u32, h: u32, channels: usize) -> Result<usize, String> {
    if w == 0 || h == 0 {
        return Err("Pixel dimensions must be positive".into());
    }
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(channels))
        .filter(|n| *n <= isize::MAX as usize / std::mem::size_of::<f32>())
        .ok_or_else(|| "Pixel dimensions overflow addressable memory".into())
}

pub fn adopt<T: Copy>(values: Vec<T>) -> Result<Arc<Allocation<T>>, String> {
    let bytes = values
        .capacity()
        .checked_mul(std::mem::size_of::<T>())
        .ok_or("Pixel capacity overflows")?;
    let lease = resources::global().reserve(bytes as u64, &check)?;
    resources::global().adopt(values, lease)
}

pub fn collect<T: Copy>(values: impl IntoIterator<Item = T>) -> Result<Arc<Allocation<T>>, String> {
    let mut builder = Builder::new()?;
    for value in values {
        builder.extend(&[value])?;
    }
    builder.finish()
}

pub struct Builder<T: Copy> {
    values: Vec<T>,
    lease: Arc<Lease>,
}
impl<T: Copy> Builder<T> {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            values: Vec::new(),
            lease: resources::global().reserve(0, &check)?,
        })
    }
    pub fn extend(&mut self, values: &[T]) -> Result<(), String> {
        let len = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or("Pixel length overflows")?;
        if len > self.values.capacity() {
            let capacity = len.max(self.values.capacity().saturating_mul(2)).max(1024);
            let bytes = capacity
                .checked_mul(std::mem::size_of::<T>())
                .ok_or("Pixel capacity overflows")?;
            self.lease.resize(bytes as u64, &check)?;
            self.values
                .try_reserve_exact(capacity - self.values.len())
                .map_err(|_| "Not enough memory to read pixels")?;
            self.lease.resize(
                (self.values.capacity() * std::mem::size_of::<T>()) as u64,
                &check,
            )?;
        }
        self.values.extend_from_slice(values);
        Ok(())
    }
    pub fn finish(self) -> Result<Arc<Allocation<T>>, String> {
        resources::global().adopt(self.values, self.lease)
    }
}
impl Write for Builder<u8> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend(bytes).map_err(std::io::Error::other)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn make_mut<'a, T: Copy, C: CancellationCheck + ?Sized>(
    buffer: &'a mut Arc<Allocation<T>>,
    cancel: &C,
) -> Result<&'a mut [T], String> {
    if Arc::get_mut(buffer).is_none() {
        *buffer = Arc::new(buffer.try_clone(cancel)?);
    }
    Ok(Arc::get_mut(buffer).unwrap().as_mut_slice())
}

pub mod bytes {
    use super::*;
    use serde::{
        Deserializer, Serializer,
        de::{SeqAccess, Visitor},
    };
    pub fn serialize<S: Serializer>(
        data: &Arc<Allocation<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(data.as_slice(), serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Allocation<u8>>, D::Error> {
        struct Bytes;
        impl<'de> Visitor<'de> for Bytes {
            type Value = Arc<Allocation<u8>>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut builder = Builder::new().map_err(serde::de::Error::custom)?;
                while let Some(value) = sequence.next_element::<u8>()? {
                    builder.extend(&[value]).map_err(serde::de::Error::custom)?;
                }
                builder.finish().map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_seq(Bytes)
    }
}
