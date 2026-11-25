use std::{ffi::OsString, fs::OpenOptions, io::Write, path::Path};

use anyhow::{Result, bail};

/// A version receipt for GUI-subsystem binaries, without opening a tray or desktop.
pub fn report(args: &[OsString], revision: Option<&str>, version: Option<&str>) -> Result<bool> {
    if args.first().is_none_or(|arg| arg != "--release-identity") {
        return Ok(false);
    }
    let (Some(revision), Some(version), [_, receipt]) = (revision, version, args) else {
        bail!("release identity is unavailable or the receipt path is missing");
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(receipt))?;
    writeln!(file, "{revision}\n{version}")?;
    Ok(true)
}
