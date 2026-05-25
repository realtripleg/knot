# knot

Loose shoelaces are long and messy. Tie them in a knot and they're contained,
smaller, and your shoes don't fall out. `knot` does the same for files: it ties a
file into a compact `.knot` archive and unties it back, checking along the way
that nothing came loose.

It's a from-scratch DEFLATE-style compressor: LZ77 plus Huffman coding, built by
hand. On plain text and source code it lands within a few percent of `gzip -9`:

| input         | original |   knot | gzip -9 |
|---------------|---------:|-------:|--------:|
| Rust source   |   46,877 | 14,291 |  13,751 |
| /etc/services |  299,903 | 72,324 |  70,384 |

## Install

### Prebuilt binaries

Download the build for your platform from the
[Releases](https://github.com/realtripleg/knot/releases) page:

| platform              | asset                       |
|-----------------------|-----------------------------|
| Linux (x86_64)        | `knot-linux-x86_64.tar.gz`  |
| macOS (Apple Silicon) | `knot-macos-aarch64.tar.gz` |
| macOS (Intel)         | `knot-macos-x86_64.tar.gz`  |
| Windows (x86_64)      | `knot-windows-x86_64.zip`   |

**Linux and macOS**

```sh
tar xzf knot-*.tar.gz
sudo install -m755 knot /usr/local/bin/knot
# macOS only: clear the "unidentified developer" quarantine flag
xattr -d com.apple.quarantine /usr/local/bin/knot 2>/dev/null || true
```

**Windows**

Extract `knot.exe` from the zip, then run it from that folder or move it into a
directory on your `Path`. It's a self-contained executable; there's nothing else
to install.

### Build from source

Needs a stable Rust toolchain (install `rustup`, then `rustup default stable`):

```sh
git clone https://github.com/realtripleg/knot
cd knot
cargo build --release
```

The binary is `target/release/knot` (`target\release\knot.exe` on Windows). On
Linux and macOS, drop it on your `PATH`:

```sh
install -Dm755 target/release/knot ~/.local/bin/knot
```

## Usage

Three verbs: `tie` (compress), `untie` (decompress), `inspect` (show metadata).

### tie

```sh
$ knot tie report.txt
tied report.txt -> report.txt.knot (46877 -> 14291 bytes, 30.5%)
```

Writes `report.txt.knot` next to the original. Files that can't shrink (already
compressed archives, images, ...) are stored verbatim, so a `.knot` is never
bigger than it needs to be.

### untie

```sh
$ knot untie report.txt.knot
untied report.txt.knot -> report.txt (46877 bytes, checksum OK)
```

Restores the original bytes and verifies the CRC32 checksum before writing. Use
`-o`/`--output` to restore somewhere else:

```sh
$ knot untie report.txt.knot -o restored.txt
```

### inspect

```sh
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

A `.knot` file is a fixed header followed by the payload. All integers are
little-endian.

| offset | size | field                                           |
|-------:|-----:|-------------------------------------------------|
|      0 |    4 | magic bytes `KNOT`                              |
|      4 |    1 | format version (currently 1)                    |
|      5 |    1 | flags (bit 0 = stored / uncompressed)           |
|      6 |    2 | filename length `N`                             |
|      8 |    N | original filename (UTF-8, basename only)        |
|    8+N |    8 | original size in bytes                          |
|   16+N |    4 | CRC32 of the original bytes                     |
|   20+N |  ... | payload (compressed, or raw if the stored flag) |

The compressed payload is a single bitstream: two Huffman code-length tables (286
literal/length codes and 30 distance codes, 4 bits each), then the encoded token
stream, ending with an end-of-block marker.

## How it works

Two classic stages, the same pair `zip`/`gzip` use:

1. **LZ77** slides a 32 KB window over the input and replaces repeated byte
   sequences with `(length, distance)` back-references: "copy 12 bytes from 400
   bytes back." Anything not part of a repeat is emitted as a literal byte. This
   removes redundancy.

2. **Huffman coding** then gives common symbols short bit patterns and rare ones
   longer patterns. `knot` builds two canonical Huffman tables (one for literals
   and match lengths, one for distances) and stores only the code lengths, since
   both sides can rebuild identical codes from those alone.

Untying reverses both: rebuild the Huffman tables, decode the tokens, replay the
back-references, then confirm the size and checksum match what was stored.
