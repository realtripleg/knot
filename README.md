# knot

Tie a file into a compact `.knot` archive, and untie it back. A from-scratch
DEFLATE-style compressor (LZ77 + Huffman), written in Rust.

> Loose shoelaces are long and messy. Tie them in a knot and they're contained,
> smaller, and your shoes don't fall out. Same idea, for files.

On plain text and source code it lands within a few percent of `gzip -9` and `zip`:

| input         | original |   knot | gzip -9 |  zip -9 |
|---------------|---------:|-------:|--------:|--------:|
| Rust source   |   46,877 | 14,293 |  13,753 |  13,864 |
| /etc/services |  299,903 | 72,331 |  70,391 |  70,518 |

**Docs:** <https://realtripleg.github.io/knot/>

## Install

Prebuilt binaries for Linux, macOS (Intel + Apple Silicon), and Windows are on the
[releases page](https://github.com/realtripleg/knot/releases/latest).

```sh
# Linux / macOS
tar xzf knot-*.tar.gz
sudo install -m755 knot /usr/local/bin/knot
```

On Windows, extract `knot.exe` from the zip and put it on your `Path`.

### From source

Needs a stable Rust toolchain (`rustup default stable`):

```sh
git clone https://github.com/realtripleg/knot
cd knot
cargo build --release   # binary at target/release/knot
```

## Usage

```sh
knot tie     report.txt          # compress   -> report.txt.knot
knot inspect report.txt.knot     # show metadata
knot untie   report.txt.knot     # decompress (verifies the checksum)
```

`untie` takes `-o`/`--output` to restore elsewhere. Files that can't shrink are
stored verbatim, so a `.knot` is never bigger than the original.

The [docs](https://realtripleg.github.io/knot/) cover the file-format spec and how
it works.
