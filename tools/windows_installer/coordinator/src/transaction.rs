use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use chronobreak_recorder_control::install_guard::Installation;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::fs_guard::image_names;
use crate::fs_guard::{Identity, Node, fixed_write, present, reject_running_images};
use crate::metadata::{Metadata, Snapshot};
use crate::{LEGACY_INVENTORY, MANIFEST, Member};

const STAGED: &str = "staged";
const PREVIOUS: &str = "previous";
const OWNERSHIP: &str = "installed.json";
const JOURNAL: &str = "transaction.json";
const BLOCK: &str = "launch.blocked";
const UNINSTALLER: &str = "uninstall.exe";
const MARKER: &[u8] = b"Chronobreak setup is recovering an installation.\n";
const MAX_STATE: u64 = 512 * 1024;
const RUNTIME_FILES: [&str; 9] = [
    "resources/media-runtime/runtime-manifest.json",
    "resources/media-runtime/bin/ffmpeg.exe",
    "resources/media-runtime/bin/ffprobe.exe",
    "resources/media-runtime/licenses/AMF_LICENSE.txt",
    "resources/media-runtime/licenses/COPYING.GPLv3.txt",
    "resources/media-runtime/licenses/LIBVPL_LICENSE.txt",
    "resources/media-runtime/licenses/NVCODEC_LICENSE.txt",
    "resources/media-runtime/licenses/SOURCE_AND_BUILD.md",
    "resources/media-runtime/licenses/THIRD_PARTY_NOTICES.md",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Tree {
    legacy: bool,
    root: Identity,
    directories: BTreeMap<String, Identity>,
    files: BTreeMap<String, (Identity, Member)>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    schema: u32,
    tree: Tree,
    metadata: Snapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Stage,
    MovePrevious,
    Publish,
    Metadata,
    Committed,
    Uninstall,
    UninstallMetadata,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    nonce: String,
    phase: Phase,
    old: Option<Tree>,
    prior: Snapshot,
    candidate: Option<Installed>,
}

#[derive(Clone, Copy)]
enum Inventory {
    Exact,
    InterruptedCleanup,
    PreserveUnknown,
}

struct LegacyBarrier(BTreeMap<String, Node>);

impl LegacyBarrier {
    fn hold(volume: &Node, path: &Path, receipt: Option<&Tree>) -> Result<Option<Self>> {
        let Some(receipt) = receipt.filter(|tree| tree.legacy) else {
            return Ok(None);
        };
        let mut pins = BTreeMap::new();
        let mut images = Vec::new();
        for (name, (id, member)) in receipt
            .files
            .iter()
            .filter(|(name, _)| name.ends_with(".exe"))
        {
            ensure!(
                Node::file_identity(&path.join(name))?.id == *id,
                "historical executable path changed"
            );
            let pin = Node::deny_execution_by_id(volume, *id)?;
            pin.matches(member)?;
            images.extend(image_names(&path.join(name), *id)?);
            pins.insert(name.clone(), pin);
        }
        reject_running_images(&images)?;
        Ok(Some(Self(pins)))
    }
}

struct OpenTree {
    root: Node,
    dirs: BTreeMap<String, Node>,
    files: BTreeMap<String, Node>,
}

impl OpenTree {
    fn open(
        path: &Path,
        expected: &BTreeMap<String, Member>,
        legacy: bool,
        inventory: Inventory,
        barrier: Option<&LegacyBarrier>,
    ) -> Result<Self> {
        let root = Node::directory(path, true)?;
        let mut dirs = BTreeMap::new();
        let mut files = BTreeMap::new();
        for name in expected.keys() {
            for (index, _) in name.match_indices('/') {
                let name = &name[..index];
                if !dirs.contains_key(name) && present(&path.join(name))? {
                    let node = Node::directory(&path.join(name), true)?;
                    ensure!(
                        node.id.volume == root.id.volume,
                        "redirected product directory"
                    );
                    dirs.insert(name.to_owned(), node);
                }
            }
            let leaf = path.join(name);
            if !present(&leaf)? && !matches!(inventory, Inventory::Exact) {
                continue;
            }
            let node = if let Some(executable) = barrier.and_then(|barrier| barrier.0.get(name)) {
                let node = Node::file_identity(&leaf)?;
                ensure!(
                    node.id == executable.id,
                    "historical executable identity changed"
                );
                executable.matches(&expected[name])?;
                node
            } else {
                let node = Node::file(&leaf, true, legacy && name.ends_with(".exe"))?;
                node.matches(&expected[name])
                    .with_context(|| format!("cannot verify {name}"))?;
                node
            };
            ensure!(node.id.volume == root.id.volume, "redirected product file");
            files.insert(name.to_owned(), node);
        }
        let tree = Self { root, dirs, files };
        if !matches!(inventory, Inventory::PreserveUnknown) {
            tree.check_names(path, "")?;
        }
        if legacy && barrier.is_none() {
            let mut images = Vec::new();
            for (name, file) in tree.files.iter().filter(|(name, _)| name.ends_with(".exe")) {
                images.extend(image_names(&path.join(name), file.id)?);
            }
            reject_running_images(&images)?;
        }
        Ok(tree)
    }

    fn check_names(&self, root: &Path, prefix: &str) -> Result<()> {
        for entry in fs::read_dir(root.join(prefix))? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("unlisted product entry"))?;
            let name = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if self.dirs.contains_key(&name) {
                self.check_names(root, &name)?;
            } else {
                ensure!(
                    self.files.contains_key(&name),
                    "unlisted content in the product folder. Move it to a safe location before upgrading."
                );
            }
        }
        Ok(())
    }

    fn receipt(&self, members: BTreeMap<String, Member>, legacy: bool) -> Tree {
        Tree {
            legacy,
            root: self.root.id,
            directories: self
                .dirs
                .iter()
                .map(|(name, node)| (name.clone(), node.id))
                .collect(),
            files: members
                .into_iter()
                .map(|(name, member)| (name.clone(), (self.files[&name].id, member)))
                .collect(),
        }
    }

    fn verify(path: &Path, receipt: &Tree, inventory: Inventory) -> Result<Self> {
        Self::verify_with_barrier(path, receipt, inventory, None)
    }

    fn verify_with_barrier(
        path: &Path,
        receipt: &Tree,
        inventory: Inventory,
        barrier: Option<&LegacyBarrier>,
    ) -> Result<Self> {
        receipt.validate()?;
        let members = receipt
            .files
            .iter()
            .map(|(name, (_, member))| (name.clone(), member.clone()))
            .collect();
        let tree = Self::open(path, &members, receipt.legacy, inventory, barrier)?;
        ensure!(
            tree.root.id == receipt.root,
            "product folder identity changed"
        );
        for (name, node) in &tree.dirs {
            ensure!(
                receipt.directories.get(name) == Some(&node.id),
                "product directory identity changed"
            );
        }
        for (name, node) in &tree.files {
            ensure!(
                receipt.files[name].0 == node.id,
                "product file identity changed"
            );
        }
        if matches!(inventory, Inventory::Exact) {
            ensure!(
                tree.dirs.len() == receipt.directories.len()
                    && tree.files.len() == receipt.files.len(),
                "product tree is incomplete"
            );
        }
        Ok(tree)
    }

    // NTFS refuses to rename a directory with open descendant path handles.
    // Legacy launch protection is held by file ID, independent of these handles.
    fn into_root(self) -> Node {
        self.root
    }

    fn cleanup(self, path: &Path, preserve_unknown: bool, keep_uninstaller: bool) -> Result<()> {
        for (name, file) in self.files {
            if keep_uninstaller && name == UNINSTALLER {
                continue;
            }
            file.delete()?;
            fault("delete-file")?;
        }
        for (name, directory) in self.dirs.into_iter().rev() {
            delete_empty(path.join(name).as_path(), directory, preserve_unknown)?;
        }
        if !keep_uninstaller {
            delete_empty(path, self.root, preserve_unknown)?;
        }
        Ok(())
    }
}

