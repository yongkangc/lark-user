//! International protocol adapters require account-specific live evidence before activation.

use crate::{Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use zeroize::Zeroize;

pub const MAX_HAR_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum Operation {
    Chats,
    Messages,
    Search,
    Threads,
    Sync,
    Send,
    Reply,
    Watch,
}

pub fn require_live(operation: Operation) -> Result<()> {
    let name = match operation {
        Operation::Chats => "Chat discovery",
        Operation::Messages => "Message reading",
        Operation::Search => "Server-side search",
        Operation::Threads => "Thread reading",
        Operation::Sync => "Incremental server sync",
        Operation::Send => "Message sending",
        Operation::Reply => "Message reply",
        Operation::Watch => "Live updates",
    };
    Err(Error::new(
        ErrorCode::ProtocolUnverified,
        format!(
            "{name} is blocked: the enterprise international Lark regional route, session-derived authentication and response schema have not been verified. Provide a local cookie export, exact signed-in web URL, local HAR and an approved read chat. No guessed endpoint, official API fallback or network request is used"
        ),
    ))
}

#[derive(Debug, Serialize)]
pub struct Capability {
    pub operation: &'static str,
    pub status: &'static str,
    pub evidence: &'static str,
}

pub fn capabilities() -> Vec<Capability> {
    vec![
        Capability {
            operation: "cookie_import_and_local_status",
            status: "available_local",
            evidence: "synthetic fixture and CLI tests; not live authentication proof",
        },
        Capability {
            operation: "local_archive_reads_search_export_context",
            status: "available_local",
            evidence: "synthetic SQLite, pagination and CLI tests; imported records only",
        },
        Capability {
            operation: "passive_har_inspection",
            status: "experimental_research",
            evidence: "header-name and origin extraction only; no protocol replay",
        },
        Capability {
            operation: "international_auth_chat_message_search_thread_sync",
            status: "blocked_unverified",
            evidence: "no authorized enterprise session supplied; Feishu sources do not verify Lark regions",
        },
        Capability {
            operation: "send_reply_watch",
            status: "blocked_unverified",
            evidence: "no verified international transport or separately approved send destination",
        },
    ]
}

#[derive(Deserialize)]
struct Har {
    log: HarLog,
}
#[derive(Deserialize)]
struct HarLog {
    entries: Vec<HarEntry>,
}
#[derive(Deserialize)]
struct HarEntry {
    request: HarRequest,
    response: HarResponse,
}
#[derive(Deserialize)]
struct HarRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<HarHeader>,
    #[serde(default)]
    cookies: Vec<HarCookie>,
}
#[derive(Deserialize)]
struct HarCookie {
    name: String,
}
#[derive(Deserialize)]
struct HarResponse {
    status: u16,
    #[serde(default)]
    headers: Vec<HarHeader>,
    #[serde(default)]
    content: HarContent,
}
#[derive(Default, Deserialize)]
struct HarContent {
    #[serde(default, rename = "mimeType")]
    mime_type: String,
}
#[derive(Deserialize)]
struct HarHeader {
    name: String,
    value: String,
}
impl Drop for HarHeader {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[derive(Debug, Serialize)]
pub struct HarObservation {
    pub origin: String,
    pub method: String,
    pub path_sha256: String,
    pub feishu_reference_path_candidate: Option<&'static str>,
    pub header_names: BTreeSet<String>,
    pub cookie_names: BTreeSet<String>,
    pub additional_session_header_names: BTreeSet<String>,
    pub numeric_command_id: Option<u32>,
    pub response_status: u16,
    pub response_mime_type: String,
    pub set_cookie_seen: bool,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct HarReport {
    pub origins: BTreeSet<String>,
    pub observations: Vec<HarObservation>,
    pub ignored_entries: usize,
    pub live_protocol_verified: bool,
    pub note: &'static str,
}

pub fn inspect_har(bytes: &[u8]) -> Result<HarReport> {
    if bytes.len() > MAX_HAR_BYTES {
        return Err(Error::invalid(
            "HAR exceeds 32 MiB; capture only the relevant read workflow",
        ));
    }
    let har: Har = serde_json::from_slice(bytes).map_err(|_| {
        Error::invalid("Malformed HAR; URLs, credentials and response contents are redacted")
    })?;
    if har.log.entries.len() > 10000 {
        return Err(Error::invalid("HAR is limited to 10000 entries"));
    }
    let mut report = HarReport {
        origins: BTreeSet::new(),
        observations: Vec::new(),
        ignored_entries: 0,
        live_protocol_verified: false,
        note: "Origins and fields were observed in a user-supplied HAR. Query strings, unknown paths, header values and bodies are omitted. Successful browser traffic does not prove direct cookie-only access, read safety, pagination, tenant identity or protobuf compatibility.",
    };
    for entry in har.log.entries {
        let Ok(url) = url::Url::parse(&entry.request.url) else {
            report.ignored_entries += 1;
            continue;
        };
        if !matches!(url.scheme(), "https" | "wss")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            report.ignored_entries += 1;
            continue;
        }
        let origin = url.origin().ascii_serialization();
        report.origins.insert(origin.clone());
        let header_names: BTreeSet<String> = entry
            .request
            .headers
            .iter()
            .filter_map(|h| safe_name(&h.name))
            .collect();
        let mut cookie_names: BTreeSet<String> = entry
            .request
            .cookies
            .iter()
            .filter_map(|c| safe_name(&c.name))
            .collect();
        for header in &entry.request.headers {
            if header.name.eq_ignore_ascii_case("cookie") {
                for part in header.value.split(';') {
                    if let Some((name, _)) = part.trim().split_once('=')
                        && let Some(name) = safe_name(name)
                    {
                        cookie_names.insert(name);
                    }
                }
            }
        }
        let numeric_command_id = entry
            .request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("x-command"))
            .and_then(|h| {
                if h.value.len() <= 9
                    && !h.value.is_empty()
                    && h.value.bytes().all(|b| b.is_ascii_digit())
                {
                    h.value.parse().ok()
                } else {
                    None
                }
            });
        let retry_after_seconds = entry
            .response
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("retry-after"))
            .and_then(|h| {
                if h.value.len() <= 9 && h.value.bytes().all(|b| b.is_ascii_digit()) {
                    h.value.parse().ok()
                } else {
                    None
                }
            });
        let reference = match url.path() {
            "/im/gateway/" => Some("/im/gateway/"),
            "/accounts/csrf" => Some("/accounts/csrf"),
            "/accounts/web/user" => Some("/accounts/web/user"),
            "/suite/passport/frontier_ticket/" => Some("/suite/passport/frontier_ticket/"),
            _ => None,
        };
        let method = if entry.request.method.bytes().all(|b| b.is_ascii_uppercase())
            && entry.request.method.len() <= 16
        {
            entry.request.method.clone()
        } else {
            "REDACTED".into()
        };
        let mime = entry
            .response
            .content
            .mime_type
            .split(';')
            .next()
            .unwrap_or("");
        let response_mime_type = if mime.len() <= 128
            && mime
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-.+".contains(&b))
        {
            mime.to_owned()
        } else {
            "REDACTED".into()
        };
        report.observations.push(HarObservation {
            origin,
            method,
            path_sha256: hex::encode(Sha256::digest(url.path().as_bytes())),
            feishu_reference_path_candidate: reference,
            additional_session_header_names: header_names
                .iter()
                .filter(|n| {
                    n.contains("csrf")
                        || n.contains("token")
                        || n.contains("device")
                        || n.as_str() == "authorization"
                })
                .cloned()
                .collect(),
            header_names,
            cookie_names,
            numeric_command_id,
            response_status: entry.response.status,
            response_mime_type,
            set_cookie_seen: entry
                .response
                .headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case("set-cookie")),
            retry_after_seconds,
        });
    }
    Ok(report)
}

fn safe_name(value: &str) -> Option<String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    {
        return None;
    }
    Some(value.to_ascii_lowercase())
}
