//! knot — a from-scratch compression format.
//!
//! Stage 1: the real `.knot` header (magic, version, flags, filename, size,
//! CRC32) written in "stored" mode — the payload is still the raw bytes, but
//! everything around it is the actual format, and untie now verifies the
//! checksum. Compression lands in later stages.

pub mod bitio;
pub mod error;
pub mod format;
pub mod huffman;
pub mod lz77;

use std::fs;
use std::path::{Path, PathBuf};

use error::{KnotError, Result};
use format::Header;

/// Tie `input` into `<input>.knot` (stage 1: stored, uncompressed payload).
pub fn tie(input: &Path) -> Result<()> {
    let data = fs::read(input)?;

    let header = Header {
        version: format::VERSION,
        flags: format::FLAG_STORED,
        filename: basename(input),
        original_size: data.len() as u64,
        crc32: crc32(&data),
    };

    let out_path = knot_name(input);
    let mut bytes = header.to_bytes();
    bytes.extend_from_slice(&data);
    fs::write(&out_path, &bytes)?;

    println!(
        "tied {} -> {} ({} -> {} bytes)",
        input.display(),
        out_path.display(),
        data.len(),
        bytes.len()
    );
    Ok(())
}

/// Untie a `.knot` file back to the original bytes, verifying size + checksum.
pub fn untie(input: &Path, output: Option<&Path>) -> Result<()> {
    let file = fs::read(input)?;
    let (header, payload) = Header::parse(&file)?;

    let data = if header.is_stored() {
        payload.to_vec()
    } else {
        return Err(KnotError::Corrupt(
            "compressed payload, but this build only understands stored mode",
        ));
    };

    if data.len() as u64 != header.original_size {
        return Err(KnotError::SizeMismatch {
            expected: header.original_size,
            actual: data.len() as u64,
        });
    }
    let actual = crc32(&data);
    if actual != header.crc32 {
        return Err(KnotError::ChecksumMismatch {
            expected: header.crc32,
            actual,
        });
    }

    let out_path = match output {
        Some(path) => path.to_path_buf(),
        None => safe_output_name(&header.filename)?,
    };
    fs::write(&out_path, &data)?;

    println!(
        "untied {} -> {} ({} bytes, checksum OK)",
        input.display(),
        out_path.display(),
        data.len()
    );
    Ok(())
}

/// Print a `.knot` file's header without untying it.
pub fn inspect(input: &Path) -> Result<()> {
    let file = fs::read(input)?;
    let (header, payload) = Header::parse(&file)?;

    let pct = if header.original_size > 0 {
        file.len() as f64 / header.original_size as f64 * 100.0
    } else {
        0.0
    };

    println!("{}", input.display());
    println!("  format:          KNOT v{}", header.version);
    println!(
        "  mode:            {}",
        if header.is_stored() {
            "stored (uncompressed)"
        } else {
            "compressed"
        }
    );
    println!("  original name:   {}", header.filename);
    println!("  original size:   {} bytes", header.original_size);
    println!(
        "  on-disk size:    {} bytes (header + {} payload)",
        file.len(),
        payload.len()
    );
    println!("  ratio:           {pct:.1}% of original");
    println!("  crc32:           {:#010x}", header.crc32);
    Ok(())
}

// --- helpers ---

fn crc32(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

fn knot_name(input: &Path) -> PathBuf {
    let mut name = input.as_os_str().to_owned();
    name.push(".knot");
    PathBuf::from(name)
}

fn basename(input: &Path) -> String {
    input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("untied"))
}

/// Turn a *stored* filename into a safe output path: only its final component,
/// never an absolute or parent path. Neutralizes `../../etc/passwd`-style names.
fn safe_output_name(stored: &str) -> Result<PathBuf> {
    Path::new(stored)
        .file_name()
        .map(PathBuf::from)
        .ok_or(KnotError::BadFilename)
}