fn delete_empty(path: &Path, pin: Node, preserve_unknown: bool) -> Result<()> {
    if fs::read_dir(path)?.next().transpose()?.is_none() {
        pin.delete()?;
    } else {
        ensure!(
            preserve_unknown,
            "unrecognized files were retained during setup cleanup"
        );
    }
    Ok(())
}

impl Tree {
    fn validate(&self) -> Result<()> {
        validate_members(self.files.keys().map(String::as_str), self.legacy, true)?;
        if self.legacy {
            let known: BTreeMap<String, Member> = serde_json::from_str(LEGACY_INVENTORY)?;
            ensure!(
                self.files
                    .iter()
                    .all(|(name, (_, member))| known.get(name) == Some(member)),
                "historical setup receipt differs from official inventory"
            );
        }
        for (identity, member) in self.files.values() {
            ensure!(
                identity.volume == self.root.volume,
                "incorrect setup volume"
            );
            ensure!(
                member.bytes <= 4 * 1024 * 1024 * 1024 && is_digest(&member.sha256),
                "invalid setup file fingerprint"
            );
        }
        ensure!(
            self.directories
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == BTreeSet::from([
                    "resources",
                    "resources/media-runtime",
                    "resources/media-runtime/bin",
                    "resources/media-runtime/licenses"
                ]),
            "unexpected setup directory receipt"
        );
        ensure!(
            self.directories
                .values()
                .all(|id| id.volume == self.root.volume),
            "incorrect setup volume"
        );
        Ok(())
    }
}

