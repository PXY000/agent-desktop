use crate::AppError;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const AGENT_DESKTOP_HOME_ENV: &str = "AGENT_DESKTOP_HOME";
const STATE_DIR_NAME: &str = ".agent-desktop";
const LOCK_PROBE_NAME: &str = "refstore.lock";

pub(crate) struct StateRootInputs {
    pub override_active: bool,
    pub env_value: Option<PathBuf>,
    pub home_fallback: Option<PathBuf>,
    pub temp_fallback: Option<PathBuf>,
}

/// Binary-level preflight: validates `AGENT_DESKTOP_HOME` without touching
/// `HOME`/`USERPROFILE`. A no-op when the variable is unset, so legacy
/// behavior (surfacing a missing-HOME error later, at first state touch)
/// stays byte-identical to today.
pub fn validate_state_root_env() -> Result<(), AppError> {
    let override_active = crate::refs::home_override_active();
    let env_value = std::env::var_os(AGENT_DESKTOP_HOME_ENV).map(PathBuf::from);
    validate_state_root_env_value(override_active, env_value, is_owned_by_current_user)
}

/// Resolves the private temporary state root directory when `$HOME/.agent-desktop`
/// is inaccessible (for example inside a read-only `$HOME` sandbox).
pub fn fallback_temp_state_root() -> PathBuf {
    default_temp_state_root()
}

pub(crate) fn validate_state_root_env_value(
    override_active: bool,
    env_value: Option<PathBuf>,
    owned_by_current_user: impl Fn(&std::fs::Metadata) -> bool,
) -> Result<(), AppError> {
    if override_active {
        return Ok(());
    }
    match env_value {
        Some(path) => validate_env_root(&path, &owned_by_current_user),
        None => Ok(()),
    }
}

pub(crate) fn resolve_configured_state_root() -> Result<PathBuf, AppError> {
    let inputs = StateRootInputs {
        override_active: crate::refs::home_override_active(),
        env_value: std::env::var_os(AGENT_DESKTOP_HOME_ENV).map(PathBuf::from),
        home_fallback: crate::refs::home_dir(),
        temp_fallback: Some(default_temp_state_root()),
    };
    resolve_state_root_with_fallback(inputs, is_owned_by_current_user, is_sandbox_blocked)
}

#[cfg(test)]
pub(crate) fn resolve_state_root(
    override_active: bool,
    env_value: Option<PathBuf>,
    home_fallback: Option<PathBuf>,
    owned_by_current_user: impl Fn(&std::fs::Metadata) -> bool,
) -> Result<PathBuf, AppError> {
    let inputs = StateRootInputs {
        override_active,
        env_value,
        home_fallback,
        temp_fallback: None,
    };
    resolve_state_root_with_fallback(inputs, owned_by_current_user, |_| false)
}

pub(crate) fn resolve_state_root_with_fallback(
    inputs: StateRootInputs,
    owned_by_current_user: impl Fn(&std::fs::Metadata) -> bool,
    sandbox_blocked: impl Fn(&Path) -> bool,
) -> Result<PathBuf, AppError> {
    if !inputs.override_active
        && let Some(env_path) = inputs.env_value
    {
        validate_env_root(&env_path, &owned_by_current_user)?;
        return Ok(env_path);
    }
    if let Some(home) = inputs.home_fallback {
        let primary = home.join(STATE_DIR_NAME);
        if !inputs.override_active
            && sandbox_blocked(&primary)
            && let Some(fallback) = inputs.temp_fallback
            && prepare_private_state_root(&fallback).is_ok()
        {
            return Ok(fallback);
        }
        return Ok(primary);
    }
    if !inputs.override_active
        && let Some(fallback) = inputs.temp_fallback
        && prepare_private_state_root(&fallback).is_ok()
    {
        return Ok(fallback);
    }
    Err(AppError::Internal("HOME directory not found".into()))
}

fn is_sandbox_blocked(primary: &Path) -> bool {
    let Some(parent) = primary.parent() else {
        return false;
    };
    if !parent.exists() {
        return false;
    }
    match prepare_private_state_root(primary) {
        Ok(()) => false,
        Err(err) => matches!(
            err.kind(),
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
        ),
    }
}

fn prepare_private_state_root(root: &Path) -> std::io::Result<()> {
    crate::storage::private_file_parent::ensure_private(root)?;
    let probe = root.join(LOCK_PROBE_NAME);
    let _ = crate::storage::private_file::open_private_lock(&probe, true)?;
    Ok(())
}

#[cfg(unix)]
fn default_temp_state_root() -> PathBuf {
    let uid = unsafe { libc::geteuid() };
    std::env::temp_dir().join(format!("agent-desktop-{uid}"))
}

#[cfg(not(unix))]
fn default_temp_state_root() -> PathBuf {
    let suffix = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|raw| {
            raw.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());
    match suffix {
        Some(user) => std::env::temp_dir().join(format!("agent-desktop-{user}")),
        None => std::env::temp_dir().join("agent-desktop"),
    }
}

fn validate_env_root(
    path: &Path,
    owned_by_current_user: &dyn Fn(&std::fs::Metadata) -> bool,
) -> Result<(), AppError> {
    if path.as_os_str().is_empty() {
        return Err(AppError::invalid_input_with_suggestion(
            "AGENT_DESKTOP_HOME must not be empty",
            "Unset the variable or set it to an absolute directory path.",
        ));
    }
    if !path.is_absolute() {
        return Err(AppError::invalid_input_with_suggestion(
            format!(
                "AGENT_DESKTOP_HOME must be an absolute path, got '{}'",
                path.display()
            ),
            "Set AGENT_DESKTOP_HOME to an absolute directory path.",
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME must not be a symlink: '{}'",
                        path.display()
                    ),
                    "Point AGENT_DESKTOP_HOME at a real directory, not a symlink.",
                ));
            }
            if !meta.is_dir() {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME must be a directory: '{}'",
                        path.display()
                    ),
                    "Remove the file at this path or choose a different AGENT_DESKTOP_HOME.",
                ));
            }
            if !owned_by_current_user(&meta) {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME is owned by a different user: '{}'",
                        path.display()
                    ),
                    "Choose a directory you own for AGENT_DESKTOP_HOME.",
                ));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::invalid_input_with_suggestion(
            format!(
                "Cannot access AGENT_DESKTOP_HOME '{}': {err}",
                path.display()
            ),
            "Check permissions on the parent directory or choose a different path.",
        )),
    }
}

#[cfg(unix)]
pub(crate) fn is_owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.uid() == unsafe { libc::getuid() }
}

#[cfg(not(unix))]
pub(crate) fn is_owned_by_current_user(_meta: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
#[path = "state_root_tests.rs"]
mod tests;
