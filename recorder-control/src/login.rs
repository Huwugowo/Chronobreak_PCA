//! Exact owned Run-value identity, independent of the dormant TOML preference.
use anyhow::{Result, bail};
use std::path::Path;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "Chronobreak Recorder";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

pub fn login_command(executable: &Path) -> Result<String> {
    let path = executable
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("recorder path is not Unicode"))?;
    if !executable.is_absolute() || path.contains(['"', '\0']) {
        bail!("invalid recorder startup path");
    }
    let command = format!("\"{path}\"");
    if command.encode_utf16().count() > 260 {
        bail!("recorder startup command exceeds Windows Run limit");
    }
    Ok(command)
}

impl RegistryValue {
    pub fn command(command: &str) -> Self {
        Self {
            kind: 1,
            bytes: command
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect(),
        }
    }
}

pub trait Registration {
    fn read(&self) -> Result<Option<RegistryValue>>;
    fn write(&self, value: Option<&RegistryValue>) -> Result<()>;
}

#[cfg(windows)]
pub struct WindowsRegistration {
    key: String,
    name: String,
}

#[cfg(windows)]
impl WindowsRegistration {
    pub fn production() -> Self {
        Self {
            key: RUN_KEY.into(),
            name: RUN_VALUE.into(),
        }
    }
    /// Only isolated test callers should use a non-production key.
    pub fn isolated(key: String, name: String) -> Self {
        Self { key, name }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the successfully opened key.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    impl Registration for WindowsRegistration {
        fn read(&self) -> Result<Option<RegistryValue>> {
            let key = wide(&self.key);
            let name = wide(&self.name);
            let mut kind = REG_VALUE_TYPE::default();
            let mut length = 0;
            // SAFETY: NUL-terminated names and valid type/length output pointers.
            let result = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    RRF_RT_ANY | RRF_NOEXPAND,
                    Some(&mut kind),
                    None,
                    Some(&mut length),
                )
            };
            if result == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            result.ok()?;
            if length > 65536 {
                bail!("startup registration exceeds safe bound");
            }
            let mut bytes = vec![0; length as usize];
            // SAFETY: owned writable buffer exactly matches the declared byte length.
            unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    RRF_RT_ANY | RRF_NOEXPAND,
                    Some(&mut kind),
                    Some(bytes.as_mut_ptr().cast()),
                    Some(&mut length),
                )
                .ok()?;
            }
            bytes.truncate(length as usize);
            Ok(Some(RegistryValue {
                kind: kind.0,
                bytes,
            }))
        }

        fn write(&self, value: Option<&RegistryValue>) -> Result<()> {
            let key = wide(&self.key);
            let name = wide(&self.name);
            let mut handle = HKEY::default();
            let result = if value.is_some() {
                // SAFETY: names are terminated and handle is valid writable output.
                unsafe {
                    RegCreateKeyExW(
                        HKEY_CURRENT_USER,
                        PCWSTR(key.as_ptr()),
                        None,
                        PCWSTR::null(),
                        REG_OPTION_NON_VOLATILE,
                        KEY_SET_VALUE,
                        None,
                        &mut handle,
                        None,
                    )
                }
            } else {
                // SAFETY: read-only input name and valid writable handle output.
                unsafe {
                    RegOpenKeyExW(
                        HKEY_CURRENT_USER,
                        PCWSTR(key.as_ptr()),
                        None,
                        KEY_SET_VALUE,
                        &mut handle,
                    )
                }
            };
            if result == ERROR_FILE_NOT_FOUND && value.is_none() {
                return Ok(());
            }
            result.ok()?;
            let handle = Key(handle);
            match value {
                Some(value) => {
                    // SAFETY: key is live; bytes remain owned for the synchronous copy.
                    unsafe {
                        RegSetValueExW(
                            handle.0,
                            PCWSTR(name.as_ptr()),
                            None,
                            REG_VALUE_TYPE(value.kind),
                            Some(&value.bytes),
                        )
                        .ok()?;
                    }
                }
                None => {
                    // SAFETY: key is live and value name is terminated.
                    let result = unsafe { RegDeleteValueW(handle.0, PCWSTR(name.as_ptr())) };
                    if result != ERROR_FILE_NOT_FOUND {
                        result.ok()?;
                    }
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_is_quoted_absolute_tray_only_and_bounded() {
        #[cfg(windows)]
        let path = Path::new(r"C:\Program Files\Chronobreak\recorder.exe");
        #[cfg(not(windows))]
        let path = Path::new("/opt/Chronobreak/recorder.exe");
        assert_eq!(
            login_command(path).unwrap(),
            format!("\"{}\"", path.display())
        );
        for path in ["relative.exe", "bad\"path", "bad\0path"] {
            assert!(login_command(Path::new(path)).is_err());
        }
        assert!(login_command(&path.with_file_name("x".repeat(261))).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn disposable_hkcu_value_round_trips_without_touching_the_run_key() {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyW};
        use windows::core::PCWSTR;
        let root = tempfile::tempdir().unwrap();
        let key = format!(
            r"Software\Chronobreak\Tests\{}",
            root.path().file_name().unwrap().to_string_lossy()
        );
        let registration = WindowsRegistration::isolated(key.clone(), "Fixture recorder".into());
        struct Cleanup(WindowsRegistration, String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.write(None);
                let key: Vec<u16> = self.1.encode_utf16().chain(Some(0)).collect();
                // SAFETY: this unique temporary leaf was created solely by this fixture.
                unsafe {
                    let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()));
                }
            }
        }
        let fixture = Cleanup(registration, key);
        assert_eq!(fixture.0.read().unwrap(), None);
        let value = RegistryValue::command(r#""C:\Fixture Space\recorder.exe""#);
        fixture.0.write(Some(&value)).unwrap();
        assert_eq!(fixture.0.read().unwrap(), Some(value));
        fixture.0.write(None).unwrap();
        assert_eq!(fixture.0.read().unwrap(), None);
    }
}