fn validate_members<'a>(
    keys: impl Iterator<Item = &'a str>,
    legacy: bool,
    installed: bool,
) -> Result<()> {
    let mut expected = BTreeSet::from(RUNTIME_FILES);
    expected.insert("league-replay-app.exe");
    if !legacy {
        expected.insert("recorder.exe");
        expected.insert(MANIFEST);
    }
    if installed {
        expected.insert(UNINSTALLER);
    }
    ensure!(
        keys.collect::<BTreeSet<_>>() == expected,
        "unexpected product inventory"
    );
    Ok(())
}

fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn fingerprint(bytes: &[u8]) -> Member {
    Member {
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}

fn read_record<T: DeserializeOwned>(parent: &Path, name: &str) -> Result<Option<T>> {
    let path = parent.join(name);
    if !present(&path)? {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(
        &Node::file(&path, false, false)?.read(MAX_STATE)?,
    )?))
}

struct Setup {
    root: PathBuf,
    parent: PathBuf,
    state: PathBuf,
    parents: Vec<Node>,
    state_pin: Node,
    metadata: Metadata,
}

impl Setup {
    fn open(installation: &Installation, metadata: Metadata, parents: Vec<Node>) -> Result<Self> {
        let root = installation.root().to_owned();
        let state = installation.state().to_owned();
        let parent = root.parent().context("missing setup parent")?.to_owned();
        let state_pin = Node::directory(&state, false)?;
        ensure!(
            parents
                .last()
                .is_some_and(|pin| pin.id.volume == state_pin.id.volume),
            "setup state is on another volume"
        );
        Ok(Self {
            root,
            parent,
            state,
            parents,
            state_pin,
            metadata,
        })
    }

    fn write(&self, name: &str, record: &impl Serialize) -> Result<()> {
        let bytes = serde_json::to_vec(record)?;
        ensure!(
            bytes.len() as u64 <= MAX_STATE,
            "setup transaction exceeds limit"
        );
        fixed_write(&self.state, &self.state_pin, name, &bytes)
    }

    fn remove_record(&self, name: &str) -> Result<()> {
        let path = self.state.join(name);
        if present(&path)? {
            Node::file(&path, true, false)?.delete()?;
        }
        Ok(())
    }

    fn mark_blocked(&self) -> Result<()> {
        let path = self.state.join(BLOCK);
        if present(&path)? {
            ensure!(
                Node::file(&path, false, false)?.read(1024)? == MARKER,
                "foreign setup launch marker"
            );
        } else {
            fixed_write(&self.state, &self.state_pin, BLOCK, MARKER)?;
        }
        Ok(())
    }

    fn unblock(&self) -> Result<()> {
        let path = self.state.join(BLOCK);
        if present(&path)? {
            let pin = Node::file(&path, true, false)?;
            ensure!(pin.read(1024)? == MARKER, "foreign setup launch marker");
            pin.delete()?;
        }
        Ok(())
    }

    fn phase(&self, journal: &mut Journal, phase: Phase) -> Result<()> {
        let previous = journal.phase;
        journal.phase = phase;
        if let Err(error) = self.write(JOURNAL, journal) {
            journal.phase = previous;
            return Err(error);
        }
        Ok(())
    }

