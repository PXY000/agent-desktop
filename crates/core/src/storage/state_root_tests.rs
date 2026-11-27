use super::*;
use crate::refs::refs_test_support::TempDir;
use std::fs;

fn always_owned(_meta: &std::fs::Metadata) -> bool {
    true
}

#[test]
fn env_set_to_existing_absolute_dir_resolves_exactly() {
    let dir = TempDir::new();
    let resolved = resolve_state_root(
        false,
        Some(dir.path().to_path_buf()),
        Some(PathBuf::from("/should/not/be/used")),
        always_owned,
    )
    .expect("resolves");
    assert_eq!(resolved, dir.path());
}

#[test]
fn env_unset_resolves_to_home_fallback_joined_with_dotdir() {
    let home = PathBuf::from("/home/example-user");
    let resolved =
        resolve_state_root(false, None, Some(home.clone()), always_owned).expect("resolves");
    assert_eq!(resolved, home.join(".agent-desktop"));
}

#[test]
fn sandbox_blocked_home_falls_back_to_private_temp_state_root() {
    let temp = TempDir::new();
    let fallback = temp.path().join("sandbox-state");
    let home = PathBuf::from("/home/example-user");
    let inputs = StateRootInputs {
        override_active: false,
        env_value: None,
        home_fallback: Some(home),
        temp_fallback: Some(fallback.clone()),
    };
    let resolved = resolve_state_root_with_fallback(inputs, always_owned, |_| true)
        .expect("sandbox fallback resolves");
    assert_eq!(resolved, fallback);
    assert!(fallback.join("refstore.lock").exists());
}

#[test]
fn explicit_env_never_falls_back_when_sandbox_blocked() {
    let dir = TempDir::new();
    let temp = TempDir::new();
    let inputs = StateRootInputs {
        override_active: false,
        env_value: Some(dir.path().to_path_buf()),
        home_fallback: Some(PathBuf::from("/home/example-user")),
        temp_fallback: Some(temp.path().join("sandbox-state")),
    };
    let resolved = resolve_state_root_with_fallback(inputs, always_owned, |_| true)
        .expect("explicit env wins");
    assert_eq!(resolved, dir.path());
}

#[test]
fn relative_env_value_is_invalid_args() {
    let err = resolve_state_root(
        false,
        Some(PathBuf::from("relative/path")),
        Some(PathBuf::from("/home/example-user")),
        always_owned,
    )
    .expect_err("must reject relative path");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn empty_env_value_is_invalid_args() {
    let err = resolve_state_root(
        false,
        Some(PathBuf::from("")),
        Some(PathBuf::from("/home/example-user")),
        always_owned,
    )
    .expect_err("must reject empty path");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn regular_file_leaf_is_invalid_args() {
    let dir = TempDir::new();
    let file_path = dir.path().join("not-a-dir");
    fs::write(&file_path, b"x").expect("write file");

    let err = resolve_state_root(
        false,
        Some(file_path),
        Some(PathBuf::from("/home/example-user")),
        always_owned,
    )
    .expect_err("must reject regular file leaf");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn nonexistent_leaf_is_valid() {
    let dir = TempDir::new();
    let missing = dir.path().join("does-not-exist-yet");

    let resolved = resolve_state_root(
        false,
        Some(missing.clone()),
        Some(PathBuf::from("/home/example-user")),
        always_owned,
    )
    .expect("nonexistent leaf is valid, created lazily");
    assert_eq!(resolved, missing);
    assert!(!missing.exists());
}

#[test]
fn override_active_wins_over_env_value() {
    let dir = TempDir::new();
    let home = PathBuf::from("/home/example-user");
    let resolved = resolve_state_root(
        true,
        Some(dir.path().to_path_buf()),
        Some(home.clone()),
        always_owned,
    )
    .expect("resolves via home fallback, ignoring env");
    assert_eq!(resolved, home.join(".agent-desktop"));
}

#[test]
fn env_set_with_no_home_fallback_still_resolves() {
    let dir = TempDir::new();
    let resolved = resolve_state_root(false, Some(dir.path().to_path_buf()), None, always_owned)
        .expect("env path does not need HOME/USERPROFILE");
    assert_eq!(resolved, dir.path());
}

#[test]
fn leaf_owned_by_different_user_is_invalid_args() {
    let dir = TempDir::new();
    let err = resolve_state_root(
        false,
        Some(dir.path().to_path_buf()),
        Some(PathBuf::from("/home/example-user")),
        |_meta| false,
    )
    .expect_err("owner predicate reports mismatch");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn preflight_env_unset_is_a_no_op() {
    validate_state_root_env_value(false, None, always_owned).expect("unset env is a no-op");
}

#[test]
fn preflight_override_active_skips_env_validation() {
    validate_state_root_env_value(true, Some(PathBuf::from("relative/bad")), always_owned)
        .expect("test HOME override bypasses env validation, mirroring resolve_state_root");
}

#[test]
fn preflight_relative_env_value_is_invalid_args() {
    let err =
        validate_state_root_env_value(false, Some(PathBuf::from("relative/path")), always_owned)
            .expect_err("must reject relative path");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn preflight_empty_env_value_is_invalid_args() {
    let err = validate_state_root_env_value(false, Some(PathBuf::from("")), always_owned)
        .expect_err("must reject empty path");
    assert_eq!(err.code(), "INVALID_ARGS");
}

#[test]
fn preflight_nonexistent_absolute_leaf_does_not_require_home_and_is_valid() {
    let dir = TempDir::new();
    let missing = dir.path().join("does-not-exist-yet");

    validate_state_root_env_value(false, Some(missing.clone()), always_owned)
        .expect("nonexistent absolute leaf is valid without touching HOME");
    assert!(
        !missing.exists(),
        "preflight validation must not create the state root directory"
    );
}
