//! LZ77 sliding-window compression: the "dictionary" stage.
//!
//! We scan the input left to right. At each position we look backwards (up to
//! `WINDOW_SIZE` bytes) for the longest run of bytes that repeats what's coming
//! up. A repeat becomes a `Match { length, distance }` back-reference; anything
//! else is emitted as a `Literal`. Decoding just replays those instructions.
//!
//! To find matches fast we keep a hash table keyed on 3-byte sequences. Each
//! bucket is the head of a chain of earlier positions that share that hash,
//! linked through `prev`. We walk the chain (bounded by `MAX_CHAIN_LEN`) and
//! keep the longest match.

use crate::error::{KnotError, Result};

/// Largest distance we can point back — the sliding window. Fits in a u16.
pub const WINDOW_SIZE: usize = 32_768;
/// Shortest run worth encoding as a match (below this, literals are cheaper).
pub const MIN_MATCH: usize = 3;
/// Longest single match we encode.
pub const MAX_MATCH: usize = 258;

const HASH_BITS: u32 = 15;
const HASH_SIZE: usize = 1 << HASH_BITS; // 32768 buckets
const MAX_CHAIN_LEN: usize = 256; // probe limit per position (speed vs. ratio)

/// One LZ77 instruction: a raw byte, or "copy `length` bytes from `distance`
/// bytes back."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Literal(u8),
    Match { length: u16, distance: u16 },
}

/// Hash the 3 bytes at `b[0..3]` into a bucket index.
fn hash3(b: &[u8]) -> usize {
    let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | (b[2] as u32);
    // Knuth multiplicative hash, then keep the top HASH_BITS bits.
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// Length of the common prefix of `data[a..]` and `data[b..]`, capped at `max`.
fn match_len(data: &[u8], a: usize, b: usize, max: usize) -> usize {
    let mut len = 0;
    while len < max && data[a + len] == data[b + len] {
        len += 1;
    }
    len
}

/// Compress `data` into a stream of tokens (greedy matching).
pub fn encode(data: &[u8]) -> Vec<Token> {
    let n = data.len();
    let mut tokens = Vec::new();
    if n == 0 {
        return tokens;
    }

    // head[h] = most recent position whose 3-byte hash is h, or -1.
    // prev[p] = the position before p that shared p's hash, or -1.
    let mut head = vec![-1i32; HASH_SIZE];
    let mut prev = vec![-1i32; n];

    let mut i = 0;
    while i < n {
        // Need at least MIN_MATCH bytes ahead to hash / match.
        if i + MIN_MATCH > n {
            tokens.push(Token::Literal(data[i]));
            i += 1;
            continue;
        }

        let h = hash3(&data[i..i + 3]);
        let max_len = MAX_MATCH.min(n - i);
        let min_pos = i.saturating_sub(WINDOW_SIZE);

        let mut best_len = 0;
        let mut best_pos = 0;
        let mut candidate = head[h];
        let mut chain = MAX_CHAIN_LEN;

        while candidate >= 0 && (candidate as usize) >= min_pos && chain > 0 {
            let c = candidate as usize;
            // Cheap reject: a longer match must at least match at offset
            // best_len. Only then do the full compare.
            if data[c + best_len] == data[i + best_len] {
                let len = match_len(data, c, i, max_len);
                if len > best_len {
                    best_len = len;
                    best_pos = c;
                    if len >= max_len {
                        break; // can't do better
                    }
                }
            }
            candidate = prev[c];
            chain -= 1;
        }

        if best_len >= MIN_MATCH {
            tokens.push(Token::Match {
                length: best_len as u16,
                distance: (i - best_pos) as u16,
            });
            // Register every position the match covers so future searches can
            // find them, then skip past the whole match.
            let end = i + best_len;
            while i < end {
                if i + MIN_MATCH <= n {
                    let hh = hash3(&data[i..i + 3]);
                    prev[i] = head[hh];
                    head[hh] = i as i32;
                }
                i += 1;
            }
        } else {
            // No useful match: emit a literal, but still register this position.
            prev[i] = head[h];
            head[h] = i as i32;
            tokens.push(Token::Literal(data[i]));
            i += 1;
        }
    }

    tokens
}

/// Replay a token stream back into the original bytes.
pub fn decode(tokens: &[Token]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for &token in tokens {
        match token {
            Token::Literal(b) => out.push(b),
            Token::Match { length, distance } => {
                let dist = distance as usize;
                if dist == 0 || dist > out.len() {
                    return Err(KnotError::Corrupt(
                        "back-reference points outside the window",
                    ));
                }
                let start = out.len() - dist;
                for k in 0..length as usize {
                    // Copy the byte *out by value* before pushing: we can't hold
                    // a reference into `out` while we also push to it. Reading
                    // bytes we just wrote is intentional — that's how a short
                    // distance expands a run (e.g. dist=1 repeats one byte).
                    let byte = out[start + k];
                    out.push(byte);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) {
        let tokens = encode(data);
        let back = decode(&tokens).unwrap();
        assert_eq!(back, data, "round-trip mismatch");
    }

    #[test]
    fn empty() {
        roundtrip(b"");
    }

    #[test]
    fn single_byte() {
        roundtrip(b"x");
    }

    #[test]
    fn no_repeats() {
        roundtrip(b"abcdefghijklmnop");
    }

    #[test]
    fn simple_repeats() {
        roundtrip(b"abcabcabcabcabcabc");
    }

    #[test]
    fn long_run_of_one_byte() {
        // Exercises overlapping copies (distance 1) past MAX_MATCH.
        roundtrip(&[b'a'; 1000]);
    }

    #[test]
    fn english_like() {
        let mut v = Vec::new();
        for _ in 0..200 {
            v.extend_from_slice(b"the quick brown fox jumps over the lazy dog ");
        }
        roundtrip(&v);
    }

    #[test]
    fn produces_matches_on_repetition() {
        let tokens = encode(b"abcabcabcabcabc");
        assert!(tokens.iter().any(|t| matches!(t, Token::Match { .. })));
    }

    #[test]
    fn pseudo_random_bytes() {
        // A cheap LCG so the test is deterministic but non-repetitive.
        let mut v = Vec::new();
        let mut x = 0x1234_5678u32;
        for _ in 0..8000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            v.push((x >> 16) as u8);
        }
        roundtrip(&v);
    }

    #[test]
    fn rejects_bad_backreference() {
        // distance points before the start of output.
        let bad = [Token::Match { length: 3, distance: 5 }];
        assert!(decode(&bad).is_err());
    }
}
