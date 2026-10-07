mod support;
use lark_user::{archive::Archive, storage::Store};
use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};
use support::*;

struct Harness {
    directory: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        Self {
            directory: private_tempdir(),
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lark-user"));
        command.args([
            "--store-dir",
            self.directory.path().join("store").to_str().unwrap(),
            "--key-file",
            self.directory.path().join("explicit.key").to_str().unwrap(),
            "--json",
        ]);
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn initialize(&self) {
        let key = self.run(&[
            "auth",
            "keygen",
            "--out",
            self.directory.path().join("explicit.key").to_str().unwrap(),
        ]);
        assert!(
            key.status.success(),
            "{}",
            String::from_utf8_lossy(&key.stderr)
        );
        let configured = self.run(&[
            "profile",
            "configure",
            "--web-url",
            "https://tenant.example.test/messenger",
            "--account-id",
            "synthetic-account-001",
            "--tenant-id",
            "synthetic-tenant-001",
        ]);
        assert!(
            configured.status.success(),
            "{}",
            String::from_utf8_lossy(&configured.stderr)
        );
    }
    fn store(&self) -> Store {
        Store::new(
            self.directory.path().join("store"),
            Some(self.directory.path().join("explicit.key")),
        )
    }
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
fn error_code(output: &Output) -> String {
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn cli_imports_from_stdin_reports_unverified_status_and_logs_out() {
    let harness = Harness::new();
    harness.initialize();
    let mut child = harness
        .command()
        .args(["auth", "import", "--cookie-file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("fixtures/playwright.json"))
        .unwrap();
    let imported = child.wait_with_output().unwrap();
    assert!(imported.status.success());
    assert_eq!(json_output(&imported)["status"], "imported_unverified");
    assert!(!String::from_utf8_lossy(&imported.stdout).contains("SYNTHETIC_COOKIE"));
    assert!(imported.stderr.is_empty());
    let status = harness.run(&["auth", "status"]);
    assert!(status.status.success());
    assert!(json_output(&status)["authenticated"].is_null());
    assert_eq!(json_output(&status)["live_verified"], false);
    assert!(harness.run(&["auth", "logout"]).status.success());
    let missing = harness.run(&["auth", "status"]);
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(error_code(&missing), "AUTH_MISSING");
}

#[test]
fn cli_expired_session_uses_stable_auth_exit_code_without_secret_output() {
    let harness = Harness::new();
    harness.initialize();
    let mut cookies: Value =
        serde_json::from_slice(include_bytes!("fixtures/playwright.json")).unwrap();
    for cookie in cookies.as_array_mut().unwrap() {
        cookie["expires"] = 10.into();
    }
    harness
        .store()
        .import_session("enterprise", &serde_json::to_vec(&cookies).unwrap(), 1)
        .unwrap();
    let status = harness.run(&["auth", "status"]);
    assert_eq!(status.status.code(), Some(3));
    assert_eq!(error_code(&status), "AUTH_EXPIRED");
    assert!(status.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&status.stderr).contains("SYNTHETIC_COOKIE"));
}

#[test]
fn cli_local_read_search_context_and_jsonl_export_are_real_process_workflows() {
    let harness = Harness::new();
    harness.initialize();
    let mut record = message("cli-message", "cli-chat", 0);
    record.sent_at = chrono::Utc::now();
    record.text = Some("Synthetic searchable commitment".into());
    let input = harness.directory.path().join("normalized.jsonl");
    private_write(&input, &serde_json::to_vec(&record).unwrap());
    let import = harness.run(&["archive", "import", "--file", input.to_str().unwrap()]);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    assert_eq!(json_output(&import)["inserted"], 1);
    let messages = harness.run(&[
        "messages", "list", "--chat", "cli-chat", "--local", "--limit", "1",
    ]);
    assert!(messages.status.success());
    assert_eq!(
        json_output(&messages)["items"][0]["message_id"],
        "cli-message"
    );
    assert_eq!(json_output(&messages)["source"], "local_archive");
    assert_eq!(json_output(&messages)["completeness"]["complete"], false);
    let search = harness.run(&["messages", "search", "searchable commitment", "--local"]);
    assert_eq!(json_output(&search)["items"].as_array().unwrap().len(), 1);
    let context = harness.run(&[
        "context",
        "--chat",
        "cli-chat",
        "--local",
        "--max-bytes",
        "4096",
    ]);
    assert!(
        context.status.success(),
        "{}",
        String::from_utf8_lossy(&context.stderr)
    );
    assert!(context.stdout.len() <= 4097);
    assert_eq!(
        json_output(&context)["messages"][0]["source"]["message_id"],
        "cli-message"
    );
    let out = harness.directory.path().join("export.jsonl");
    let export = harness.run(&[
        "export",
        "--chat",
        "cli-chat",
        "--local",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(export.status.success());
    let records = Archive::parse_jsonl(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message_id, "cli-message");
    let repeated = harness.run(&[
        "export",
        "--chat",
        "cli-chat",
        "--local",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(error_code(&repeated), "OUTPUT_ERROR");
    let stream = harness.run(&["export", "--chat", "cli-chat", "--local", "--out", "-"]);
    assert!(stream.status.success());
    assert_eq!(Archive::parse_jsonl(&stream.stdout).unwrap().len(), 1);
    let metadata: Value = serde_json::from_slice(&stream.stderr).unwrap();
    assert_eq!(metadata["export"]["exported_messages"], 1);
}

#[test]
fn default_online_commands_fail_and_dry_runs_are_explicitly_unsendable() {
    let harness = Harness::new();
    for args in [
        vec!["chats", "list"],
        vec!["messages", "list", "--chat", "approved-placeholder"],
        vec!["messages", "search", "query"],
        vec!["threads", "get", "thread-placeholder"],
        vec!["sync", "--chat", "chat-placeholder"],
        vec!["watch", "--chat", "chat-placeholder", "--jsonl"],
    ] {
        let output = harness.run(&args);
        assert_eq!(output.status.code(), Some(5));
        assert_eq!(error_code(&output), "PROTOCOL_UNVERIFIED");
        assert!(output.stdout.is_empty());
    }
    let draft = harness.directory.path().join("draft.txt");
    private_write(&draft, b"Synthetic draft; no message may be sent");
    let dry_run = harness.run(&[
        "messages",
        "send",
        "--chat",
        "chat-placeholder",
        "--text-file",
        draft.to_str().unwrap(),
        "--dry-run",
    ]);
    assert!(dry_run.status.success());
    assert_eq!(json_output(&dry_run)["sendable"], false);
    assert_eq!(json_output(&dry_run)["network_requests"], 0);
    assert_eq!(json_output(&dry_run)["automatic_retries"], 0);
    let send = harness.run(&[
        "messages",
        "send",
        "--chat",
        "chat-placeholder",
        "--text-file",
        draft.to_str().unwrap(),
    ]);
    assert_eq!(send.status.code(), Some(5));
    let raw_cookie_argument =
        harness.run(&["auth", "import", "--cookie", "SYNTHETIC_ARGUMENT_COOKIE"]);
    assert_eq!(raw_cookie_argument.status.code(), Some(2));
    assert!(
        !String::from_utf8_lossy(&raw_cookie_argument.stderr).contains("SYNTHETIC_ARGUMENT_COOKIE")
    );
}

#[test]
fn archive_clear_requires_explicit_flag_and_profile_rebinding_is_denied() {
    let harness = Harness::new();
    harness.initialize();
    let path = harness
        .store()
        .profile_dir("enterprise")
        .unwrap()
        .join("archive.sqlite");
    {
        let mut archive = Archive::open_or_create(&path, scope()).unwrap();
        archive
            .import(vec![message("a", "one", 0)], None, now())
            .unwrap();
    }
    let denied = harness.run(&["archive", "clear"]);
    assert_eq!(denied.status.code(), Some(2));
    let rebind = harness.run(&[
        "profile",
        "configure",
        "--tenant-id",
        "another-synthetic-tenant",
    ]);
    assert_eq!(error_code(&rebind), "SCOPE_CONFLICT");
    let cleared = harness.run(&["archive", "clear", "--yes"]);
    assert!(cleared.status.success());
    assert_eq!(json_output(&cleared)["local_messages_removed"], 1);
}
