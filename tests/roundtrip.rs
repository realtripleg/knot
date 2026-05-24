//! End-to-end tests of the public `tie`/`untie` pipeline — header, checksum,
//! stored fallback, and compression — exercised through real files in a temp
//! directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("knot-it-{nanos}-{tag}"))
}

fn knot_path(src: &Path) -> PathBuf {
    let mut s = src.as_os_str().to_owned();
    s.push(".knot");
    PathBuf::from(s)
}

fn check_roundtrip(tag: &str, data: &[u8]) {
    let src = unique(tag);
    fs::write(&src, data).unwrap();
    knot::tie(&src).unwrap();

    let kn = knot_path(&src);
    let out = unique(&format!("{tag}-out"));
    knot::untie(&kn, Some(&out)).unwrap();

    assert_eq!(fs::read(&out).unwrap(), data, "round-trip mismatch for {tag}");

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&kn);
    let _ = fs::remove_file(&out);
}

#[test]
fn empty_file() {
    check_roundtrip("empty", b"");
}

#[test]
fn tiny_text() {
    check_roundtrip("tiny", b"shoelaces");
}

#[test]
fn compressible_text() {
    let data = b"the quick brown fox ".repeat(500);
    check_roundtrip("compressible", &data);
}

#[test]
fn incompressible_uses_stored_fallback() {
    // Pseudo-random data can't shrink, so the stored fallback should keep the
    // .knot from growing beyond the original (plus the small header).
    let mut data = Vec::new();
    let mut x = 0x1234_5678u32;
    for _ in 0..20_000 {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        data.push((x >> 24) as u8);
    }

    let src = unique("incompressible");
    fs::write(&src, &data).unwrap();
    knot::tie(&src).unwrap();

    let kn = knot_path(&src);
    let knot_size = fs::metadata(&kn).unwrap().len() as usize;
    assert!(
        knot_size <= data.len() + 128,
        "stored fallback should avoid bloat: {knot_size} vs {}",
        data.len()
    );

    let out = unique("incompressible-out");
    knot::untie(&kn, Some(&out)).unwrap();
    assert_eq!(fs::read(&out).unwrap(), data);

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&kn);
    let _ = fs::remove_file(&out);
}

#[test]
fn truncated_archive_is_detected() {
    let data = b"integrity matters; tie it tight. ".repeat(300);
    let src = unique("truncated");
    fs::write(&src, &data).unwrap();
    knot::tie(&src).unwrap();

    let kn = knot_path(&src);
    let mut bytes = fs::read(&kn).unwrap();
    bytes.truncate(bytes.len() * 3 / 4); // lop off the tail of the payload
    fs::write(&kn, &bytes).unwrap();

    let out = unique("truncated-out");
    assert!(
        knot::untie(&kn, Some(&out)).is_err(),
        "a truncated .knot must be rejected, never silently restored"
    );

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&kn);
    let _ = fs::remove_file(&out);
}

#[test]
fn tampered_checksum_is_detected() {
    let data = b"integrity matters; tie it tight. ".repeat(300);
    let src = unique("checksum");
    fs::write(&src, &data).unwrap();
    knot::tie(&src).unwrap();

    let kn = knot_path(&src);
    let mut bytes = fs::read(&kn).unwrap();
    // The CRC32 field sits at offset 16 + filename_len (see the format spec).
    // The payload stays intact, so the data decodes fine but no longer matches
    // the recorded checksum.
    let crc_offset = 16 + src.file_name().unwrap().as_encoded_bytes().len();
    bytes[crc_offset] ^= 0xFF;
    fs::write(&kn, &bytes).unwrap();

    let out = unique("checksum-out");
    assert!(
        knot::untie(&kn, Some(&out)).is_err(),
        "a tampered checksum must be caught"
    );

    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&kn);
    let _ = fs::remove_file(&out);
}