    fn move_to_state(&self, node: &Node, name: &str) -> Result<()> {
        node.rename(&self.state_pin, OsStr::new(name), false)
    }

    fn publish(&self, node: &Node) -> Result<()> {
        node.rename(
            self.parents.last().context("missing setup parent pin")?,
            self.root.file_name().context("missing setup root name")?,
            false,
        )
    }

    fn load_installation(&self) -> Result<Option<Installed>> {
        let result: Option<Installed> = read_record(&self.state, OWNERSHIP)?;
        if let Some(installed) = &result {
            ensure!(
                installed.schema == 1 && !installed.tree.legacy,
                "invalid installed ownership receipt"
            );
            installed.tree.validate()?;
        }
        Ok(result)
    }

    fn recover(&self) -> Result<()> {
        let Some(journal) = read_record::<Journal>(&self.state, JOURNAL)? else {
            ensure!(
                !present(&self.state.join(PREVIOUS))?
                    && !present(&self.state.join(STAGED))?
                    && !present(&self.state.join(BLOCK))?,
                "setup has unrecognized recovery state"
            );
            return Ok(());
        };
        ensure!(
            journal.schema == 1 && valid_nonce(&journal.nonce),
            "invalid setup journal"
        );
        if let Some(old) = &journal.old {
            old.validate()?;
        }
        match journal.phase {
            Phase::Uninstall | Phase::UninstallMetadata => self.resume_uninstall(&journal),
            _ => {
                let candidate = journal
                    .candidate
                    .as_ref()
                    .context("missing candidate recovery receipt")?;
                ensure!(
                    candidate.schema == 1 && !candidate.tree.legacy,
                    "invalid recovery candidate"
                );
                candidate.tree.validate()?;
                ensure!(
                    journal
                        .old
                        .as_ref()
                        .is_none_or(|old| old.root != candidate.tree.root),
                    "conflicting product generations"
                );
                if journal.phase == Phase::Committed {
                    self.finish_commit(&journal, candidate)
                } else {
                    self.rollback(&journal, candidate, None)
                }
            }
        }
    }

    fn rollback(
        &self,
        journal: &Journal,
        candidate: &Installed,
        existing: Option<&LegacyBarrier>,
    ) -> Result<()> {
        self.metadata
            .validate_transition(&journal.prior, &candidate.metadata)?;
        let staged = self.state.join(STAGED);
        let previous = self.state.join(PREVIOUS);
        let previous_exists = present(&previous)?;
        let staged_exists = present(&staged)?;
        let stable = present(&self.root)?
            .then(|| Node::directory(&self.root, true))
            .transpose()?;
        let mut old_stable = false;
        let mut candidate_stable = false;
        if let Some(stable) = stable {
            old_stable = journal
                .old
                .as_ref()
                .is_some_and(|old| stable.id == old.root);
            candidate_stable = stable.id == candidate.tree.root;
            ensure!(
                old_stable || candidate_stable,
                "unrecognized stable installation during repair"
            );
        }
        let legacy_path = if old_stable { &self.root } else { &previous };
        let owned_barrier = if existing.is_none() {
            LegacyBarrier::hold(&self.state_pin, legacy_path, journal.old.as_ref())?
        } else {
            None
        };
        let legacy = existing.or(owned_barrier.as_ref());
        let mut previous_root = None;
        match &journal.old {
            Some(old) => {
                ensure!(
                    old_stable != previous_exists,
                    "previous installation cannot be identified uniquely"
                );
                if old_stable {
                    OpenTree::verify_with_barrier(&self.root, old, Inventory::Exact, legacy)?;
                } else {
                    previous_root = Some(
                        OpenTree::verify_with_barrier(&previous, old, Inventory::Exact, legacy)?
                            .into_root(),
                    );
                }
            }
            None => ensure!(!previous_exists, "unexpected previous installation"),
        }
        ensure!(
            !candidate_stable || !staged_exists,
            "candidate appears in two locations"
        );
        if staged_exists {
            let inventory = if previous_exists {
                Inventory::Exact
            } else {
                Inventory::InterruptedCleanup
            };
            OpenTree::verify(&staged, &candidate.tree, inventory)?;
        }
        if candidate_stable {
            let root = OpenTree::verify(&self.root, &candidate.tree, Inventory::Exact)?.into_root();
            self.move_to_state(&root, STAGED)?;
        }
        if let Some(root) = &previous_root {
            self.publish(root)?;
        }
        self.metadata
            .apply(&candidate.metadata, &journal.prior, &journal.nonce)?;
        self.unblock()?;
        if present(&staged)? {
            OpenTree::verify(&staged, &candidate.tree, Inventory::InterruptedCleanup)?
                .cleanup(&staged, false, false)?;
        }
        self.remove_record(JOURNAL)?;
        drop((owned_barrier, previous_root));
        Ok(())
    }

