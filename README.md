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

## Install (Arch Linux)

Build from source with a stable Rust toolchain:

```sh
git clone https://github.com/realtripleg/knot
cd knot
cargo build --release
```

The binary is `target/release/knot`. Put it on your `PATH`, for example:

```sh
install -Dm755 target/release/knot ~/.local/bin/knot
```

If you don't have Rust yet, install the `rustup` package and run
`rustup default stable`.

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

## License

Not yet decided.
