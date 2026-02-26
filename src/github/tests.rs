//! Unit tests for GitHub App private key loading.
//!
//! Covers happy paths (valid RSA key), unhappy paths (missing, empty, invalid),
//! key type rejection (ECDSA, Ed25519, public keys, certificates), and
//! encrypted key detection.

use super::*;
use cap_std::fs_utf8::Dir as Utf8Dir;
use rstest::{fixture, rstest};
use tempfile::TempDir;

/// Fixture providing valid RSA PEM content (PKCS#1 format).
#[fixture]
fn valid_rsa_pem() -> String {
    include_str!("../../tests/fixtures/test_rsa_private_key.pem").to_owned()
}

/// Fixture providing an ECDSA PEM key (SEC 1 format).
#[fixture]
fn ec_pem() -> String {
    include_str!("../../tests/fixtures/test_ec_private_key.pem").to_owned()
}

/// Fixture providing an Ed25519 PEM key (PKCS#8 format).
#[fixture]
fn ed25519_pem() -> String {
    include_str!("../../tests/fixtures/test_ed25519_private_key.pem").to_owned()
}

/// Fixture providing a temporary directory opened as a `Dir` capability.
#[fixture]
fn temp_key_dir() -> (TempDir, Utf8Dir) {
    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let path_str = temp_dir
        .path()
        .to_str()
        .expect("temp dir path should be UTF-8");
    let dir =
        Utf8Dir::open_ambient_dir(path_str, ambient_authority()).expect("should open temp dir");
    (temp_dir, dir)
}

#[rstest]
fn load_valid_rsa_key_succeeds(valid_rsa_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("key.pem", &valid_rsa_pem)
        .expect("should write key");
    let path = Utf8Path::new("/display/key.pem");
    let result = load_private_key_from_dir(&dir, "key.pem", path);
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[rstest]
fn load_missing_file_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let path = Utf8Path::new("/config/missing.pem");
    let result = load_private_key_from_dir(&dir, "missing.pem", path);
    assert!(result.is_err(), "expected Err for missing file");
    let error = result.as_ref().err();
    let message = format!("{error:?}");
    assert!(
        message.contains("failed to read file"),
        "error should mention file read failure: {message}"
    );
}

#[rstest]
fn load_empty_file_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("empty.pem", "").expect("should write empty file");
    let path = Utf8Path::new("/config/empty.pem");
    let result = load_private_key_from_dir(&dir, "empty.pem", path);
    assert!(result.is_err(), "expected Err for empty file");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("empty"),
        "error should mention empty file: {message}"
    );
}

#[rstest]
fn load_invalid_pem_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("garbage.pem", "this is not a PEM file at all")
        .expect("should write garbage file");
    let path = Utf8Path::new("/config/garbage.pem");
    let result = load_private_key_from_dir(&dir, "garbage.pem", path);
    assert!(result.is_err(), "expected Err for invalid PEM");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("invalid RSA private key"),
        "error should mention invalid RSA key: {message}"
    );
}

#[rstest]
fn load_ec_key_returns_clear_error(ec_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("ec.pem", &ec_pem).expect("should write EC key");
    let path = Utf8Path::new("/config/ec.pem");
    let result = load_private_key_from_dir(&dir, "ec.pem", path);
    assert!(result.is_err(), "expected Err for ECDSA key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("ECDSA"),
        "error should mention ECDSA: {message}"
    );
    assert!(
        message.contains("RSA"),
        "error should mention RSA requirement: {message}"
    );
}

#[rstest]
fn load_ed25519_key_returns_clear_error(ed25519_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("ed25519.pem", &ed25519_pem)
        .expect("should write Ed25519 key");
    let path = Utf8Path::new("/config/ed25519.pem");
    let result = load_private_key_from_dir(&dir, "ed25519.pem", path);
    assert!(result.is_err(), "expected Err for Ed25519 key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("invalid RSA private key"),
        "Ed25519 PKCS#8 should fail RSA parse: {message}"
    );
}

#[rstest]
fn error_includes_file_path(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let display = Utf8Path::new("/home/user/.config/podbot/app.pem");
    let result = load_private_key_from_dir(&dir, "nonexistent.pem", display);
    match result {
        Err(GitHubError::PrivateKeyLoadFailed { ref path, .. }) => {
            assert_eq!(
                path.to_str(),
                Some("/home/user/.config/podbot/app.pem"),
                "error path should match display path"
            );
        }
        other => panic!("expected PrivateKeyLoadFailed, got: {other:?}"),
    }
}