    fn finish_commit(&self, journal: &Journal, candidate: &Installed) -> Result<()> {
        self.publish_receipt(candidate)?;
        self.cleanup_commit(journal)
    }

    fn publish_receipt(&self, candidate: &Installed) -> Result<()> {
        let _stable = OpenTree::verify(&self.root, &candidate.tree, Inventory::Exact)?;
        ensure!(
            self.metadata.read()? == candidate.metadata,
            "committed product metadata changed"
        );
        ensure!(
            !present(&self.state.join(STAGED))?,
            "committed transaction has unexplained staging"
        );
        self.write(OWNERSHIP, candidate)?;
        self.unblock()
    }

    fn cleanup_commit(&self, journal: &Journal) -> Result<()> {
        let previous = self.state.join(PREVIOUS);
        if present(&previous)? {
            let old = journal
                .old
                .as_ref()
                .context("unexpected previous installation")?;
            OpenTree::verify(&previous, old, Inventory::InterruptedCleanup)?
                .cleanup(&previous, false, false)?;
        }
        self.remove_record(JOURNAL)
    }

    fn before_install(&self, prior: &Snapshot) -> Result<Option<OpenTree>> {
        match (self.load_installation()?, present(&self.root)?) {
            (Some(installed), true) => {
                ensure!(
                    *prior == installed.metadata,
                    "unrecognized registered installation or shortcut"
                );
                Ok(Some(OpenTree::verify(
                    &self.root,
                    &installed.tree,
                    Inventory::Exact,
                )?))
            }
            (None, false) => {
                ensure!(
                    *prior == Snapshot::default(),
                    "Unrecognized installation registration. Remove the prior installation separately before setup."
                );
                Ok(None)
            }
            (None, true) => {
                self.metadata.verify_legacy(prior)?;
                let legacy = serde_json::from_str(LEGACY_INVENTORY)?;
                Ok(Some(OpenTree::open(
                    &self.root,
                    &legacy,
                    true,
                    Inventory::Exact,
                    None,
                )?))
            }
            (Some(_), false) => bail!("registered installation is missing its product folder"),
        }
    }

    fn install(&self, scratch: &Path, nonce: String) -> Result<()> {
        let _scratch = Node::boundaries(scratch)?;
        let candidate = scratch.join("payload");
        let (manifest, mut members, uninstaller) = verify_package(scratch, &candidate)?;
        self.metadata.reject_msi()?;
        self.recover()?;
        let prior = self.metadata.read()?;
        let old_pins = self.before_install(&prior)?;
        if let Some(file) = old_pins.as_ref().and_then(|tree| tree.files.get(MANIFEST)) {
            let installed: crate::Manifest =
                serde_json::from_slice(&file.read(crate::MAX_MANIFEST)?)?;
            ensure!(
                numeric_version(&manifest.version)? >= numeric_version(&installed.version)?,
                "This installer is older than the installed Chronobreak version."
            );
        }
        self.metadata.observe_run(false)?;
        let desired = self.metadata.desired(&manifest.version, scratch)?;
        let candidate_pin = Node::directory(&candidate, false)?;
        uninstaller.rename(&candidate_pin, OsStr::new(UNINSTALLER), false)?;
        drop((uninstaller, candidate_pin));
        let staged_pins = OpenTree::open(&candidate, &members, false, Inventory::Exact, None)?;
        let old = if let Some(tree) = &old_pins {
            match self.load_installation()? {
                Some(installed) => Some(installed.tree),
                None => Some(tree.receipt(serde_json::from_str(LEGACY_INVENTORY)?, true)),
            }
        } else {
            None
        };
        let installed = Installed {
            schema: 1,
            tree: staged_pins.receipt(std::mem::take(&mut members), false),
            metadata: desired,
        };
        installed.tree.validate()?;
        ensure!(
            staged_pins.root.id.volume == self.state_pin.id.volume,
            "setup staging is on another volume"
        );
        drop((old_pins, staged_pins));
        let legacy = LegacyBarrier::hold(&self.state_pin, &self.root, old.as_ref())?;
        let mut journal = Journal {
            schema: 1,
            nonce,
            phase: Phase::Prepared,
            old,
            prior,
            candidate: Some(installed),
        };
        self.write(JOURNAL, &journal)?;
        let result = self.replace(&mut journal, &candidate, legacy.as_ref());
        if let Err(error) = result {
            let installed = journal
                .candidate
                .as_ref()
                .context("missing candidate ownership")?;
            if journal.phase == Phase::Committed {
                self.publish_receipt(installed)?;
                return Err(error)
                    .context("the new installation committed and cleanup needs repair");
            }
            return match self.rollback(&journal, installed, legacy.as_ref()) {
                Ok(()) => Err(error).context("setup failed and restored the prior state"),
                Err(recovery) => {
                    Err(error).context(format!("setup retained recovery state: {recovery:#}"))
                }
            };
        }
        let installed = journal
            .candidate
            .as_ref()
            .context("missing completed candidate")?;
        self.publish_receipt(installed)?;
        drop(legacy);
        self.cleanup_commit(&journal)
    }

