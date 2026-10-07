mod support;
use lark_user::{
    ErrorCode,
    session::Session,
    storage::{Store, read_private_file},
};
use serde_json::json;
use support::*;

const COOKIES: &[u8] = include_bytes!("fixtures/playwright.json");

#[test]
fn preserves_playwright_attributes_and_redacts_debug_status() {
    let session =
        Session::import(COOKIES, "https://tenant.example.test", now().timestamp()).unwrap();
    let cookies = serde_json::to_value(&session.cookies).unwrap();
    assert_eq!(cookies[0]["expires"], 4102444800.5);
    assert_eq!(cookies[0]["priority"], "High");
    assert_eq!(cookies[1]["partitionKey"], "https://tenant.example.test");
    let debug = format!("{session:?} {:?}", session.cookies);
    let status = serde_json::to_string(&session.status(now().timestamp()).unwrap()).unwrap();
    assert!(!debug.contains("SYNTHETIC_COOKIE"));
    assert!(!status.contains("SYNTHETIC_COOKIE"));
    assert!(status.contains("imported_unverified"));
    assert!(status.contains("\"authenticated\":null"));
}

#[test]
fn accepts_storage_state_and_rejects_malformed_cookies_without_echo() {
    let cookies: serde_json::Value = serde_json::from_slice(COOKIES).unwrap();
    let state = serde_json::to_vec(&json!({"cookies":cookies,"origins":[{"origin":"https://tenant.example.test","localStorage":[{"name":"not_imported","value":"SYNTHETIC_LOCAL_STORAGE"}]}]})).unwrap();
    assert_eq!(
        Session::import(&state, "https://tenant.example.test", 0)
            .unwrap()
            .cookies
            .len(),
        2
    );
    let bad = br#"[{"name":"session","value":"SYNTHETIC_BAD_COOKIE","domain":42}]"#;
    let error = Session::import(bad, "https://tenant.example.test", 0).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert!(!error.to_string().contains("SYNTHETIC_BAD_COOKIE"));
}

#[test]
fn cookie_matching_respects_host_path_expiry_secure_and_partitioning() {
    let session = Session::import(COOKIES, "https://tenant.example.test", 0).unwrap();
    let cookie = &session.cookies[0];
    assert!(
        cookie.matches(
            &"https://child.tenant.example.test/messages"
                .parse()
                .unwrap(),
            0
        )
    );
    assert!(
        !cookie.matches(
            &"https://tenant.example.test.attacker.test/"
                .parse()
                .unwrap(),
            0
        )
    );
    assert!(!cookie.matches(&"http://tenant.example.test/".parse().unwrap(), 0));
    assert!(!cookie.matches(&"https://tenant.example.test/".parse().unwrap(), 4102444801));
    assert!(!session.cookies[1].matches(&"https://api.example.test/messages".parse().unwrap(), 0));
    let mut value: serde_json::Value = serde_json::from_slice(COOKIES).unwrap();
    value[0]["domain"] = "api.example.test".into();
    value[0]["path"] = "/messages".into();
    let session = Session::import(
        &serde_json::to_vec(&value).unwrap(),
        "https://tenant.example.test",
        0,
    )
    .unwrap();
    assert!(
        session.cookies[0].matches(&"https://api.example.test/messages/one".parse().unwrap(), 0)
    );
    assert!(!session.cookies[0].matches(
        &"https://api.example.test/messages-other".parse().unwrap(),
        0
    ));
    assert!(!session.cookies[0].matches(
        &"https://child.api.example.test/messages".parse().unwrap(),
        0
    ));
}

#[test]
fn returns_auth_expired_without_claiming_unexpired_cookies_are_authenticated() {
    let mut value: serde_json::Value = serde_json::from_slice(COOKIES).unwrap();
    for cookie in value.as_array_mut().unwrap() {
        cookie["expires"] = 10.into();
    }
    let session = Session::import(
        &serde_json::to_vec(&value).unwrap(),
        "https://tenant.example.test",
        1,
    )
    .unwrap();
    assert_eq!(session.status(11).unwrap_err().code, ErrorCode::AuthExpired);
    assert!(!session.status(9).unwrap().live_verified);
}

#[test]
fn vault_encrypts_persists_and_detects_tampering() {
    let directory = private_tempdir();
    let key = directory.path().join("explicit.key");
    Store::keygen(&key).unwrap();
    let store = Store::new(directory.path().join("store"), Some(key));
    store
        .configure(
            "enterprise",
            Some("https://tenant.example.test/messenger"),
            None,
            None,
            None,
        )
        .unwrap();
    store
        .import_session("enterprise", COOKIES, now().timestamp())
        .unwrap();
    let vault = store
        .profile_dir("enterprise")
        .unwrap()
        .join("session.vault");
    let mut bytes = std::fs::read(&vault).unwrap();
    assert!(
        !bytes
            .windows(b"SYNTHETIC_COOKIE_VALUE_A".len())
            .any(|v| v == b"SYNTHETIC_COOKIE_VALUE_A")
    );
    assert_eq!(
        serde_json::to_value(
            store
                .load_session("enterprise")
                .unwrap()
                .cookies
                .iter()
                .collect::<Vec<_>>()
        )
        .unwrap()[0]["priority"],
        "High"
    );
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    private_write(&vault, &bytes);
    assert_eq!(
        store.load_session("enterprise").unwrap_err().code,
        ErrorCode::StorageError
    );
    assert!(store.logout("enterprise").unwrap());
    assert!(!vault.exists());
    assert_eq!(
        store.load_session("enterprise").unwrap_err().code,
        ErrorCode::AuthMissing
    );
}

#[test]
fn vaults_are_bound_to_profile_and_scope_configuration_is_guarded() {
    let directory = private_tempdir();
    let key = directory.path().join("explicit.key");
    Store::keygen(&key).unwrap();
    let store = Store::new(directory.path().join("store"), Some(key));
    for name in ["one", "two"] {
        store
            .configure(name, Some("https://tenant.example.test"), None, None, None)
            .unwrap();
        store
            .import_session(name, COOKIES, now().timestamp())
            .unwrap();
    }
    let one = store.profile_dir("one").unwrap().join("session.vault");
    let two = store.profile_dir("two").unwrap().join("session.vault");
    private_write(&two, &std::fs::read(&one).unwrap());
    assert_eq!(
        store.load_session("two").unwrap_err().code,
        ErrorCode::StorageError
    );
    assert_eq!(
        store
            .configure("one", Some("https://other.example.test"), None, None, None)
            .unwrap_err()
            .code,
        ErrorCode::ScopeConflict
    );
    store
        .configure(
            "one",
            None,
            Some(scope().account_id),
            Some(scope().tenant_id),
            None,
        )
        .unwrap();
    assert_eq!(
        store
            .configure("one", None, Some("another-account".into()), None, None)
            .unwrap_err()
            .code,
        ErrorCode::ScopeConflict
    );
    assert!(store.profile_dir("../outside").is_err());
}

#[cfg(unix)]
#[test]
fn private_inputs_reject_readable_files_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = private_tempdir();
    let path = directory.path().join("input.json");
    std::fs::write(&path, COOKIES).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        read_private_file(&path, 4096).unwrap_err().code,
        ErrorCode::UnsafeFile
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = directory.path().join("linked.json");
    symlink(&path, &link).unwrap();
    assert_eq!(
        read_private_file(&link, 4096).unwrap_err().code,
        ErrorCode::UnsafeFile
    );
    assert!(read_private_file(&path, 4096).is_ok());
    assert_eq!(
        read_private_file(&path, 1).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
}
