use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chronobreak_recorder_control::config_lock::ConfigLock;
use chronobreak_recorder_control::login::{
    Registration, RegistryValue, WindowsRegistration, login_command,
};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use windows::Win32::System::{Com::*, Registry::*};
use windows::Win32::UI::Shell::{
    IShellLinkDataList, IShellLinkW, SLDF_DISABLE_LINK_PATH_TRACKING, SLDF_FORCE_NO_LINKTRACK,
    SLGP_RAWPATH, ShellLink,
};
use windows::core::{Interface, PCWSTR, PWSTR};

use crate::fs_guard::{Node, present, wide};

#[cfg(not(feature = "fixture"))]
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\League Replay";
#[cfg(not(feature = "fixture"))]
const LEGACY_KEY: &str = r"Software\leaguereplay\League Replay";
const MSI_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{2F5F2965-28AC-4978-9F2F-B54016C646F8}";
const MAX_LINK: u64 = 32 * 1024;
const LINKS: usize = 3;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    uninstall: Values,
    legacy: Values,
    links: [Option<Vec<u8>>; LINKS],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Value {
    kind: u32,
    bytes: Vec<u8>,
}

impl Value {
    fn string(text: &str) -> Self {
        let registration = RegistryValue::command(text);
        Self {
            kind: registration.kind,
            bytes: registration.bytes,
        }
    }

    fn number(number: u32) -> Self {
        Self {
            kind: REG_DWORD.0,
            bytes: number.to_le_bytes().to_vec(),
        }
    }
}

type Values = BTreeMap<String, Value>;

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this handle is owned by the wrapper.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(hive: HKEY, name: &str, access: REG_SAM_FLAGS) -> Result<Option<Key>> {
    let name = wide(OsStr::new(name));
    let mut key = HKEY::default();
    // SAFETY: the subkey is NUL-terminated and the handle output is live.
    match unsafe { RegOpenKeyExW(hive, PCWSTR(name.as_ptr()), None, access, &mut key) } {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        status => {
            status.ok()?;
            Ok(Some(Key(key)))
        }
    }
}

fn values(key: &Key) -> Result<Values> {
    let mut subkeys = 0;
    let mut count = 0;
    // SAFETY: the handle and requested output fields remain live.
    unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            Some(&mut subkeys),
            None,
            None,
            Some(&mut count),
            None,
            None,
            None,
            None,
        )
    }
    .ok()?;
    ensure!(
        subkeys == 0 && count <= 32,
        "unexpected product registry contents"
    );
    let mut result = Values::new();
    for index in 0..count {
        let mut name = [0u16; 128];
        let mut name_len = name.len() as u32;
        let mut data = [0u8; 4096];
        let mut data_len = data.len() as u32;
        let mut kind = 0;
        // SAFETY: both buffers and their sizes are provided to the registry API.
        unsafe {
            RegEnumValueW(
                key.0,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        }
        .ok()?;
        let name = String::from_utf16(&name[..name_len as usize])?.to_ascii_lowercase();
        result.insert(
            name,
            Value {
                kind,
                bytes: data[..data_len as usize].to_vec(),
            },
        );
    }
    Ok(result)
}

fn read_key(key: &str) -> Result<Values> {
    open(HKEY_CURRENT_USER, key, KEY_READ)?
        .as_ref()
        .map(values)
        .transpose()
        .map(Option::unwrap_or_default)
}

fn write_key(name: &str, old: &Values, new: &Values) -> Result<()> {
    let name_wide = wide(OsStr::new(name));
    let key = if let Some(key) = open(HKEY_CURRENT_USER, name, KEY_READ | KEY_SET_VALUE)? {
        key
    } else if new.is_empty() {
        return Ok(());
    } else {
        let mut key = HKEY::default();
        // SAFETY: the subkey is NUL-terminated and the handle output is live.
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(name_wide.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        }
        .ok()?;
        Key(key)
    };
    let existing = values(&key)?;
    validate_values(&existing, old, new)?;
    for (index, value) in new {
        if existing.get(index) != Some(value) {
            let index = wide(OsStr::new(index));
            crate::transaction::fault("before-registry-value")?;
            // SAFETY: the key, NUL-terminated value name and byte slice are live.
            unsafe {
                RegSetValueExW(
                    key.0,
                    PCWSTR(index.as_ptr()),
                    None,
                    REG_VALUE_TYPE(value.kind),
                    Some(&value.bytes),
                )
            }
            .ok()?;
            crate::transaction::fault("registry-value")?;
        }
    }
    for index in existing.keys().filter(|index| !new.contains_key(*index)) {
        let index = wide(OsStr::new(index));
        crate::transaction::fault("before-registry-value")?;
        // SAFETY: the key and NUL-terminated value name are live.
        unsafe { RegDeleteValueW(key.0, PCWSTR(index.as_ptr())) }.ok()?;
        crate::transaction::fault("registry-value")?;
    }
    ensure!(values(&key)? == *new, "product registration changed");
    // SAFETY: the key is live and owned.
    unsafe { RegFlushKey(key.0) }.ok()?;
    if new.is_empty() {
        drop(key);
        // SAFETY: the fixed subkey name is NUL-terminated and was verified empty.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(name_wide.as_ptr())) }.ok()?;
    }
    Ok(())
}

