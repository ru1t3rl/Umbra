//! A bounds-checked big-endian reader over a datagram (NFR-2: no panic on any input).

use super::DecodeError;

/// Reads fixed-size big-endian fields from a slice; every read is checked.
pub(super) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Bytes not read yet.
    pub(super) fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let end = self.pos.saturating_add(N);
        let truncated = DecodeError::Truncated {
            needed: end,
            got: self.buf.len(),
        };
        let bytes = self.buf.get(self.pos..end).ok_or(truncated.clone())?;
        let array = <[u8; N]>::try_from(bytes).map_err(|_| truncated)?;
        self.pos = end;
        Ok(array)
    }

    pub(super) fn u8(&mut self) -> Result<u8, DecodeError> {
        self.array::<1>().map(|[b]| b)
    }

    pub(super) fn u16(&mut self) -> Result<u16, DecodeError> {
        self.array().map(u16::from_be_bytes)
    }

    pub(super) fn u32(&mut self) -> Result<u32, DecodeError> {
        self.array().map(u32::from_be_bytes)
    }

    pub(super) fn f32(&mut self) -> Result<f32, DecodeError> {
        self.array().map(f32::from_be_bytes)
    }

    pub(super) fn f64(&mut self) -> Result<f64, DecodeError> {
        self.array().map(f64::from_be_bytes)
    }

    pub(super) fn f32x3(&mut self) -> Result<[f32; 3], DecodeError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub(super) fn f64x3(&mut self) -> Result<[f64; 3], DecodeError> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }

    pub(super) fn bytes<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.array()
    }
}
