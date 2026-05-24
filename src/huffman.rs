//! Canonical Huffman coding.
//!
//! Given how often each symbol occurs we build an optimal set of bit-lengths
//! (shorter codes for common symbols), capped at `MAX_CODE_BITS` so codes always
//! fit in a u16 and the decoder stays bounded. From the *lengths alone* both
//! sides deterministically rebuild identical codes — that's what "canonical"
//! means — so we only ever store the lengths, never the codes themselves.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::bitio::BitReader;
use crate::error::{KnotError, Result};

/// Maximum code length; codes therefore fit in 15 bits.
pub const MAX_CODE_BITS: u8 = 15;

/// Compute canonical code lengths for `freqs`. Symbols with frequency 0 get
/// length 0 ("not present"). The returned vector is the same length as `freqs`.
pub fn code_lengths(freqs: &[u32]) -> Vec<u8> {
    let n = freqs.len();
    let mut lengths = vec![0u8; n];

    let used: Vec<usize> = (0..n).filter(|&i| freqs[i] > 0).collect();
    match used.len() {
        0 => return lengths,
        // A lone symbol still needs at least one bit so it can be written.
        1 => {
            lengths[used[0]] = 1;
            return lengths;
        }
        _ => {}
    }

    // Tree nodes live in an arena (a flat Vec); children are referred to by
    // index. This avoids `Box`/`Rc<RefCell>` and the borrow-checker pain of a
    // pointer-based tree — we just hand around `usize` indices.
    struct Node {
        left: usize,
        right: usize,
        symbol: Option<usize>,
    }
    let mut nodes: Vec<Node> = Vec::with_capacity(used.len() * 2);
    // Min-heap keyed on (weight, arena index). The index tie-breaker keeps the
    // tree deterministic when weights are equal.
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();

    for &sym in &used {
        let idx = nodes.len();
        nodes.push(Node {
            left: 0,
            right: 0,
            symbol: Some(sym),
        });
        heap.push(Reverse((freqs[sym] as u64, idx)));
    }
    while heap.len() > 1 {
        let Reverse((w1, a)) = heap.pop().unwrap();
        let Reverse((w2, b)) = heap.pop().unwrap();
        let idx = nodes.len();
        nodes.push(Node {
            left: a,
            right: b,
            symbol: None,
        });
        heap.push(Reverse((w1 + w2, idx)));
    }
    let Reverse((_, root)) = heap.pop().unwrap();

    // A leaf's depth is its code length. Iterative DFS (no recursion). Depths
    // can exceed 255 for a degenerate tree, so accumulate in u16 before the
    // length-limiting step below clamps everything to MAX_CODE_BITS.
    let mut raw = vec![0u16; n];
    let mut stack = vec![(root, 0u16)];
    while let Some((idx, depth)) = stack.pop() {
        match nodes[idx].symbol {
            Some(sym) => raw[sym] = depth,
            None => {
                stack.push((nodes[idx].left, depth + 1));
                stack.push((nodes[idx].right, depth + 1));
            }
        }
    }

    limit_lengths(&mut raw, freqs, MAX_CODE_BITS);

    for (out, &r) in lengths.iter_mut().zip(raw.iter()) {
        *out = r as u8; // safe: limiting guarantees r <= MAX_CODE_BITS
    }
    lengths
}