fn validate_values(current: &Values, old: &Values, new: &Values) -> Result<()> {
    for name in current
        .keys()
        .chain(old.keys())
        .chain(new.keys())
        .collect::<BTreeSet<_>>()
    {
        ensure!(
            current.get(name) == old.get(name) || current.get(name) == new.get(name),
            "unrecognized product registration"
        );
    }
    Ok(())
}

struct Com;
impl Com {
    fn start() -> Result<Self> {
        // SAFETY: this coordinator initializes COM on its main thread.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: this balances successful initialization on this thread.
        unsafe { CoUninitialize() };
    }
}

fn link() -> Result<IShellLinkW> {
    // SAFETY: callers have initialized COM and ShellLink is a system class.
    Ok(unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?)
}

fn owned_link(path: &Path, executable: &Path) -> Result<()> {
    let _com = Com::start()?;
    let link = link()?;
    let persistence: IPersistFile = link.cast()?;
    let name = wide(path.as_os_str());
    let mut target = [0u16; 32768];
    let mut args = [0u16; 2];
    // SAFETY: COM objects and the bounded input/output buffers are live.
    unsafe {
        persistence.Load(PCWSTR(name.as_ptr()), STGM_READ)?;
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
        link.GetArguments(&mut args)?;
    }
    let null = target
        .iter()
        .position(|unit| *unit == 0)
        .context("invalid shortcut target")?;
    let target = OsString::from_wide(&target[..null]);
    ensure!(
        target.eq_ignore_ascii_case(executable.as_os_str()) && args[0] == 0,
        "unrecognized product shortcut"
    );
    Ok(())
}

pub struct Metadata {
    root: PathBuf,
    config: PathBuf,
    uninstall: String,
    legacy: String,
    msi: Option<String>,
    links: [PathBuf; LINKS],
    run: WindowsRegistration,
}

impl Metadata {
    #[cfg(not(feature = "fixture"))]
    pub fn production(root: &Path) -> Result<Self> {
        let roamed = known_folder(&windows::Win32::UI::Shell::FOLDERID_Programs)?;
        let desktop = known_folder(&windows::Win32::UI::Shell::FOLDERID_Desktop)?;
        let config = std::env::var_os("LEAGUE_REPLAY_CONFIG")
            .map(PathBuf::from)
            .unwrap_or(
                directories::ProjectDirs::from("", "", "LeagueReplay")
                    .context("cannot locate settings folder")?
                    .config_dir()
                    .join("config.toml"),
            );
        Ok(Self {
            root: root.into(),
            config,
            uninstall: UNINSTALL_KEY.into(),
            legacy: LEGACY_KEY.into(),
            msi: None,
            links: [
                roamed.join("Chronobreak.lnk"),
                roamed.join("League Replay.lnk"),
                desktop.join("League Replay.lnk"),
            ],
            run: WindowsRegistration::production(),
        })
    }

    #[cfg(feature = "fixture")]
    pub fn fixture(root: &Path, parent: &Path, name: &str) -> Result<Self> {
        let key = format!(r"Software\ChronobreakSetupFixture\{name}");
        let programs = parent.join("Programs");
        let desktop = parent.join("Desktop");
        for path in [&programs, &desktop] {
            if !present(path)? {
                std::fs::create_dir(path)?;
            }
        }
        Ok(Self {
            root: root.into(),
            config: parent.join("config.toml"),
            uninstall: format!(r"{key}\Uninstall"),
            legacy: format!(r"{key}\Legacy"),
            msi: Some(format!(r"{key}\Msi")),
            links: [
                programs.join("Chronobreak.lnk"),
                programs.join("League Replay.lnk"),
                desktop.join("League Replay.lnk"),
            ],
            run: WindowsRegistration::isolated(
                format!(r"{key}\Run"),
                "Chronobreak Recorder".into(),
            ),
        })
    }

