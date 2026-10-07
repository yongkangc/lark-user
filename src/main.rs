#![forbid(unsafe_code)]

mod cli;

use chrono::{DateTime, Duration, Utc};
use clap::{Parser, error::ErrorKind};
use cli::*;
use lark_user::{
    Error, ErrorCode, Result,
    archive::{Archive, MAX_IMPORT_BYTES, MessageQuery},
    context,
    model::{Completeness, validate_id},
    protocol::{self, Operation},
    session::MAX_COOKIE_BYTES,
    storage::{Profile, ProfileLock, Store, read_private_file},
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use zeroize::Zeroizing;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(_) => {
            return report_error(Error::invalid(
                "Invalid command arguments; values are redacted. Run lark-user --help",
            ));
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(error),
    }
}

fn report_error(error: Error) -> ExitCode {
    let code = error.exit_code();
    if let Ok(bytes) = serde_json::to_vec(&json!({"error":error})) {
        let _ = std::io::stderr().write_all(&bytes);
        let _ = std::io::stderr().write_all(b"\n");
    }
    ExitCode::from(code)
}

fn run(cli: Cli) -> Result<()> {
    let root = match cli.store_dir {
        Some(root) => root,
        None => Store::default_root()?,
    };
    let store = Store::new(root, cli.key_file);
    store.profile_dir(&cli.profile)?;
    let now = Utc::now();
    match cli.command {
        Command::Capabilities => output(
            &json!({"international_lark_live_verified":false,"capabilities":protocol::capabilities()}),
            cli.json,
        ),
        Command::Profile { command } => match command {
            Profiles::Configure(config) => {
                output(&configure(&store, &cli.profile, config)?, cli.json)
            }
            Profiles::Show => output(&store.load_profile(&cli.profile)?, cli.json),
        },
        Command::Auth { command } => match command {
            Auth::Keygen { out } => {
                Store::keygen(&absolute(out)?)?;
                output(
                    &json!({"created":true,"key_bytes":32,"key_values":"redacted"}),
                    cli.json,
                )
            }
            Auth::Import {
                cookie_file,
                config,
            } => {
                let bytes = input(&cookie_file, MAX_COOKIE_BYTES)?;
                configure(&store, &cli.profile, config)?;
                output(
                    &store.import_session(&cli.profile, &bytes, now.timestamp())?,
                    cli.json,
                )
            }
            Auth::Status => output(
                &store.load_session(&cli.profile)?.status(now.timestamp())?,
                cli.json,
            ),
            Auth::Logout => output(
                &json!({"local_session_removed":store.logout(&cli.profile)?,"remote_session_revoked":false,"archive_deleted":false,"note":"Local credential deletion only; the browser session is unchanged"}),
                cli.json,
            ),
        },
        Command::Chats { command } => match command {
            Chats::List {
                local,
                limit,
                cursor,
            } => {
                live_unless_local(local, Operation::Chats)?;
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(&archive.chats(limit, cursor.as_deref())?, cli.json)
            }
            Chats::Get { chat_id, local } => {
                live_unless_local(local, Operation::Chats)?;
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &json!({"data":archive.get_chat(&chat_id)?,"source":"local_archive","completeness":Completeness::local()}),
                    cli.json,
                )
            }
        },
        Command::Messages { command } => match command {
            Messages::List { chat, query } => {
                live_unless_local(query.local, Operation::Messages)?;
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &archive.messages(&message_query(query, Some(chat), None, None)?)?,
                    cli.json,
                )
            }
            Messages::Get { message_id, local } => {
                live_unless_local(local, Operation::Messages)?;
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &json!({"data":archive.get_message(&message_id)?,"source":"local_archive","completeness":Completeness::local()}),
                    cli.json,
                )
            }
            Messages::Search {
                query_text,
                chat,
                query,
            } => {
                live_unless_local(query.local, Operation::Search)?;
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &archive.messages(&message_query(query, chat, None, Some(query_text))?)?,
                    cli.json,
                )
            }
            Messages::Send {
                chat,
                text_file,
                dry_run,
            } => draft(
                "send",
                &chat,
                &text_file,
                dry_run,
                Operation::Send,
                cli.json,
            ),
            Messages::Reply {
                message,
                text_file,
                dry_run,
            } => draft(
                "reply",
                &message,
                &text_file,
                dry_run,
                Operation::Reply,
                cli.json,
            ),
        },
        Command::Threads {
            command: Threads::Get { thread_id, query },
        } => {
            live_unless_local(query.local, Operation::Threads)?;
            let (_lock, _profile, archive) =
                open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
            output(
                &archive.messages(&message_query(query, None, Some(thread_id), None)?)?,
                cli.json,
            )
        }
        Command::Archive { command } => match command {
            Archives::Import { file } => {
                let messages = Archive::parse_jsonl(&input(&file, MAX_IMPORT_BYTES)?)?;
                let (_lock, profile, mut archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Initialize)?;
                let cutoff = now - Duration::days(profile.retention_days.into());
                output(&archive.import(messages, Some(cutoff), now)?, cli.json)
            }
            Archives::Coverage { chat } => {
                if let Some(id) = &chat {
                    validate_id(id)?;
                }
                let (_lock, _profile, archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &json!({"source":"local_archive","coverage":archive.coverage(chat.as_deref())?,"completeness":Completeness::local()}),
                    cli.json,
                )
            }
            Archives::Prune { before } => {
                let (_lock, profile, mut archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                let before = before
                    .map(|v| timestamp(&v))
                    .transpose()?
                    .unwrap_or(now - Duration::days(profile.retention_days.into()));
                output(
                    &json!({"local_messages_removed":archive.prune(before,now)?,"before":before,"server_messages_modified":false}),
                    cli.json,
                )
            }
            Archives::Clear { yes } => {
                if !yes {
                    return Err(Error::invalid(
                        "Archive clearing requires the explicit --yes flag",
                    ));
                }
                let (_lock, _profile, mut archive) =
                    open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
                output(
                    &json!({"local_messages_removed":archive.clear(now)?,"server_messages_modified":false}),
                    cli.json,
                )
            }
        },
        Command::Protocol {
            command: Protocol::Inspect { har },
        } => output(
            &protocol::inspect_har(&input(&har, protocol::MAX_HAR_BYTES)?)?,
            cli.json,
        ),
        Command::Export {
            chat,
            format: ExportFormat::Jsonl,
            out,
            query,
        } => {
            live_unless_local(query.local, Operation::Messages)?;
            let (_lock, _profile, archive) =
                open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
            let page = archive.messages(&message_query(query, Some(chat), None, None)?)?;
            let mut bytes = Zeroizing::new(Vec::new());
            for message in &page.items {
                serde_json::to_writer(&mut *bytes, message).map_err(|_| Error::storage())?;
                bytes.push(b'\n');
            }
            let summary = json!({"source":page.source,"exported_messages":page.items.len(),"has_more":page.has_more,"next_cursor":page.next_cursor,"completeness":page.completeness});
            if out == Path::new("-") {
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|_| output_error())?;
                let mut diagnostics = std::io::stderr().lock();
                serde_json::to_writer(&mut diagnostics, &json!({"export":summary}))
                    .map_err(|_| output_error())?;
                diagnostics.write_all(b"\n").map_err(|_| output_error())?;
                Ok(())
            } else {
                export_file(&absolute(out)?, &bytes)?;
                output(&summary, cli.json)
            }
        }
        Command::Context {
            chat,
            max_bytes,
            query,
        } => {
            live_unless_local(query.local, Operation::Messages)?;
            let (_lock, _profile, archive) =
                open_archive(&store, &cli.profile, ArchiveAccess::Existing)?;
            let bundle = context::generate(
                &archive,
                &message_query(query, chat, None, None)?,
                max_bytes,
                now,
            )?;
            // Compact serialization is part of the context byte budget.
            output(&bundle, true)
        }
        Command::Sync { .. } => protocol::require_live(Operation::Sync),
        Command::Watch { .. } => protocol::require_live(Operation::Watch),
    }
}