    fn replace(
        &self,
        journal: &mut Journal,
        incoming: &Path,
        legacy: Option<&LegacyBarrier>,
    ) -> Result<()> {
        self.mark_blocked()?;
        self.phase(journal, Phase::Stage)?;
        fault("before-stage")?;
        let candidate = &journal
            .candidate
            .as_ref()
            .context("missing candidate ownership")?
            .tree;
        let staged = OpenTree::verify(incoming, candidate, Inventory::Exact)?.into_root();
        self.move_to_state(&staged, STAGED)?;
        drop(staged);
        fault("after-stage")?;
        if journal.old.is_some() {
            self.phase(journal, Phase::MovePrevious)?;
            fault("before-previous")?;
            let old = journal.old.as_ref().context("missing previous ownership")?;
            let old = OpenTree::verify_with_barrier(&self.root, old, Inventory::Exact, legacy)?
                .into_root();
            self.move_to_state(&old, PREVIOUS)?;
            fault("after-previous")?;
        }
        self.phase(journal, Phase::Publish)?;
        fault("before-publish")?;
        let candidate = &journal
            .candidate
            .as_ref()
            .context("missing candidate ownership")?
            .tree;
        let staged =
            OpenTree::verify(&self.state.join(STAGED), candidate, Inventory::Exact)?.into_root();
        self.publish(&staged)?;
        drop(staged);
        fault("after-publish")?;
        self.phase(journal, Phase::Metadata)?;
        let candidate = journal
            .candidate
            .as_ref()
            .context("missing candidate ownership")?;
        let _stable = OpenTree::verify(&self.root, &candidate.tree, Inventory::Exact)?;
        fault("before-metadata")?;
        self.metadata
            .apply(&journal.prior, &candidate.metadata, &journal.nonce)?;
        fault("after-metadata")?;
        self.phase(journal, Phase::Committed)?;
        fault("after-commit")?;
        Ok(())
    }

    fn uninstall(&self) -> Result<()> {
        self.recover()?;
        let Some(installed) = self.load_installation()? else {
            println!("Chronobreak is already uninstalled.");
            return Ok(());
        };
        ensure!(
            Some(installed.tree.files[MANIFEST].1.sha256.as_str())
                == option_env!("CHRONOBREAK_PAYLOAD_SHA256"),
            "Use the uninstaller from the currently installed Chronobreak version."
        );
        ensure!(
            self.metadata.read()? == installed.metadata,
            "unrecognized product registration"
        );
        OpenTree::verify(&self.root, &installed.tree, Inventory::PreserveUnknown)?;
        let journal = Journal {
            schema: 1,
            nonce: new_nonce()?,
            phase: Phase::Uninstall,
            old: Some(installed.tree),
            prior: installed.metadata,
            candidate: None,
        };
        self.write(JOURNAL, &journal)?;
        self.resume_uninstall(&journal)
    }

