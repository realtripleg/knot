//! DEFLATE-style length and distance code tables (RFC 1951).
//!
//! A match length (3..=258) or distance (1..=32768) is split into a *code*
//! (which gets a Huffman symbol) plus a few *extra bits* that pick a value
//! within the code's range. This lets one Huffman symbol cover a span of
//! values, so we don't need 256 distinct length symbols or 32768 distance
//! symbols. Both `huffman.rs` and `codec.rs` read these so the encoder and
//! decoder can never disagree.

/// Base length for length codes 257..=285 (index 0 == code 257).
pub const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
/// Extra bits read after each length code.
pub const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Base distance for distance codes 0..=29.
pub const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
/// Extra bits read after each distance code.
pub const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Map a match length to its literal/length alphabet symbol (257..=285), the
/// number of extra bits, and the extra value to write after the code.
pub fn length_code(length: u16) -> (usize, u8, u16) {
    let mut idx = 0;
    while idx + 1 < LENGTH_BASE.len() && LENGTH_BASE[idx + 1] <= length {
        idx += 1;
    }
    (257 + idx, LENGTH_EXTRA[idx], length - LENGTH_BASE[idx])
}

/// Map a match distance to its distance alphabet symbol (0..=29), the number of
/// extra bits, and the extra value to write after the code.
pub fn distance_code(distance: u16) -> (usize, u8, u16) {
    let mut idx = 0;
    while idx + 1 < DIST_BASE.len() && DIST_BASE[idx + 1] <= distance {
        idx += 1;
    }
    (idx, DIST_EXTRA[idx], distance - DIST_BASE[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_boundaries() {
        assert_eq!(length_code(3), (257, 0, 0));
        assert_eq!(length_code(10), (264, 0, 0));
        assert_eq!(length_code(11), (265, 1, 0));
        assert_eq!(length_code(12), (265, 1, 1));
        assert_eq!(length_code(258), (285, 0, 0));
    }

    #[test]
    fn distance_boundaries() {
        assert_eq!(distance_code(1), (0, 0, 0));
        assert_eq!(distance_code(4), (3, 0, 0));
        assert_eq!(distance_code(5), (4, 1, 0));
        assert_eq!(distance_code(6), (4, 1, 1));
        assert_eq!(distance_code(32768), (29, 13, 8191));
    }

    /// Every length/distance must round-trip through code + extra back to itself.
    #[test]
    fn codes_reconstruct_values() {
        for length in 3u16..=258 {
            let (sym, _bits, extra) = length_code(length);
            assert_eq!(LENGTH_BASE[sym - 257] + extra, length);
        }
        for distance in 1u16..=32768 {
            let (sym, _bits, extra) = distance_code(distance);
            assert_eq!(DIST_BASE[sym] + extra, distance);
        }
    }
}
