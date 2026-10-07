use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scope {
    pub account_id: String,
    pub tenant_id: String,
}

pub fn validate_id(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(Error::invalid(
            "IDs must contain 1 to 256 UTF-8 bytes without control characters",
        ));
    }
    Ok(())
}

impl Scope {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.account_id)?;
        validate_id(&self.tenant_id)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sender {
    pub id: Option<String>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub message_id: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Option<String>,
    pub name: Option<String>,
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub scope: Scope,
    pub chat_id: String,
    pub message_id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub sender: Sender,
    pub sent_at: DateTime<Utc>,
    #[serde(default)]
    pub edited_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub mentions: Vec<Mention>,
    #[serde(default)]
    pub quote: Option<Quote>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub message_type: String,
    #[serde(default)]
    pub structured_payload: Option<serde_json::Value>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_link_verified: bool,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceReference {
    pub scope: Scope,
    pub chat_id: String,
    pub message_id: String,
    pub thread_id: Option<String>,
    pub url: Option<String>,
    pub link_verified: bool,
}

impl Message {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        validate_id(&self.chat_id)?;
        validate_id(&self.message_id)?;
        if let Some(id) = &self.thread_id {
            validate_id(id)?;
        }
        if self.message_type.is_empty() || self.message_type.len() > 128 {
            return Err(Error::invalid(
                "Message type is required and bounded to 128 bytes",
            ));
        }
        if self.edited_at.is_some_and(|t| t < self.sent_at)
            || self.updated_at.is_some_and(|t| t < self.sent_at)
        {
            return Err(Error::invalid(
                "Edited/updated timestamps cannot precede the sent timestamp",
            ));
        }
        if let Some(value) = &self.source_url {
            let url = url::Url::parse(value).map_err(|_| Error::invalid("Invalid source URL"))?;
            if url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(Error::invalid(
                    "User-supplied source links must be HTTPS without credentials, queries or fragments; verified deep-link formats are not established",
                ));
            }
        }
        Ok(())
    }

    pub fn revision_ms(&self) -> i64 {
        self.sent_at
            .max(self.edited_at.unwrap_or(self.sent_at))
            .max(self.updated_at.unwrap_or(self.sent_at))
            .timestamp_millis()
    }

    pub fn source(&self) -> SourceReference {
        SourceReference {
            scope: self.scope.clone(),
            chat_id: self.chat_id.clone(),
            message_id: self.message_id.clone(),
            thread_id: self.thread_id.clone(),
            url: self.source_url.clone(),
            link_verified: self.source_link_verified,
        }
    }

    pub(crate) fn normalize_import(&mut self) {
        self.source_link_verified = false;
        if self.deleted {
            self.text = None;
            self.mentions.clear();
            self.quote = None;
            self.attachments.clear();
            self.structured_payload = None;
            self.source_url = None;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Chat {
    pub scope: Scope,
    pub chat_id: String,
    pub name: Option<String>,
    pub kind: &'static str,
    pub message_count: usize,
    pub last_message_at: DateTime<Utc>,
    pub discovery: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Completeness {
    pub complete: bool,
    pub reasons: Vec<String>,
}

impl Completeness {
    pub fn local() -> Self {
        Self {
            complete: false,
            reasons: vec![
                "local_archive_only".into(),
                "international_server_not_verified".into(),
                "history_coverage_unknown".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub source: &'static str,
    pub limit: usize,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub completeness: Completeness,
}
