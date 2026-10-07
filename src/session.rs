use crate::{Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use url::Url;
use zeroize::Zeroize;

pub const MAX_COOKIE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookie {
    pub name: String,
    value: String,
    pub domain: String,
    pub path: String,
    pub expires: f64,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<SameSite>,
    #[serde(flatten)]
    pub attributes: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Debug for BrowserCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BrowserCookie([REDACTED])")
    }
}

impl Drop for BrowserCookie {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl BrowserCookie {
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires >= 0.0 && self.expires <= now as f64
    }

    /// Matches ordinary cookies; partitioned cookies require a verified browsing context.
    pub fn matches(&self, url: &Url, now: i64) -> bool {
        if self.is_expired(now)
            || self.attributes.contains_key("partitionKey")
            || self
                .attributes
                .get("partitioned")
                .is_some_and(|v| v == &serde_json::Value::Bool(true))
        {
            return false;
        }
        if self.secure && url.scheme() != "https" {
            return false;
        }
        let Some(host) = url.host_str() else {
            return false;
        };
        let domain = self.domain.trim_start_matches('.');
        let host_match = host.eq_ignore_ascii_case(domain)
            || (self.domain.starts_with('.')
                && host
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{}", domain.to_ascii_lowercase())));
        let path = url.path();
        let path_match = path == self.path
            || (path.starts_with(&self.path)
                && (self.path.ends_with('/')
                    || path.as_bytes().get(self.path.len()) == Some(&b'/')));
        host_match && path_match
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let token = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
        if self.name.is_empty()
            || self.name.len() > 256
            || !self.name.bytes().all(token)
            || self.value.len() > 32768
            || !self
                .value
                .bytes()
                .all(|b| (0x21..=0x7e).contains(&b) && !b"\";,\\".contains(&b))
            || !self.path.starts_with('/')
            || self.path.len() > 4096
            || self.path.bytes().any(|b| b.is_ascii_control())
            || !self.expires.is_finite()
            || (self.expires < 0.0 && self.expires != -1.0)
        {
            return Err(Error::invalid(
                "Malformed browser cookie; all values are redacted",
            ));
        }
        let domain = self.domain.strip_prefix('.').unwrap_or(&self.domain);
        if domain.is_empty()
            || domain.starts_with('.')
            || domain.ends_with('.')
            || domain.contains(['/', '\\', ':', '@'])
            || !domain.is_ascii()
            || url::Host::parse(domain).is_err()
        {
            return Err(Error::invalid(
                "Invalid cookie domain; all values are redacted",
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub web_origin: String,
    pub imported_at: i64,
    pub cookies: Vec<BrowserCookie>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("cookie_count", &self.cookies.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Serialize)]
pub struct SessionStatus {
    pub status: &'static str,
    pub authenticated: Option<bool>,
    pub live_verified: bool,
    pub cookie_count: usize,
    pub expired_cookie_count: usize,
    pub cookie_domains: Vec<String>,
    pub imported_at: i64,
    pub web_origin: String,
    pub note: &'static str,
}

impl Session {
    pub fn import(bytes: &[u8], web_origin: &str, now: i64) -> Result<Self> {
        if bytes.len() > MAX_COOKIE_BYTES {
            return Err(Error::invalid("Cookie input exceeds 4 MiB"));
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Export {
            Cookies(Vec<BrowserCookie>),
            State { cookies: Vec<BrowserCookie> },
        }
        let export: Export = serde_json::from_slice(bytes).map_err(|_| Error::invalid("Expected a Playwright cookie array or storage-state object; parse details are redacted"))?;
        let cookies = match export {
            Export::Cookies(cookies) | Export::State { cookies } => cookies,
        };
        if cookies.is_empty() || cookies.len() > 4096 {
            return Err(Error::invalid(
                "Cookie export must contain 1 to 4096 cookies",
            ));
        }
        let mut identities = BTreeSet::new();
        for cookie in &cookies {
            cookie.validate()?;
            let partition = cookie
                .attributes
                .get("partitionKey")
                .map(serde_json::Value::to_string)
                .unwrap_or_default();
            if !identities.insert((
                cookie.name.clone(),
                cookie.domain.to_ascii_lowercase(),
                cookie.path.clone(),
                partition,
            )) {
                return Err(Error::invalid(
                    "Cookie export contains duplicate cookie identities",
                ));
            }
        }
        Ok(Self {
            web_origin: web_origin.to_owned(),
            imported_at: now,
            cookies,
        })
    }

    pub fn status(&self, now: i64) -> Result<SessionStatus> {
        let expired = self.cookies.iter().filter(|c| c.is_expired(now)).count();
        if expired == self.cookies.len() {
            return Err(Error::new(
                ErrorCode::AuthExpired,
                "All imported cookies have expired. Sign in with normal enterprise SSO/QR/2FA and import a fresh export",
            ));
        }
        Ok(SessionStatus {
            status: "imported_unverified",
            authenticated: None,
            live_verified: false,
            cookie_count: self.cookies.len(),
            expired_cookie_count: expired,
            cookie_domains: self
                .cookies
                .iter()
                .map(|c| c.domain.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            imported_at: self.imported_at,
            web_origin: self.web_origin.clone(),
            note: "Cookie metadata cannot establish a valid Lark session. International authentication and session updates are not verified",
        })
    }
}
