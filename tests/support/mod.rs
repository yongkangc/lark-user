#![allow(dead_code)]
use chrono::{DateTime, Duration, Utc};
use lark_user::{
    archive::Archive,
    model::{Message, Scope},
};
use std::{fs, path::Path};

pub fn scope() -> Scope {
    Scope {
        account_id: "synthetic-account-001".into(),
        tenant_id: "synthetic-tenant-001".into(),
    }
}
pub fn now() -> DateTime<Utc> {
    "2026-10-01T12:00:00Z".parse().unwrap()
}
pub fn message(id: &str, chat: &str, minute: i64) -> Message {
    let mut message = Archive::parse_jsonl(include_bytes!("../fixtures/normalized.jsonl"))
        .unwrap()
        .remove(0);
    message.message_id = id.into();
    message.chat_id = chat.into();
    message.sent_at = now() + Duration::minutes(minute);
    message.text = Some(format!("Synthetic content for {id}"));
    message
}

pub fn private_write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

pub fn private_tempdir() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().unwrap()
}