fn configure(store: &Store, name: &str, config: Configure) -> Result<Profile> {
    store.configure(
        name,
        config.web_url.as_deref(),
        config.account_id,
        config.tenant_id,
        config.retention_days,
    )
}

enum ArchiveAccess {
    Existing,
    Initialize,
}

fn open_archive(
    store: &Store,
    name: &str,
    access: ArchiveAccess,
) -> Result<(ProfileLock, Profile, Archive)> {
    let lock = store.lock(name)?;
    let profile = store.load_profile(name)?;
    let path = store.profile_dir(name)?.join("archive.sqlite");
    let archive = match access {
        ArchiveAccess::Existing => Archive::open_existing(&path, profile.scope()?),
        ArchiveAccess::Initialize => Archive::open_or_create(&path, profile.scope()?),
    }?;
    Ok((lock, profile, archive))
}

fn live_unless_local(local: bool, operation: Operation) -> Result<()> {
    if local {
        Ok(())
    } else {
        protocol::require_live(operation)
    }
}

fn message_query(
    query: Query,
    chat: Option<String>,
    thread: Option<String>,
    search: Option<String>,
) -> Result<MessageQuery> {
    Ok(MessageQuery {
        chat,
        thread,
        since: query.since.map(|s| timestamp(&s)).transpose()?,
        search,
        limit: query.limit,
        cursor: query.cursor,
    })
}

