# knot

Tie a file into a compact `.knot` archive, and untie it back. A from-scratch
DEFLATE-style compressor (LZ77 + Huffman), written in Rust. On plain text and
source code it lands within a few percent of `gzip -9` and `zip`.

> Loose shoelaces are long and messy. Tie them in a knot and they're contained,
> smaller, and your shoes don't fall out. Same idea for files: `knot` cares about
> size *and* integrity, so every archive carries a CRC32 checksum.

## Install

Download a prebuilt binary from the
[latest release](https://github.com/realtripleg/knot/releases/latest):

| platform              | asset                       |
|-----------------------|-----------------------------|
| Linux (x86_64)        | `knot-linux-x86_64.tar.gz`  |
| macOS (Apple Silicon) | `knot-macos-aarch64.tar.gz` |
| macOS (Intel)         | `knot-macos-x86_64.tar.gz`  |
| Windows (x86_64)      | `knot-windows-x86_64.zip`   |

```sh
# Linux / macOS
tar xzf knot-*.tar.gz
sudo install -m755 knot /usr/local/bin/knot
# macOS only: clear the "unidentified developer" quarantine flag
xattr -d com.apple.quarantine /usr/local/bin/knot 2>/dev/null || true
```

On Windows, extract `knot.exe` and put it on your `Path`. The binary is
self-contained, so there's nothing else to install.

To build from source instead, with a stable Rust toolchain: `cargo build --release`.

## Commands

### tie — compress

```
$ knot tie report.txt
tied report.txt -> report.txt.knot (46877 -> 14291 bytes, 30.5%)
```

Writes `<file>.knot` next to the original. Inputs that can't shrink (already
compressed archives, images) are stored verbatim, so the archive never grows.

### untie — decompress

```
$ knot untie report.txt.knot
untied report.txt.knot -> report.txt (46877 bytes, checksum OK)
```

Restores the original bytes and verifies the CRC32 checksum before writing. Pass
`-o`/`--output` to restore somewhere else.

### inspect — metadata

```
$ knot inspect report.txt.knot
report.txt.knot
  format:          KNOT v1
  mode:            compressed
  original name:   report.txt
  original size:   46877 bytes
  on-disk size:    14291 bytes (header + 14260 payload)
  ratio:           30.5% of original
  crc32:           0xc03cba1e
```

## File format

A `.knot` file is a fixed header followed by the payload. Integers are little-endian.

| offset | size | field                                           |
|-------:|-----:|-------------------------------------------------|
|      0 |    4 | magic bytes `KNOT`                              |
|      4 |    1 | format version (currently 1)                    |
|      5 |    1 | flags (bit 0 = stored / uncompressed)           |
|      6 |    2 | filename length `N`                             |
|      8 |    N | original filename (UTF-8, basename only)        |
|    8+N |    8 | original size in bytes                          |
|   16+N |    4 | CRC32 of the original bytes                     |
|   20+N |  ... | payload (compressed, or raw if stored)          |

The compressed payload is a single bitstream: two Huffman code-length tables (286
literal/length codes and 30 distance codes, 4 bits each), then the encoded token
stream, ending with an end-of-block marker.

## How it works

Two classic stages, the same pair `zip` and `gzip` use:

1. **LZ77** slides a 32 KB window over the input and replaces repeated byte
   sequences with `(length, distance)` back-references ("copy 12 bytes from 400
   bytes back"). Anything not part of a repeat is emitted as a literal byte.

2. **Huffman coding** gives common symbols short bit patterns and rare ones longer
   patterns. `knot` builds two canonical Huffman tables (one for literals and match
   lengths, one for distances) and stores only the code lengths, since both sides
   rebuild identical codes from those alone.

Untying reverses both, then confirms the size and checksum match what was stored.

---

[Source on GitHub](https://github.com/realtripleg/knot)
