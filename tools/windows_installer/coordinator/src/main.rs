#![deny(clippy::undocumented_unsafe_blocks)]

use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(windows)]
mod fs_guard;
#[cfg(windows)]
mod metadata;
#[cfg(windows)]
mod transaction;
#[cfg(windows)]
mod webview;

const MANIFEST: &str = "payload.json";
const MAX_MANIFEST: u64 = 64 * 1024;
const LEGACY_INVENTORY: &str = include_str!("legacy-0.1.0.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    revision: String,
    version: String,
    runtime: String,
    files: BTreeMap<String, Member>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct Member {
    bytes: u64,
    sha256: String,
}

fn plain(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 == 0
    }
    #[cfg(not(windows))]
    {
        !metadata.file_type().is_symlink()
    }
}

fn inspect(
    directory: &Path,
    prefix: &str,
    manifest: Option<&str>,
    expected: &mut BTreeMap<String, Member>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid payload name"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(plain(&metadata), "redirected payload entry");
        let relative = format!("{prefix}{name}");
        if Some(relative.as_str()) == manifest {
            ensure!(metadata.is_file(), "invalid payload manifest");
        } else if metadata.is_dir() {
            let child_prefix = format!("{relative}/");
            ensure!(
                expected.keys().any(|key| key.starts_with(&child_prefix)),
                "unexpected payload directory {relative}"
            );
            inspect(&path, &child_prefix, manifest, expected)?;
        } else {
            ensure!(metadata.is_file(), "unsupported payload entry {relative}");
            let member = expected
                .remove(&relative)
                .with_context(|| format!("unexpected payload member {relative}"))?;
            ensure!(
                metadata.len() == member.bytes,
                "payload length mismatch for {relative}"
            );
            let mut file = File::open(&path)?.take(member.bytes.saturating_add(1));
            let mut hasher = Sha256::new();
            let count = io::copy(&mut file, &mut hasher)?;
            ensure!(
                count == member.bytes && format!("{:x}", hasher.finalize()) == member.sha256,
                "payload hash mismatch for {relative}"
            );
        }
    }
    Ok(())
}

fn verify_root(root: &Path) -> Result<()> {
    for ancestor in root.ancestors() {
        if !ancestor.as_os_str().is_empty() {
            let metadata = fs::symlink_metadata(ancestor)?;
            ensure!(
                metadata.is_dir() && plain(&metadata),
                "redirected payload root"
            );
        }
    }
    Ok(())
}

fn verify_inventory(
    root: &Path,
    manifest: Option<&str>,
    mut expected: BTreeMap<String, Member>,
) -> Result<()> {
    inspect(root, "", manifest, &mut expected)?;
    ensure!(expected.is_empty(), "missing payload members");
    Ok(())
}

fn verify(root: &Path, expected_hash: &str) -> Result<Manifest> {
    verify_root(root)?;
    let path = root.join(MANIFEST);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && plain(&metadata) && metadata.len() <= MAX_MANIFEST,
        "invalid payload manifest"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected_hash,
        "payload manifest hash mismatch"
    );
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(manifest.schema == 1, "unsupported payload schema");
    for required in [
        "league-replay-app.exe",
        "recorder.exe",
        "resources/media-runtime/runtime-manifest.json",
    ] {
        ensure!(
            manifest.files.contains_key(required),
            "missing required payload member {required}"
        );
    }
    verify_inventory(root, Some(MANIFEST), manifest.files.clone())?;
    Ok(manifest)
}

fn run() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    #[cfg(windows)]
    if let [command, scratch] = args.as_slice()
        && command == "install-webview"
    {
        return webview::install(Path::new(scratch));
    }
    #[cfg(windows)]
    if let [command, scratch] = args.as_slice()
        && command == "clean-extraction"
    {
        return transaction::clean_extraction(Path::new(scratch));
    }
    #[cfg(windows)]
    if let Some(result) = transaction::dispatch(&args) {
        return result;
    }
    let [command, root] = args.as_slice() else {
        bail!("expected verify-payload or verify-legacy-payload and an absolute directory");
    };
    let root = PathBuf::from(root);
    ensure!(root.is_absolute(), "payload directory must be absolute");
    if command == "verify-legacy-payload" {
        verify_root(&root)?;
        verify_inventory(&root, None, serde_json::from_str(LEGACY_INVENTORY)?)?;
        println!("Verified historical NSIS 0.1.0 payload inventory");
        return Ok(());
    }
    ensure!(command == "verify-payload", "unsupported setup command");
    let digest = option_env!("CHRONOBREAK_PAYLOAD_SHA256")
        .context("no payload is bound to this coordinator")?;
    let manifest = verify(&root, digest)?;
    println!(
        "Verified {} {} {}",
        manifest.version, manifest.revision, manifest.runtime
    );
    Ok(())
}

fn main() -> Result<()> {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(root: &Path) -> String {
        let payload = b"isolated payload fixture";
        let mut files = serde_json::Map::new();
        for name in [
            "league-replay-app.exe",
            "recorder.exe",
            "resources/media-runtime/runtime-manifest.json",
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, payload).unwrap();
            files.insert(
                name.to_owned(),
                serde_json::json!({
                    "bytes": payload.len(),
                    "sha256": format!("{:x}", Sha256::digest(payload)),
                }),
            );
        }
        let manifest = serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "revision": "fixture",
            "version": "fixture",
            "runtime": "fixture",
            "files": files,
        }))
        .unwrap();
        fs::write(root.join(MANIFEST), &manifest).unwrap();
        format!("{:x}", Sha256::digest(&manifest))
    }

    #[test]
    fn verifies_exact_inventory_and_rejects_altered_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let digest = make_payload(root);
        assert!(verify(root, &digest).is_ok());
        assert!(verify(root, "foreign-manifest").is_err());
        fs::write(root.join("extra.txt"), b"extra").unwrap();
        assert!(verify(root, &digest).is_err());
        fs::remove_file(root.join("extra.txt")).unwrap();
        fs::write(root.join("recorder.exe"), b"ISOLATED PAYLOAD FIXTURE").unwrap();
        assert!(verify(root, &digest).is_err());
        fs::remove_file(root.join("recorder.exe")).unwrap();
        assert!(verify(root, &digest).is_err());
    }
}
