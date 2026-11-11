//! Unit tests for the host-side token store.

use camino::Utf8Path;
use cap_std::ambient_authority;
use cap_std::fs::MetadataExt;
use cap_std::fs_utf8::Dir;
use mockable::MockEnv;
use rstest::{fixture, rstest};
use tempfile::TempDir;

use super::*;

#[fixture]
fn runtime_dir() -> TempDir {
    tempfile::tempdir().expect("temporary runtime directory should be created")
}

fn store(runtime_dir: &TempDir) -> TokenStore {
    let path = Utf8Path::from_path(runtime_dir.path()).expect("temp path should be UTF-8");
    TokenStore::new(path, "session-1").expect("token store should be created")
}

/// Open the temporary runtime directory through a capability handle.
fn open_runtime_dir(runtime_dir: &TempDir) -> Dir {
    let path = Utf8Path::from_path(runtime_dir.path()).expect("temp path should be UTF-8");
    Dir::open_ambient_dir(path, ambient_authority()).expect("runtime directory should open")
}

fn mode_of(dir: &Dir, path: &str) -> u32 {
    dir.metadata(path)
        .expect("metadata should be available")
        .mode()
        & 0o777
}

#[rstest]
fn write_creates_owner_only_directory_and_file(runtime_dir: TempDir) {
    let store = store(&runtime_dir);

    store
        .write_token("token-value")
        .expect("token should be written");

    let root = open_runtime_dir(&runtime_dir);
    assert_eq!(mode_of(&root, "podbot"), 0o700);
    assert_eq!(mode_of(&root, "podbot/session-1"), 0o700);
    assert_eq!(mode_of(&root, "podbot/session-1/ghapp_token"), 0o600);
}

#[rstest]
fn write_replaces_previous_token(runtime_dir: TempDir) {
    let store = store(&runtime_dir);

    store.write_token("first").expect("first write should work");
    store
        .write_token("second")
        .expect("second write should work");

    let root = open_runtime_dir(&runtime_dir);
    let contents = root
        .read_to_string("podbot/session-1/ghapp_token")
        .expect("token file should be readable");
    assert_eq!(contents, "second");
}

#[rstest]
fn cleanup_removes_session_directory_and_is_idempotent(runtime_dir: TempDir) {
    let store = store(&runtime_dir);
    store.write_token("token").expect("token should be written");

    store.cleanup().expect("first cleanup should succeed");
    assert!(!store.session_dir().exists());
    store.cleanup().expect("second cleanup should also succeed");
}

#[rstest]
#[case::dot(".")]
#[case::dot_dot("..")]
#[case::slash("session/../escape")]
#[case::blank("")]
fn unsafe_session_identifiers_are_rejected(#[case] session_id: &str, runtime_dir: TempDir) {
    let path = Utf8Path::from_path(runtime_dir.path()).expect("temp path should be UTF-8");
    let result = TokenStore::new(path, session_id);

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { ref field, .. }))
                if field == "session_id"
        ),
        "expected session_id rejection, got {result:?}"
    );
}

#[rstest]
fn missing_runtime_directory_fails_closed(runtime_dir: TempDir) {
    let missing = runtime_dir.path().join("does-not-exist");
    let path = Utf8Path::from_path(&missing).expect("path should be UTF-8");
    let store = TokenStore::new(path, "session-1").expect("store construction does not touch disk");

    let result = store.write_token("token");

    assert!(
        matches!(
            result,
            Err(PodbotError::GitHub(
                GitHubError::TokenStoreWriteFailed { .. }
            ))
        ),
        "expected write failure, got {result:?}"
    );
}

#[rstest]
fn from_env_uses_runtime_directory(runtime_dir: TempDir) {
    let mut env = MockEnv::new();
    let runtime_path = runtime_dir
        .path()
        .to_str()
        .expect("temp path should be UTF-8")
        .to_owned();
    env.expect_string()
        .withf(|name| name == XDG_RUNTIME_DIR)
        .returning(move |_| Some(runtime_path.clone()));

    let store = TokenStore::from_env(&env, "session-1").expect("store should resolve from env");

    assert_eq!(
        store.token_path(),
        Utf8Path::from_path(runtime_dir.path())
            .expect("temp path should be UTF-8")
            .join("podbot/session-1/ghapp_token")
    );
}

#[rstest]
fn from_env_requires_runtime_directory() {
    let mut env = MockEnv::new();
    env.expect_string().returning(|_| None);

    let result = TokenStore::from_env(&env, "session-1");

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == XDG_RUNTIME_DIR
        ),
        "expected XDG_RUNTIME_DIR rejection, got {result:?}"
    );
}