    pub fn reject_msi(&self) -> Result<()> {
        if let Some(key) = &self.msi {
            ensure!(
                open(HKEY_CURRENT_USER, key, KEY_READ)?.is_none(),
                "Remove the MSI beta separately before setup."
            );
            return Ok(());
        }
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
                ensure!(
                    open(hive, MSI_KEY, KEY_READ | view)?.is_none(),
                    "Remove the MSI beta separately before setup."
                );
            }
        }
        Ok(())
    }

    fn registry(&self, brand: &str, version: &str) -> Result<Values> {
        let root = self.root.to_str().context("invalid install path")?;
        let mut values = Values::new();
        for (name, text) in [
            ("mainbinaryname", "league-replay-app.exe"),
            ("displayname", brand),
            ("displayversion", version),
            (
                "publisher",
                if brand == "League Replay" {
                    "leaguereplay"
                } else {
                    "Chronobreak contributors"
                },
            ),
            ("displayicon", &format!(r#""{root}\league-replay-app.exe""#)),
            ("installlocation", &format!(r#""{root}""#)),
            ("uninstallstring", &format!(r#""{root}\uninstall.exe""#)),
        ] {
            values.insert(name.into(), Value::string(text));
        }
        values.insert("nomodify".into(), Value::number(1));
        values.insert("norepair".into(), Value::number(1));
        Ok(values)
    }

    pub fn read(&self) -> Result<Snapshot> {
        let mut links = [None, None, None];
        for (path, bytes) in self.links.iter().zip(&mut links) {
            if present(path)? {
                let _pins = Node::boundaries(path.parent().context("missing shortcut parent")?)?;
                *bytes = Some(Node::file(path, false, false)?.read(MAX_LINK)?);
            }
        }
        Ok(Snapshot {
            uninstall: read_key(&self.uninstall)?,
            legacy: read_key(&self.legacy)?,
            links,
        })
    }

    pub fn verify_legacy(&self, snapshot: &Snapshot) -> Result<()> {
        let mut uninstall = snapshot.uninstall.clone();
        let size = uninstall.remove("estimatedsize");
        ensure!(
            size.is_some_and(|size| size.kind == REG_DWORD.0 && size.bytes.len() == 4),
            "unrecognized historical uninstall registration"
        );
        ensure!(
            uninstall == self.registry("League Replay", "0.1.0")?,
            "unsupported beta registration or custom install path"
        );
        ensure!(
            snapshot.legacy
                == BTreeMap::from([(
                    "".into(),
                    Value::string(self.root.to_str().context("invalid legacy path")?)
                )]),
            "unrecognized historical installation registration"
        );
        ensure!(snapshot.links[0].is_none(), "foreign Chronobreak shortcut");
        for (index, path) in self.links.iter().enumerate().skip(1) {
            if snapshot.links[index].is_some() {
                owned_link(path, &self.root.join("league-replay-app.exe"))?;
            }
        }
        Ok(())
    }

    pub fn desired(&self, version: &str, scratch: &Path) -> Result<Snapshot> {
        let _com = Com::start()?;
        let link = link()?;
        let target = wide(self.root.join("league-replay-app.exe").as_os_str());
        let working = wide(self.root.as_os_str());
        let path = scratch.join("chronobreak-link.lnk");
        ensure!(!present(&path)?, "setup shortcut scratch is occupied");
        let name = wide(path.as_os_str());
        let persistence: IPersistFile = link.cast()?;
        let data: IShellLinkDataList = link.cast()?;
        // SAFETY: COM objects and NUL-terminated paths are live; scratch is pinned.
        unsafe {
            link.SetPath(PCWSTR(target.as_ptr()))?;
            link.SetWorkingDirectory(PCWSTR(working.as_ptr()))?;
            // Keep the link rooted at the stable path when the old executable moves.
            data.SetFlags(
                data.GetFlags()?
                    | SLDF_FORCE_NO_LINKTRACK.0 as u32
                    | SLDF_DISABLE_LINK_PATH_TRACKING.0 as u32,
            )?;
            persistence.Save(PCWSTR(name.as_ptr()), false)?;
        }
        let file = Node::file(&path, true, false)?;
        let bytes = file.read(MAX_LINK)?;
        file.delete()?;
        Ok(Snapshot {
            uninstall: self.registry("Chronobreak", version)?,
            legacy: Values::new(),
            links: [Some(bytes), None, None],
        })
    }

    pub fn validate_transition(&self, from: &Snapshot, to: &Snapshot) -> Result<()> {
        let current = self.read()?;
        validate_values(&current.uninstall, &from.uninstall, &to.uninstall)?;
        validate_values(&current.legacy, &from.legacy, &to.legacy)?;
        for index in 0..LINKS {
            ensure!(
                current.links[index] == from.links[index]
                    || current.links[index] == to.links[index],
                "unrecognized product shortcut"
            );
        }
        Ok(())
    }

    pub fn apply(&self, from: &Snapshot, to: &Snapshot, nonce: &str) -> Result<()> {
        self.validate_transition(from, to)?;
        for (index, path) in self.links.iter().enumerate() {
            crate::transaction::fault("before-shortcut")?;
            write_link(path, &from.links[index], &to.links[index], nonce)?;
            crate::transaction::fault("shortcut")?;
        }
        write_key(&self.legacy, &from.legacy, &to.legacy)?;
        // Keep Add/Remove Programs available until every other uninstall action has succeeded.
        write_key(&self.uninstall, &from.uninstall, &to.uninstall)?;
        ensure!(self.read()? == *to, "product metadata did not verify");
        Ok(())
    }

    pub fn observe_run(&self, remove: bool) -> Result<()> {
        let _config = ConfigLock::acquire(&self.config)?;
        let Some(existing) = self.run.read()? else {
            return Ok(());
        };
        let expected = RegistryValue::command(&login_command(&self.root.join("recorder.exe"))?);
        if existing != expected {
            println!("An unrelated recorder startup entry was left unchanged.");
        } else if remove {
            self.run.write(None)?;
            ensure!(
                self.run.read()?.is_none(),
                "recorder startup entry removal could not be verified"
            );
        }
        Ok(())
    }
}

#[cfg(not(feature = "fixture"))]
fn known_folder(folder: &windows::core::GUID) -> Result<PathBuf> {
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: folder points to a system-known folder ID. COM allocates the result.
    let result = unsafe { SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None) }?;
    // SAFETY: the returned path is NUL-terminated until the allocation is freed.
    let path = unsafe { OsString::from_wide(result.as_wide()) };
    // SAFETY: this frees exactly the allocation returned by Shell.
    unsafe { CoTaskMemFree(Some(result.0.cast())) };
    Ok(path.into())
}

fn write_link(
    path: &Path,
    from: &Option<Vec<u8>>,
    to: &Option<Vec<u8>>,
    nonce: &str,
) -> Result<()> {
    let parent = path.parent().context("missing shortcut parent")?;
    let temporary = parent.join(format!(".chronobreak-{nonce}.lnk.tmp"));
    if !present(path)? && to.is_none() && !present(&temporary)? {
        return Ok(());
    }
    let pins = Node::boundaries(parent)?;
    let parent_pin = pins.last().context("missing shortcut parent")?;
    let current = present(path)?
        .then(|| Node::file(path, true, false))
        .transpose()?;
    let bytes = current
        .as_ref()
        .map(|node| node.read(MAX_LINK))
        .transpose()?;
    ensure!(
        bytes == *from || bytes == *to,
        "foreign shortcut was retained"
    );
    if present(&temporary)? {
        let file = Node::file(&temporary, true, false)?;
        let written = file.read(MAX_LINK)?;
        // Interrupted writes may leave a prefix of the journaled shortcut bytes.
        ensure!(
            from.as_ref().is_some_and(|b| b.starts_with(&written))
                || to.as_ref().is_some_and(|b| b.starts_with(&written)),
            "unknown shortcut scratch was retained"
        );
        file.delete()?;
    }
    if bytes == *to {
        return Ok(());
    }
    match to {
        Some(target) => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(target)?;
            file.sync_all()?;
            drop(file);
            let file = Node::file(&temporary, true, false)?;
            ensure!(file.read(MAX_LINK)? == *target, "shortcut scratch changed");
            let replace = current.is_some();
            drop(current);
            file.rename(
                parent_pin,
                path.file_name().context("missing shortcut name")?,
                replace,
            )?;
        }
        None => current.context("missing shortcut to remove")?.delete()?,
    }
    Ok(())
}