    fn resume_uninstall(&self, journal: &Journal) -> Result<()> {
        let tree = journal
            .old
            .as_ref()
            .context("missing uninstall ownership")?;
        ensure!(
            !tree.legacy
                && journal.candidate.is_none()
                && !present(&self.state.join(STAGED))?
                && !present(&self.state.join(PREVIOUS))?,
            "unexpected uninstall recovery state"
        );
        let empty = Snapshot::default();
        self.metadata.validate_transition(&journal.prior, &empty)?;
        self.mark_blocked()?;
        if present(&self.root)? {
            let pins = OpenTree::verify(&self.root, tree, Inventory::PreserveUnknown)?;
            pins.cleanup(&self.root, true, true)?;
        } else {
            ensure!(
                journal.phase == Phase::UninstallMetadata,
                "uninstall product folder is missing"
            );
        }
        self.metadata.observe_run(true)?;
        let final_journal = Journal {
            phase: Phase::UninstallMetadata,
            schema: journal.schema,
            old: journal.old.clone(),
            prior: journal.prior.clone(),
            nonce: journal.nonce.clone(),
            candidate: None,
        };
        self.write(JOURNAL, &final_journal)?;
        self.metadata
            .apply(&journal.prior, &empty, &journal.nonce)?;
        if present(&self.root)? {
            OpenTree::verify(&self.root, tree, Inventory::PreserveUnknown)?
                .cleanup(&self.root, true, false)?;
        }
        self.remove_record(OWNERSHIP)?;
        self.unblock()?;
        self.remove_record(JOURNAL)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    signing: String,
    payload_sha256: String,
    coordinator_sha256: String,
    uninstaller_sha256: String,
}

fn verify_package(
    scratch: &Path,
    payload: &Path,
) -> Result<(crate::Manifest, BTreeMap<String, Member>, Node)> {
    let (manifest, mut members, envelope) = package_manifest(scratch, payload)?;
    OpenTree::open(payload, &members, false, Inventory::Exact, None)?;
    let uninstaller = Node::file(&scratch.join(UNINSTALLER), true, false)?;
    let member = fingerprint(&uninstaller.read(16 * 1024 * 1024)?);
    ensure!(
        member.sha256 == envelope.uninstaller_sha256,
        "packaged uninstaller hash mismatch"
    );
    members.insert(UNINSTALLER.into(), member);
    Ok((manifest, members, uninstaller))
}

fn package_manifest(
    scratch: &Path,
    payload: &Path,
) -> Result<(crate::Manifest, BTreeMap<String, Member>, Envelope)> {
    let bound =
        option_env!("CHRONOBREAK_PAYLOAD_SHA256").context("no payload is bound to setup")?;
    let envelope: Envelope =
        read_record(scratch, "envelope.json")?.context("missing setup envelope")?;
    ensure!(
        envelope.schema == 1
            && matches!(envelope.signing.as_str(), "unsigned-local" | "signed")
            && envelope.payload_sha256 == bound,
        "invalid setup envelope"
    );
    let exe = std::env::current_exe()?;
    let file = Node::file(&exe, false, false)?;
    ensure!(
        fingerprint(&file.read(16 * 1024 * 1024)?).sha256 == envelope.coordinator_sha256,
        "setup coordinator hash mismatch"
    );
    let _parent = Node::directory(payload, true)?;
    let manifest_bytes =
        Node::file(&payload.join(MANIFEST), false, false)?.read(crate::MAX_MANIFEST)?;
    let manifest_member = fingerprint(&manifest_bytes);
    ensure!(
        manifest_member.sha256 == bound,
        "payload manifest hash mismatch"
    );
    let manifest: crate::Manifest = serde_json::from_slice(&manifest_bytes)?;
    ensure!(
        manifest.schema == 1 && numeric_version(&manifest.version).is_ok(),
        "unsupported payload manifest"
    );
    let mut members = manifest.files.clone();
    members.insert(MANIFEST.into(), manifest_member);
    validate_members(members.keys().map(String::as_str), false, false)?;
    Ok((manifest, members, envelope))
}

pub fn clean_extraction(scratch: &Path) -> Result<()> {
    ensure!(
        std::env::current_exe()?.parent() == Some(scratch)
            && scratch
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|name| name.strip_prefix("Chronobreak-setup-"))
                .is_some_and(valid_nonce),
        "invalid setup extraction directory"
    );
    let _pins = Node::boundaries(scratch)?;
    let root = scratch.join("payload");
    let (_, mut members, envelope) = package_manifest(scratch, &root)?;
    let uninstaller = root.join(UNINSTALLER);
    if present(&uninstaller)? {
        let file = Node::file(&uninstaller, false, false)?;
        let member = fingerprint(&file.read(16 * 1024 * 1024)?);
        ensure!(
            member.sha256 == envelope.uninstaller_sha256,
            "unrecognized extraction content"
        );
        members.insert(UNINSTALLER.into(), member);
    }
    OpenTree::open(&root, &members, false, Inventory::InterruptedCleanup, None)?
        .cleanup(&root, false, false)
}

