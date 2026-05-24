//! knot — a from-scratch compression format.
//!
//! Stage 0 is a passthrough: `tie` and `untie` just copy bytes so we can
//! exercise the CLI and file I/O before any real compression exists.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Tie `input` into `<input>.knot`.
pub fn tie(input: &Path) -> io::Result<()> {
    let data = fs::read(input)?;

    // Append ".knot" to the whole name, like gzip turns `notes.txt` into
    // `notes.txt.gz`. We work on the OS string so odd (non-UTF-8) names survive.
    let mut out_name = input.as_os_str().to_owned();
    out_name.push(".knot");
    let output = PathBuf::from(out_name);

    fs::write(&output, &data)?;
    println!(
        "tied {} -> {} ({} bytes, stage 0: stored as-is)",
        input.display(),
        output.display(),
        data.len()
    );
    Ok(())
}

/// Untie `input` back to its original name (or to `output` if given).
pub fn untie(input: &Path, output: Option<&Path>) -> io::Result<()> {
    let data = fs::read(input)?;

    let restored = match output {
        Some(path) => path.to_path_buf(),
        None => default_restored_name(input),
    };

    fs::write(&restored, &data)?;
    println!(
        "untied {} -> {} ({} bytes)",
        input.display(),
        restored.display(),
        data.len()
    );
    Ok(())
}

/// Show what we know about a `.knot` file. Stage 0 only knows its size on
/// disk; the real header fields arrive in stage 1.
pub fn inspect(input: &Path) -> io::Result<()> {
    let meta = fs::metadata(input)?;
    println!("{}", input.display());
    println!("  size:  {} bytes", meta.len());
    println!("  (stage 0: no .knot header to read yet)");
    Ok(())
}

/// Strip a trailing `.knot` to recover the original name. If there isn't one,
/// add `.untied` so we never silently overwrite the input in place.
fn default_restored_name(input: &Path) -> PathBuf {
    if input.extension().is_some_and(|ext| ext == "knot") {
        input.with_extension("")
    } else {
        let mut name = input.as_os_str().to_owned();
        name.push(".untied");
        PathBuf::from(name)
    }
}