/// Clamp code lengths to `limit` bits while keeping a valid (complete) prefix
/// code, then hand the shortest codes to the most frequent symbols. This is the
/// standard count-redistribution: fold the overflow down, then restore the
/// Kraft equality (sum of 2^-len == 1) by splitting shorter codes.
fn limit_lengths(lengths: &mut [u16], freqs: &[u32], limit: u8) {
    let limit = limit as usize;
    let n = lengths.len();
    let max = lengths.iter().copied().max().unwrap_or(0) as usize;
    if max <= limit {
        return;
    }

    let mut count = vec![0u32; max + 1];
    for &l in lengths.iter() {
        if l > 0 {
            count[l as usize] += 1;
        }
    }
    for len in (limit + 1..=max).rev() {
        count[limit] += count[len];
        count[len] = 0;
    }

    let max_kraft = 1u64 << limit;
    let mut kraft: u64 = (1..=limit).map(|len| (count[len] as u64) << (limit - len)).sum();
    while kraft > max_kraft {
        count[limit] -= 1;
        let mut j = limit - 1;
        while j > 0 && count[j] == 0 {
            j -= 1;
        }
        if j == 0 {
            break; // shouldn't happen for real alphabets; stay safe.
        }
        count[j] -= 1;
        count[j + 1] += 2;
        kraft -= 1;
    }

    // Reassign: most frequent symbols get the shortest available codes.
    let mut used: Vec<usize> = (0..n).filter(|&i| freqs[i] > 0).collect();
    used.sort_by(|&a, &b| freqs[b].cmp(&freqs[a]).then(a.cmp(&b)));

    for l in lengths.iter_mut() {
        *l = 0;
    }
    let mut syms = used.into_iter();
    for (len, &cnt) in count.iter().enumerate().take(limit + 1).skip(1) {
        for _ in 0..cnt {
            if let Some(sym) = syms.next() {
                lengths[sym] = len as u16;
            }
        }
    }
}

