//! On-disk layout of a `.knot` file. This module is the *only* place that
//! knows the byte format; everything else goes through `Header`.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "KNOT"
//! 4       1     version
//! 5       1     flags (bit0 = stored/uncompressed)
//! 6       2     filename length N (u16, little-endian)
//! 8       N     original filename (UTF-8, basename only)
//! 8+N     8     original size (u64, little-endian)
//! 16+N    4     CRC32 of the original bytes (u32, little-endian)
//! 20+N    ..    payload (raw bytes if stored, else compressed)
//! ```

use crate::error::{KnotError, Result};

pub const MAGIC: [u8; 4] = *b"KNOT";
pub const VERSION: u8 = 1;
pub const FLAG_STORED: u8 = 0b0000_0001;

/// A parsed `.knot` header (everything before the payload).
#[derive(Debug, Clone)]
pub struct Header {
    pub version: u8,
    pub flags: u8,
    pub filename: String,
    pub original_size: u64,
    pub crc32: u32,
}

impl Header {
    pub fn is_stored(&self) -> bool {
        self.flags & FLAG_STORED != 0
    }

    /// Serialize just the header (the payload is appended separately).
    pub fn to_bytes(&self) -> Vec<u8> {
        let name = self.filename.as_bytes();
        // Real filenames are far shorter than 64 KiB, but clamp defensively
        // so the u16 length field can never overflow/lie.
        let name_len = name.len().min(u16::MAX as usize);

        let mut out = Vec::with_capacity(20 + name_len);
        out.extend_from_slice(&MAGIC);
        out.push(self.version);
        out.push(self.flags);
        out.extend_from_slice(&(name_len as u16).to_le_bytes());
        out.extend_from_slice(&name[..name_len]);
        out.extend_from_slice(&self.original_size.to_le_bytes());
        out.extend_from_slice(&self.crc32.to_le_bytes());
        out
    }

    /// Parse a header from the front of `data`, returning it plus the payload
    /// slice that follows. The returned slice borrows from `data` — see the
    /// shared lifetime in `take` below.
    pub fn parse(data: &[u8]) -> Result<(Header, &[u8])> {
        let mut off = 0usize;

        if take(data, &mut off, 4)? != MAGIC {
            return Err(KnotError::BadMagic);
        }

        let version = take(data, &mut off, 1)?[0];
        if version != VERSION {
            return Err(KnotError::UnsupportedVersion(version));
        }

        let flags = take(data, &mut off, 1)?[0];

        let name_len = u16::from_le_bytes(arr2(take(data, &mut off, 2)?)) as usize;
        let name_bytes = take(data, &mut off, name_len)?;
        let filename =
            String::from_utf8(name_bytes.to_vec()).map_err(|_| KnotError::BadFilename)?;

        let original_size = u64::from_le_bytes(arr8(take(data, &mut off, 8)?));
        let crc32 = u32::from_le_bytes(arr4(take(data, &mut off, 4)?));

        let payload = &data[off..];
        Ok((
            Header {
                version,
                flags,
                filename,
                original_size,
                crc32,
            },
            payload,
        ))
    }
}

/// Advance `off` by `n` bytes and return that slice, erroring if `data` ends early.
fn take<'a>(data: &'a [u8], off: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = off.checked_add(n).ok_or(KnotError::Truncated)?;
    let slice = data.get(*off..end).ok_or(KnotError::Truncated)?;
    *off = end;
    Ok(slice)
}

// These can't fail: `take` always returns exactly the requested length.
fn arr2(s: &[u8]) -> [u8; 2] {
    s.try_into().expect("take returned 2 bytes")
}
fn arr4(s: &[u8]) -> [u8; 4] {
    s.try_into().expect("take returned 4 bytes")
}
fn arr8(s: &[u8]) -> [u8; 8] {
    s.try_into().expect("take returned 8 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let header = Header {
            version: VERSION,
            flags: FLAG_STORED,
            filename: "hello.txt".to_string(),
            original_size: 12345,
            crc32: 0xDEAD_BEEF,
        };
        let mut bytes = header.to_bytes();
        bytes.extend_from_slice(b"the payload");

        let (parsed, payload) = Header::parse(&bytes).unwrap();
        assert_eq!(parsed.version, VERSION);
        assert!(parsed.is_stored());
        assert_eq!(parsed.filename, "hello.txt");
        assert_eq!(parsed.original_size, 12345);
        assert_eq!(parsed.crc32, 0xDEAD_BEEF);
        assert_eq!(payload, b"the payload");
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(
            Header::parse(b"NOPE and then some").unwrap_err(),
            KnotError::BadMagic
        ));
    }

    #[test]
    fn rejects_truncated() {
        assert!(matches!(
            Header::parse(b"KN").unwrap_err(),
            KnotError::Truncated
        ));
    }
}
