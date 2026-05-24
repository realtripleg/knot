//! The compressed-payload codec: LZ77 tokens entropy-coded with two canonical
//! Huffman tables (DEFLATE-style), plus the length/distance "extra bits".
//!
//! Payload layout — a single MSB-first bitstream:
//!   - 286 literal/length code lengths, 4 bits each
//!   -  30 distance      code lengths, 4 bits each
//!   - the token stream: each literal, or a length code (+extra) followed by a
//!     distance code (+extra)
//!   - the end-of-block symbol (256)
//!
//! Because the code lengths are stored, the decoder rebuilds the exact same
//! canonical Huffman tables (see `huffman.rs`) and replays the tokens.

use crate::bitio::{BitReader, BitWriter};
use crate::error::{KnotError, Result};
use crate::huffman::{self, Decoder};
use crate::lz77::{self, Token};
use crate::tables;

const LITLEN_SYMBOLS: usize = 286; // 0..=255 literals, 256 = EOB, 257..=285 lengths
const DIST_SYMBOLS: usize = 30;
const EOB: usize = 256;
const CODE_LENGTH_BITS: u8 = 4; // each stored code length is 0..=15

/// Compress `data` into a payload bitstream.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let tokens = lz77::encode(data);

    // Count how often each alphabet symbol occurs so Huffman can size codes.
    let mut litlen_freq = vec![0u32; LITLEN_SYMBOLS];
    let mut dist_freq = vec![0u32; DIST_SYMBOLS];
    for &token in &tokens {
        match token {
            Token::Literal(b) => litlen_freq[b as usize] += 1,
            Token::Match { length, distance } => {
                let (sym, _, _) = tables::length_code(length);
                litlen_freq[sym] += 1;
                let (dsym, _, _) = tables::distance_code(distance);
                dist_freq[dsym] += 1;
            }
        }
    }
    litlen_freq[EOB] += 1; // the stream always ends with one EOB

    let litlen_lengths = huffman::code_lengths(&litlen_freq);
    let dist_lengths = huffman::code_lengths(&dist_freq);
    let litlen_codes = huffman::canonical_codes(&litlen_lengths);
    let dist_codes = huffman::canonical_codes(&dist_lengths);

    let mut w = BitWriter::new();

    // Header: the two code-length tables, so the decoder can rebuild the codes.
    for &len in &litlen_lengths {
        w.write_bits(len as u32, CODE_LENGTH_BITS);
    }
    for &len in &dist_lengths {
        w.write_bits(len as u32, CODE_LENGTH_BITS);
    }

    // Body: the tokens.
    for &token in &tokens {
        match token {
            Token::Literal(b) => {
                let (code, len) = litlen_codes[b as usize];
                w.write_bits(code as u32, len);
            }
            Token::Match { length, distance } => {
                let (sym, lbits, lextra) = tables::length_code(length);
                let (code, len) = litlen_codes[sym];
                w.write_bits(code as u32, len);
                if lbits > 0 {
                    w.write_bits(lextra as u32, lbits);
                }

                let (dsym, dbits, dextra) = tables::distance_code(distance);
                let (dcode, dlen) = dist_codes[dsym];
                w.write_bits(dcode as u32, dlen);
                if dbits > 0 {
                    w.write_bits(dextra as u32, dbits);
                }
            }
        }
    }

    // Terminator.
    let (code, len) = litlen_codes[EOB];
    w.write_bits(code as u32, len);

    w.finish()
}

/// Decompress a payload bitstream back into the original bytes.
pub fn decompress(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = BitReader::new(payload);

    let mut litlen_lengths = vec![0u8; LITLEN_SYMBOLS];
    for len in litlen_lengths.iter_mut() {
        *len = r.read_bits(CODE_LENGTH_BITS)? as u8;
    }
    let mut dist_lengths = vec![0u8; DIST_SYMBOLS];
    for len in dist_lengths.iter_mut() {
        *len = r.read_bits(CODE_LENGTH_BITS)? as u8;
    }

    let litlen_dec = Decoder::from_lengths(&litlen_lengths)?;
    let dist_dec = Decoder::from_lengths(&dist_lengths)?;

    // Rebuild the token stream, then hand it to the (already tested) LZ77
    // decoder to expand the back-references.
    let mut tokens = Vec::new();
    loop {
        let sym = litlen_dec.read_symbol(&mut r)?;
        if sym == EOB {
            break;
        }
        if sym < 256 {
            tokens.push(Token::Literal(sym as u8));
            continue;
        }

        let idx = sym - 257;
        if idx >= tables::LENGTH_BASE.len() {
            return Err(KnotError::Corrupt("invalid length symbol"));
        }
        let lbits = tables::LENGTH_EXTRA[idx];
        let lextra = if lbits > 0 { r.read_bits(lbits)? as u16 } else { 0 };
        let length = tables::LENGTH_BASE[idx] + lextra;

        let dsym = dist_dec.read_symbol(&mut r)?;
        if dsym >= tables::DIST_BASE.len() {
            return Err(KnotError::Corrupt("invalid distance symbol"));
        }
        let dbits = tables::DIST_EXTRA[dsym];
        let dextra = if dbits > 0 { r.read_bits(dbits)? as u16 } else { 0 };
        let distance = tables::DIST_BASE[dsym] + dextra;

        tokens.push(Token::Match { length, distance });
    }

    lz77::decode(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8]) {
        let packed = compress(data);
        let back = decompress(&packed).unwrap();
        assert_eq!(back, data, "codec round-trip mismatch");
    }

    #[test]
    fn empty() {
        round_trip(b"");
    }

    #[test]
    fn single_byte() {
        round_trip(b"!");
    }

    #[test]
    fn english_text() {
        round_trip(b"the quick brown fox jumps over the lazy dog, the quick brown fox");
    }

    #[test]
    fn highly_repetitive() {
        round_trip(&vec![b'z'; 5000]);
    }

    #[test]
    fn source_like() {
        let snippet = r#"
            fn main() {
                let mut total = 0u64;
                for i in 0..100 { total += i; }
                println!("total = {}", total);
            }
        "#
        .repeat(60);
        round_trip(snippet.as_bytes());
    }

    #[test]
    fn pseudo_random_bytes() {
        let mut v = Vec::new();
        let mut x = 0x9E37_79B9u32;
        for _ in 0..10_000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            v.push((x >> 24) as u8);
        }
        round_trip(&v);
    }

    #[test]
    fn actually_compresses_repetitive_input() {
        let data = vec![b'a'; 10_000];
        let packed = compress(&data);
        assert!(
            packed.len() < data.len(),
            "expected compression: {} >= {}",
            packed.len(),
            data.len()
        );
    }

    #[test]
    fn truncated_payload_errors() {
        // Too short to even hold the code-length tables.
        assert!(decompress(b"\x00\x00\x00").is_err());
    }
}