fn timestamp(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(seconds) = value.parse::<i64>() {
        DateTime::from_timestamp(seconds, 0)
            .ok_or_else(|| Error::invalid("Unix timestamp is out of range"))
    } else {
        DateTime::parse_from_rfc3339(value)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|_| Error::invalid("Timestamp must be RFC3339 or Unix seconds"))
    }
}

fn input(path: &Path, max: usize) -> Result<Zeroizing<Vec<u8>>> {
    if path != Path::new("-") {
        return read_private_file(path, max);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::storage())?;
    if bytes.len() > max {
        return Err(Error::invalid("Stdin exceeds the documented size limit"));
    }
    Ok(bytes)
}

fn draft(
    operation: &str,
    destination: &str,
    text_file: &Path,
    dry_run: bool,
    live: Operation,
    compact: bool,
) -> Result<()> {
    if !dry_run {
        return protocol::require_live(live);
    }
    validate_id(destination)?;
    let text = input(text_file, 65536)?;
    if text.is_empty() || std::str::from_utf8(&text).is_err() {
        return Err(Error::invalid("Draft must be nonempty UTF-8 up to 64 KiB"));
    }
    output(
        &json!({"dry_run":true,"operation":operation,"destination_id":destination,"text_bytes":text.len(),"draft_sha256":hex::encode(Sha256::digest(&text)),"sendable":false,"reason":"international_protocol_unverified","network_requests":0,"automatic_retries":0,"note":"Intent preview only; destination, permissions, wire format and delivery are not validated"}),
        compact,
    )
}

fn output<T: Serialize>(value: &T, compact: bool) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    if compact {
        serde_json::to_writer(&mut stdout, value)
    } else {
        serde_json::to_writer_pretty(&mut stdout, value)
    }
    .map_err(|_| output_error())?;
    stdout.write_all(b"\n").map_err(|_| output_error())
}

fn output_error() -> Error {
    Error::new(ErrorCode::OutputError, "Output could not be written")
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|d| d.join(path))
            .map_err(|_| Error::storage())
    }
}

fn export_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(Error::storage)?;
    let metadata = parent.symlink_metadata().map_err(|_| Error::storage())?;
    if !metadata.is_dir() {
        return Err(Error::new(
            ErrorCode::UnsafeFile,
            "Export directory must be a real directory",
        ));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::storage())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::storage())?;
    }
    temporary.write_all(bytes).map_err(|_| Error::storage())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| Error::storage())?;
    temporary.persist_noclobber(path).map_err(|_| Error::new(ErrorCode::OutputError,"Export output already exists or could not be created; existing files were not replaced"))?;
    Ok(())
}