#[rstest]
fn load_private_key_resolves_full_path(valid_rsa_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
    let (tmp, dir) = temp_key_dir;
    dir.write("github-app.pem", &valid_rsa_pem)
        .expect("should write key");
    let full_path = tmp.path().join("github-app.pem");
    let utf8_path = Utf8Path::from_path(&full_path).expect("temp path should be UTF-8");
    let result = load_private_key(utf8_path);
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[rstest]
fn load_private_key_missing_parent_returns_error() {
    let path = Utf8Path::new("/nonexistent/directory/key.pem");
    let result = load_private_key(path);
    assert!(result.is_err(), "expected Err for missing parent");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("failed to open parent directory"),
        "error should mention parent directory: {message}"
    );
}

#[rstest]
fn load_public_key_returns_clear_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let public_pem = concat!(
        "-----BEGIN PUBLIC KEY-----\n",
        "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE\n",
        "-----END PUBLIC KEY-----\n"
    );
    dir.write("pub.pem", public_pem)
        .expect("should write public key");
    let path = Utf8Path::new("/config/pub.pem");
    let result = load_private_key_from_dir(&dir, "pub.pem", path);
    assert!(result.is_err(), "expected Err for public key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("public key"),
        "error should mention public key: {message}"
    );
}

#[rstest]
fn load_rsa_public_key_returns_clear_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let rsa_pub_pem = concat!(
        "-----BEGIN RSA PUBLIC KEY-----\n",
        "MIIBCgKCAQEA4LAdQBFm\n",
        "-----END RSA PUBLIC KEY-----\n"
    );
    dir.write("rsa_pub.pem", rsa_pub_pem)
        .expect("should write RSA public key");
    let path = Utf8Path::new("/config/rsa_pub.pem");
    let result = load_private_key_from_dir(&dir, "rsa_pub.pem", path);
    assert!(result.is_err(), "expected Err for RSA public key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("public key"),
        "error should mention public key: {message}"
    );
}

#[rstest]
fn load_certificate_returns_clear_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let cert_pem = concat!(
        "-----BEGIN CERTIFICATE-----\n",
        "MIICGzCCAaGgAwIBAgIBADAK\n",
        "-----END CERTIFICATE-----\n"
    );
    dir.write("cert.pem", cert_pem)
        .expect("should write certificate");
    let path = Utf8Path::new("/config/cert.pem");
    let result = load_private_key_from_dir(&dir, "cert.pem", path);
    assert!(result.is_err(), "expected Err for certificate");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("certificate"),
        "error should mention certificate: {message}"
    );
}

#[rstest]
fn load_encrypted_pkcs8_key_returns_clear_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let enc_pem = concat!(
        "-----BEGIN ENCRYPTED PRIVATE KEY-----\n",
        "MIIFHDBOBgkqhkiG9w0BBQ0w\n",
        "-----END ENCRYPTED PRIVATE KEY-----\n"
    );
    dir.write("encrypted.pem", enc_pem)
        .expect("should write encrypted key");
    let path = Utf8Path::new("/config/encrypted.pem");
    let result = load_private_key_from_dir(&dir, "encrypted.pem", path);
    assert!(result.is_err(), "expected Err for encrypted key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("encrypted"),
        "error should mention encryption: {message}"
    );
}

#[rstest]
fn load_legacy_encrypted_key_returns_clear_error(temp_key_dir: (TempDir, Utf8Dir)) {
    let (_tmp, dir) = temp_key_dir;
    let legacy_enc_pem = concat!(
        "-----BEGIN RSA PRIVATE KEY-----\n",
        "Proc-Type: 4,ENCRYPTED\n",
        "DEK-Info: AES-256-CBC,AABBCCDD\n",
        "\n",
        "MIIBCgKCAQEA4LAdQBFm\n",
        "-----END RSA PRIVATE KEY-----\n"
    );
    dir.write("legacy_enc.pem", legacy_enc_pem)
        .expect("should write legacy encrypted key");
    let path = Utf8Path::new("/config/legacy_enc.pem");
    let result = load_private_key_from_dir(&dir, "legacy_enc.pem", path);
    assert!(result.is_err(), "expected Err for legacy encrypted key");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("encrypted"),
        "error should mention encryption: {message}"
    );
}