fn numeric_version(version: &str) -> Result<[u16; 3]> {
    let components = version
        .split('.')
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()?;
    components
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid Windows setup version"))
}

fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 32 && nonce.bytes().all(|b| b.is_ascii_hexdigit())
}

fn new_nonce() -> Result<String> {
    use windows::Win32::System::Com::CoCreateGuid;
    // SAFETY: this writes a GUID generated by Windows.
    let nonce = unsafe { CoCreateGuid() }?;
    Ok(format!("{:032x}", nonce.to_u128()))
}

pub fn dispatch(args: &[OsString]) -> Option<Result<()>> {
    let command = args.first()?.to_str()?;
    match command {
        "install" | "uninstall" | "repair" => Some(run(command, &args[1..])),
        _ => None,
    }
}

fn run(command: &str, args: &[OsString]) -> Result<()> {
    #[cfg(feature = "fixture")]
    let (installation, args, test_name) = {
        let (fixture, args) = args
            .split_first()
            .context("missing isolated setup fixture path")?;
        let path = Path::new(fixture);
        let _pins = Node::boundaries(path)?;
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .context("invalid fixture name")?;
        ensure!(
            name.strip_prefix("chronobreak-fixture-")
                .is_some_and(valid_nonce)
                && Node::file(&path.join(".setup-fixture"), false, false)?.read(64)?
                    == b"Chronobreak disposable test fixture",
            "invalid isolated setup fixture"
        );
        (Installation::fixture(path), args, name)
    };
    #[cfg(not(feature = "fixture"))]
    let installation = Installation::production()?;

    let parent = installation
        .root()
        .parent()
        .context("missing setup parent")?;
    let parents = Node::boundaries(parent)?;
    #[cfg(feature = "fixture")]
    let maintenance = installation.begin_fixture_maintenance()?;
    #[cfg(not(feature = "fixture"))]
    let maintenance = installation.begin_maintenance()?;
    #[cfg(feature = "fixture")]
    let metadata = Metadata::fixture(installation.root(), parent, test_name)?;
    #[cfg(not(feature = "fixture"))]
    let metadata = Metadata::production(installation.root())?;
    let setup = Setup::open(&installation, metadata, parents)?;
    let result = match (command, args) {
        ("install", [scratch]) => {
            let path = Path::new(scratch);
            let nonce = path
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|name| name.strip_prefix("Chronobreak-setup-"))
                .filter(|nonce| valid_nonce(nonce))
                .context("invalid extraction scratch name")?;
            ensure!(
                path.parent() == Some(setup.parent.as_path()),
                "extraction scratch is outside the current user setup folder"
            );
            setup.install(path, nonce.to_owned())
        }
        ("repair", []) => setup.recover(),
        ("uninstall", []) => setup.uninstall(),
        _ => bail!("invalid setup arguments"),
    };
    drop(maintenance);
    result
}

pub fn fault(point: &str) -> Result<()> {
    #[cfg(feature = "fixture")]
    if let Ok(spec) = std::env::var("CHRONOBREAK_SETUP_TEST_FAULT") {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static OCCURRENCE: AtomicUsize = AtomicUsize::new(0);
        let mut fields = spec.split(':');
        let kind = fields.next();
        let name = fields.next();
        let count = fields.next().and_then(|n| n.parse().ok()).unwrap_or(1);
        if name == Some(point) && OCCURRENCE.fetch_add(1, Ordering::Relaxed) + 1 == count {
            if kind == Some("exit") {
                std::process::exit(78);
            }
            bail!("injected error at {point}");
        }
    }
    let _ = point;
    Ok(())
}
