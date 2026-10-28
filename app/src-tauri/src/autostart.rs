//! Explicit login mutations. Reading a legacy preference never registers anything.
use crate::config::{Config, ConfigMutation, ConfigPatch};
use anyhow::{Result, bail};
use chronobreak_recorder_control::login::{Registration, RegistryValue};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActualRegistration {
    Matching,
    Absent,
    Foreign,
    Unavailable,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutostartStatus {
    pub desired_autostart: bool,
    pub actual_registration: ActualRegistration,
    pub effective_startup: &'static str,
    pub error: Option<String>,
    pub disabled_for_test: bool,
}

impl AutostartStatus {
    pub fn unavailable(desired: bool, disabled: bool, message: &str) -> Self {
        Self {
            desired_autostart: desired,
            actual_registration: ActualRegistration::Unavailable,
            effective_startup: "unknown",
            error: Some(message.into()),
            disabled_for_test: disabled,
        }
    }
}

fn observation(
    desired: bool,
    registration: &impl Registration,
    expected: &RegistryValue,
) -> AutostartStatus {
    let (actual, error) = match registration.read() {
        Ok(None) => (ActualRegistration::Absent, None),
        Ok(Some(value)) if &value == expected => (ActualRegistration::Matching, None),
        Ok(Some(_)) => (
            ActualRegistration::Foreign,
            Some("The startup entry points elsewhere. It was left unchanged.".into()),
        ),
        Err(_) => (
            ActualRegistration::Unavailable,
            Some("Windows startup registration could not be read.".into()),
        ),
    };
    AutostartStatus {
        desired_autostart: desired,
        actual_registration: actual,
        effective_startup: "unknown",
        error,
        disabled_for_test: false,
    }
}

pub fn read(
    path: &Path,
    registration: &impl Registration,
    expected: &RegistryValue,
) -> Result<AutostartStatus> {
    let transaction = ConfigMutation::open(path)?;
    Ok(observation(
        transaction.config.app.autostart,
        registration,
        expected,
    ))
}

/// The caller holds the config lease; rollback compares the current owned value.
fn update_owned<T>(
    registration: &impl Registration,
    expected: &RegistryValue,
    enabled: bool,
    persist: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let prior = registration.read()?;
    if prior.as_ref().is_some_and(|value| value != expected) {
        bail!("startup registration is foreign; no change was made");
    }
    let intended = enabled.then_some(expected);
    let result = (|| {
        registration.write(intended)?;
        if registration.read()?.as_ref() != intended {
            bail!("Windows did not retain the requested startup setting");
        }
        persist()
    })();
    if let Err(error) = result {
        // Never replace an externally changed value with this operation's old value.
        if registration.read()?.as_ref() != intended {
            bail!("startup setting changed concurrently; configuration may differ: {error}");
        }
        registration.write(prior.as_ref()).map_err(|_| {
            anyhow::anyhow!("startup rollback failed; Windows and the stored preference may differ")
        })?;
        if registration.read()? != prior {
            bail!("startup rollback could not be verified; repair is required");
        }
        return Err(error);
    }
    result
}

pub fn update(
    path: &Path,
    registration: &impl Registration,
    expected: &RegistryValue,
    enabled: bool,
) -> (AutostartStatus, Option<Config>) {
    let attempt = (|| {
        let mut transaction = ConfigMutation::open(path)?;
        transaction.patch(ConfigPatch::Autostart(enabled))?;
        update_owned(registration, expected, enabled, || transaction.commit())
    })();
    match attempt {
        Ok(config) => (
            observation(config.app.autostart, registration, expected),
            Some(config),
        ),
        Err(_) => {
            let mut status = read(path, registration, expected).unwrap_or_else(|_| {
                AutostartStatus::unavailable(
                    false,
                    false,
                    "Stored startup preference could not be read.",
                )
            });
            status.error = Some("The startup setting could not be saved. Windows registration and the stored preference may differ; review the actual registration shown here before retrying.".into());
            (status, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Fake {
        value: RefCell<Option<RegistryValue>>,
        writes: RefCell<usize>,
    }
    impl Registration for Fake {
        fn read(&self) -> Result<Option<RegistryValue>> {
            Ok(self.value.borrow().clone())
        }
        fn write(&self, value: Option<&RegistryValue>) -> Result<()> {
            *self.writes.borrow_mut() += 1;
            *self.value.borrow_mut() = value.cloned();
            Ok(())
        }
    }
    #[test]
    fn legacy_true_and_missing_app_are_off_without_registry_writes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        let expected = RegistryValue::command("fixture");
        let registry = Fake::default();
        for text in [
            "",
            "[app]\nautostart = true\n",
            "[app]\nautostart = false\n",
        ] {
            std::fs::write(&path, text).unwrap();
            let status = read(&path, &registry, &expected).unwrap();
            assert_eq!(status.actual_registration, ActualRegistration::Absent);
            assert_eq!(*registry.writes.borrow(), 0);
        }
        let (status, _) = update(&path, &registry, &expected, true);
        assert_eq!(status.actual_registration, ActualRegistration::Matching);
        assert!(status.desired_autostart);
    }
    #[test]
    fn persistence_failure_rolls_back_only_the_value_this_operation_wrote() {
        let registry = Fake::default();
        let expected = RegistryValue::command("fixture");
        assert!(update_owned::<()>(&registry, &expected, true, || bail!("disk failure")).is_err());
        assert_eq!(registry.read().unwrap(), None);
        let foreign = RegistryValue::command("other app");
        assert!(
            update_owned::<()>(&registry, &expected, true, || {
                registry.write(Some(&foreign))?;
                bail!("disk failure")
            })
            .is_err()
        );
        assert_eq!(registry.read().unwrap(), Some(foreign));
    }
    #[test]
    fn foreign_entries_are_never_enabled_over_or_deleted_and_crash_gap_is_visible() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        let registry = Fake::default();
        let expected = RegistryValue::command("fixture");
        let foreign = RegistryValue::command("foreign");
        registry.write(Some(&foreign)).unwrap();
        for enabled in [true, false] {
            assert!(update(&path, &registry, &expected, enabled).1.is_none());
            assert_eq!(registry.read().unwrap(), Some(foreign.clone()));
        }
        crate::config::mutate(&path, ConfigPatch::Autostart(false)).unwrap();
        registry.write(Some(&expected)).unwrap(); // crash after registry, before TOML
        let observed = read(&path, &registry, &expected).unwrap();
        assert!(!observed.desired_autostart);
        assert_eq!(observed.actual_registration, ActualRegistration::Matching);
        let (status, _) = update(&path, &registry, &expected, false);
        assert_eq!(status.actual_registration, ActualRegistration::Absent);
    }
}
