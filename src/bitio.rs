//! Bit-level I/O, most-significant-bit-first.
//!
//! The first bit written becomes the top bit of the first output byte, and the
//! reader consumes bits in that same order. We pick MSB-first (rather than
//! DEFLATE's LSB-first) because we don't need DEFLATE interop, and it makes the
//! canonical-Huffman decoder a clean bit-by-bit walk: the integer you rebuild
//! by shifting bits in left-to-right *is* the canonical code.

use crate::error::{KnotError, Result};

/// Accumulates bits and produces a byte buffer.
pub struct BitWriter {
    bytes: Vec<u8>,
    bitbuf: u64,   // pending bits, held in the low `bitcount` bits
    bitcount: u32, // how many pending bits are valid
}

impl BitWriter {
    pub fn new() -> Self {
        BitWriter {
            bytes: Vec::new(),
            bitbuf: 0,
            bitcount: 0,
        }
    }

    /// Write the low `count` bits of `value`, most-significant of those first.
    pub fn write_bits(&mut self, value: u32, count: u8) {
        debug_assert!(count <= 32);
        let count = count as u32;
        self.bitbuf = (self.bitbuf << count) | (value as u64 & mask(count));
        self.bitcount += count;
        while self.bitcount >= 8 {
            self.bitcount -= 8;
            self.bytes.push((self.bitbuf >> self.bitcount) as u8);
            self.bitbuf &= mask(self.bitcount);
        }
    }

    /// Flush remaining bits (zero-padding the final byte's low end) and return
    /// the finished buffer. Consumes the writer.
    pub fn finish(mut self) -> Vec<u8> {
        if self.bitcount > 0 {
            let byte = (self.bitbuf << (8 - self.bitcount)) as u8;
            self.bytes.push(byte);
            self.bitbuf = 0;
            self.bitcount = 0;
        }
        self.bytes
    }
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads bits from a byte slice, most-significant-first.
pub struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    bitbuf: u64,
    bitcount: u32,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        BitReader {
            bytes,
            pos: 0,
            bitbuf: 0,
            bitcount: 0,
        }
    }

    /// Read `count` bits (most-significant first), returned right-aligned in the
    /// result. Errors with `Truncated` if the stream runs out.
    pub fn read_bits(&mut self, count: u8) -> Result<u32> {
        debug_assert!(count <= 32);
        let count = count as u32;
        while self.bitcount < count {
            let byte = *self.bytes.get(self.pos).ok_or(KnotError::Truncated)?;
            self.pos += 1;
            self.bitbuf = (self.bitbuf << 8) | byte as u64;
            self.bitcount += 8;
        }
        self.bitcount -= count;
        let value = ((self.bitbuf >> self.bitcount) & mask(count)) as u32;
        self.bitbuf &= mask(self.bitcount);
        Ok(value)
    }
}

/// Mask of the low `n` bits (handles `n` in `0..=64`).
fn mask(n: u32) -> u64 {
    if n >= 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_round_trip() {
        let mut w = BitWriter::new();
        w.write_bits(0b1, 1);
        w.write_bits(0b101, 3);
        w.write_bits(0b0, 1);
        w.write_bits(0xABCD, 16);
        w.write_bits(0b11, 2);
        let bytes = w.finish();

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(1).unwrap(), 0b1);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(1).unwrap(), 0b0);
        assert_eq!(r.read_bits(16).unwrap(), 0xABCD);
        assert_eq!(r.read_bits(2).unwrap(), 0b11);
    }

    #[test]
    fn reading_past_end_errors() {
        let mut r = BitReader::new(&[0xFF]);
        assert!(r.read_bits(8).is_ok());
        assert!(r.read_bits(1).is_err());
    }

    #[test]
    fn zero_bit_reads_and_writes() {
        let mut w = BitWriter::new();
        w.write_bits(0, 0);
        w.write_bits(0xFF, 8);
        let bytes = w.finish();
        assert_eq!(bytes, vec![0xFF]);

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(0).unwrap(), 0);
        assert_eq!(r.read_bits(8).unwrap(), 0xFF);
    }
}