/// Build canonical codes from code lengths: returns `(code, len)` per symbol
/// (a length of 0 means the symbol is unused). Codes are emitted high-bit-first.
pub fn canonical_codes(lengths: &[u8]) -> Vec<(u16, u8)> {
    let n = lengths.len();
    let mut codes = vec![(0u16, 0u8); n];
    let max = lengths.iter().copied().max().unwrap_or(0) as usize;
    if max == 0 {
        return codes;
    }

    let mut count = vec![0u32; max + 1];
    for &l in lengths {
        if l > 0 {
            count[l as usize] += 1;
        }
    }
    // First code of each length (RFC 1951 recurrence).
    let mut next = vec![0u32; max + 1];
    let mut code = 0u32;
    for bits in 1..=max {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    for (sym, &len_byte) in lengths.iter().enumerate() {
        let len = len_byte as usize;
        if len != 0 {
            codes[sym] = (next[len] as u16, len_byte);
            next[len] += 1;
        }
    }
    codes
}

/// A decoder rebuilt from code lengths alone. Reads one symbol at a time by
/// shifting in bits until the accumulated value lands in some length's code
/// range.
pub struct Decoder {
    max_len: u8,
    count: Vec<u32>,       // count[len] = number of codes of that length
    first_code: Vec<u32>,  // first canonical code of each length
    first_index: Vec<u32>, // where length `len`'s symbols start in `symbols`
    symbols: Vec<u16>,     // symbols ordered by (length, symbol)
}

impl Decoder {
    pub fn from_lengths(lengths: &[u8]) -> Result<Decoder> {
        let max = lengths.iter().copied().max().unwrap_or(0) as usize;
        if max == 0 {
            return Ok(Decoder {
                max_len: 0,
                count: vec![0; 1],
                first_code: vec![0; 1],
                first_index: vec![0; 1],
                symbols: Vec::new(),
            });
        }
        if max > MAX_CODE_BITS as usize {
            return Err(KnotError::Corrupt("Huffman code length exceeds maximum"));
        }

        let mut count = vec![0u32; max + 1];
        for &l in lengths {
            if l > 0 {
                count[l as usize] += 1;
            }
        }

        // Symbols sorted by (length, symbol) — the same order codes are assigned.
        let mut first_index = vec![0u32; max + 1];
        let mut idx = 0u32;
        for len in 1..=max {
            first_index[len] = idx;
            idx += count[len];
        }
        let mut symbols = vec![0u16; idx as usize];
        let mut cursor = first_index.clone();
        for (sym, &l) in lengths.iter().enumerate() {
            if l > 0 {
                let len = l as usize;
                symbols[cursor[len] as usize] = sym as u16;
                cursor[len] += 1;
            }
        }

        let mut first_code = vec![0u32; max + 1];
        let mut code = 0u32;
        for bits in 1..=max {
            code = (code + count[bits - 1]) << 1;
            first_code[bits] = code;
        }

        Ok(Decoder {
            max_len: max as u8,
            count,
            first_code,
            first_index,
            symbols,
        })
    }

    /// Read one symbol from `reader`, returning its index in the alphabet.
    pub fn read_symbol(&self, reader: &mut BitReader) -> Result<usize> {
        let mut code = 0u32;
        for len in 1..=self.max_len as usize {
            code = (code << 1) | reader.read_bits(1)?;
            let cnt = self.count[len];
            if cnt > 0 {
                let first = self.first_code[len];
                if code >= first && code - first < cnt {
                    let index = self.first_index[len] + (code - first);
                    return Ok(self.symbols[index as usize] as usize);
                }
            }
        }
        Err(KnotError::Corrupt("invalid Huffman code in stream"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitio::BitWriter;

    const EOB: usize = 256;

    /// Encode bytes with a per-input Huffman table, then decode and compare.
    fn round_trip(data: &[u8]) {
        let mut freqs = vec![0u32; 257];
        for &b in data {
            freqs[b as usize] += 1;
        }
        freqs[EOB] = 1; // always need an end marker

        let lengths = code_lengths(&freqs);
        assert!(lengths.iter().all(|&l| l <= MAX_CODE_BITS));
        let codes = canonical_codes(&lengths);

        let mut w = BitWriter::new();
        for &b in data {
            let (code, len) = codes[b as usize];
            w.write_bits(code as u32, len);
        }
        let (code, len) = codes[EOB];
        w.write_bits(code as u32, len);
        let bytes = w.finish();

        let decoder = Decoder::from_lengths(&lengths).unwrap();
        let mut r = BitReader::new(&bytes);
        let mut out = Vec::new();
        loop {
            let sym = decoder.read_symbol(&mut r).unwrap();
            if sym == EOB {
                break;
            }
            out.push(sym as u8);
        }
        assert_eq!(out, data, "huffman round-trip mismatch");
    }

    #[test]
    fn single_symbol_gets_one_bit() {
        let mut freqs = vec![0u32; 4];
        freqs[2] = 9;
        let lengths = code_lengths(&freqs);
        assert_eq!(lengths[2], 1);
        assert!(lengths.iter().enumerate().all(|(i, &l)| i == 2 || l == 0));
    }

    #[test]
    fn empty_input() {
        round_trip(b"");
    }

    #[test]
    fn single_byte() {
        round_trip(b"Z");
    }

    #[test]
    fn skewed_text() {
        round_trip(b"aaaaaaaaaabbbbbccd");
    }

    #[test]
    fn full_byte_range() {
        let data: Vec<u8> = (0..=255).collect();
        round_trip(&data);
    }

    #[test]
    fn pseudo_random() {
        let mut v = Vec::new();
        let mut x = 0xC0FF_EE00u32;
        for _ in 0..6000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            v.push((x >> 16) as u8);
        }
        round_trip(&v);
    }

    #[test]
    fn limits_degenerate_tree() {
        // Fibonacci frequencies force a maximally deep tree (~39 levels); the
        // limiter must pull every code down to <= MAX_CODE_BITS and still leave
        // a valid prefix code that decodes each symbol back to itself.
        let mut freqs = vec![0u32; 40];
        let (mut a, mut b) = (1u32, 1u32);
        for f in freqs.iter_mut() {
            *f = a;
            let c = a + b;
            a = b;
            b = c;
        }
        let lengths = code_lengths(&freqs);
        assert!(lengths.iter().all(|&l| l <= MAX_CODE_BITS && l > 0));

        let codes = canonical_codes(&lengths);
        let decoder = Decoder::from_lengths(&lengths).unwrap();
        for (sym, &(code, len)) in codes.iter().enumerate() {
            let mut w = BitWriter::new();
            w.write_bits(code as u32, len);
            let bytes = w.finish();
            let mut r = BitReader::new(&bytes);
            assert_eq!(decoder.read_symbol(&mut r).unwrap(), sym);
        }
    }
}
