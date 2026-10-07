use crate::{
    Error, ErrorCode, Result,
    model::Scope,
    private_files,
    session::{MAX_COOKIE_BYTES, Session},
};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng, Payload, rand_core::RngCore},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
use url::Url;
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"LARKUSER\x01";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub web_origin: String,
    pub account_id: Option<String>,
    pub tenant_id: Option<String>,
    pub identity_verification: String,
    pub retention_days: u32,
}

impl Profile {
    pub fn scope(&self) -> Result<Scope> {
        let scope = Scope {
            account_id: self.account_id.clone().ok_or_else(|| {
                Error::invalid("Set the profile's declared account ID before using an archive")
            })?,
            tenant_id: self.tenant_id.clone().ok_or_else(|| {
                Error::invalid("Set the profile's declared tenant ID before using an archive")
            })?,
        };
        scope.validate()?;
        Ok(scope)
    }
}

pub struct Store {
    root: PathBuf,
    key_file: Option<PathBuf>,
}

pub struct ProfileLock {
    _file: File,
}

impl Store {
    pub fn new(root: PathBuf, key_file: Option<PathBuf>) -> Self {
        Self { root, key_file }
    }

    pub fn default_root() -> Result<PathBuf> {
        directories::ProjectDirs::from("", "", "lark-user")
            .map(|d| d.data_local_dir().to_path_buf())
            .ok_or_else(|| Error::invalid("Specify --store-dir on this machine"))
    }

    pub fn profile_dir(&self, name: &str) -> Result<PathBuf> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(Error::invalid(
                "Profile names use 1 to 64 ASCII letters, digits, underscores or hyphens",
            ));
        }
        Ok(self.root.join("profiles").join(name))
    }

    pub fn lock(&self, name: &str) -> Result<ProfileLock> {
        private_files::directory(&self.root)?;
        private_files::directory(&self.root.join("profiles"))?;
        let directory = self.profile_dir(name)?;
        private_files::directory(&directory)?;
        let file = private_files::open(&directory.join(".lock"), true)?;
        file.lock_exclusive().map_err(|_| Error::storage())?;
        Ok(ProfileLock { _file: file })
    }

    pub fn load_profile(&self, name: &str) -> Result<Profile> {
        let path = self.profile_dir(name)?.join("profile.json");
        if !path.exists() {
            return Err(Error::new(
                ErrorCode::NotFound,
                "Profile not found. Use auth import --web-url or profile configure first",
            ));
        }
        let bytes = private_files::read(&path, 65536)?;
        let profile: Profile = serde_json::from_slice(&bytes).map_err(|_| Error::storage())?;
        origin(&profile.web_origin)?;
        if !(1..=3650).contains(&profile.retention_days) {
            return Err(Error::storage());
        }
        Ok(profile)
    }

    pub fn configure(
        &self,
        name: &str,
        web_url: Option<&str>,
        account_id: Option<String>,
        tenant_id: Option<String>,
        retention_days: Option<u32>,
    ) -> Result<Profile> {
        let _lock = self.lock(name)?;
        let directory = self.profile_dir(name)?;
        let existing = if directory.join("profile.json").exists() {
            Some(self.load_profile(name)?)
        } else {
            None
        };
        let web_origin = match web_url {
            Some(value) => origin(value)?,
            None => existing.as_ref().map(|p| p.web_origin.clone()).ok_or_else(|| Error::invalid("First import/configuration requires --web-url from the signed-in browser; no default domain is guessed"))?,
        };
        let profile = Profile {
            web_origin,
            account_id: account_id.or_else(|| existing.as_ref().and_then(|p| p.account_id.clone())),
            tenant_id: tenant_id.or_else(|| existing.as_ref().and_then(|p| p.tenant_id.clone())),
            identity_verification: "user_declared_unverified".to_owned(),
            retention_days: retention_days
                .or_else(|| existing.as_ref().map(|p| p.retention_days))
                .unwrap_or(30),
        };
        if !(1..=3650).contains(&profile.retention_days) {
            return Err(Error::invalid("Retention must be 1 to 3650 days"));
        }
        for id in [&profile.account_id, &profile.tenant_id]
            .into_iter()
            .flatten()
        {
            crate::model::validate_id(id)?;
        }
        if let Some(previous) = existing {
            let identity_changed = previous.account_id != profile.account_id
                || previous.tenant_id != profile.tenant_id;
            let declared_identity_replaced = (previous.account_id.is_some()
                && previous.account_id != profile.account_id)
                || (previous.tenant_id.is_some() && previous.tenant_id != profile.tenant_id);
            let origin_changed = previous.web_origin != profile.web_origin;
            if (directory.join("archive.sqlite").exists() && (identity_changed || origin_changed))
                || (directory.join("session.vault").exists()
                    && (declared_identity_replaced || origin_changed))
            {
                return Err(Error::new(
                    ErrorCode::ScopeConflict,
                    "Create a separate profile for another account, tenant or origin; existing credentials/archives cannot be rebound",
                ));
            }
        }
        private_files::write(
            &directory.join("profile.json"),
            &serde_json::to_vec(&profile).map_err(|_| Error::storage())?,
        )?;
        Ok(profile)
    }

    fn key_identity(&self, name: &str) -> Result<String> {
        let root = self.root.canonicalize().map_err(|_| Error::storage())?;
        Ok(format!(
            "{}:{name}",
            hex::encode(Sha256::digest(root.as_os_str().as_encoded_bytes()))
        ))
    }

    fn key(&self, name: &str, create: bool) -> Result<Zeroizing<Vec<u8>>> {
        let bytes = if let Some(path) = &self.key_file {
            private_files::read(path, 32)?
        } else {
            let entry = keyring::Entry::new("lark-user-v1", &self.key_identity(name)?)
                .map_err(|_| credential_error())?;
            match entry.get_secret() {
                Ok(key) => Zeroizing::new(key),
                Err(keyring::Error::NoEntry) if create => {
                    let mut key = Zeroizing::new(vec![0u8; 32]);
                    OsRng.fill_bytes(&mut key);
                    entry.set_secret(&key).map_err(|_| credential_error())?;
                    key
                }
                Err(_) => return Err(credential_error()),
            }
        };
        if bytes.len() != 32 {
            return Err(Error::invalid(
                "The explicit key file must contain exactly 32 random bytes",
            ));
        }
        Ok(bytes)
    }

    pub fn keygen(path: &Path) -> Result<()> {
        let mut key = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(key.as_mut());
        private_files::write_new(path, key.as_ref())
    }

    pub fn import_session(
        &self,
        name: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<crate::session::SessionStatus> {
        let _lock = self.lock(name)?;
        let profile = self.load_profile(name)?;
        let session = Session::import(bytes, &profile.web_origin, now)?;
        let status = session.status(now)?;
        let path = self.profile_dir(name)?.join("session.vault");
        let key = self.key(name, !path.exists())?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| Error::storage())?;
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let plain = Zeroizing::new(serde_json::to_vec(&session).map_err(|_| Error::storage())?);
        let aad = self.key_identity(name)?;
        let encrypted = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| Error::storage())?;
        let mut envelope = Vec::from(MAGIC);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&encrypted);
        private_files::write(&path, &envelope)?;
        Ok(status)
    }

    pub fn load_session(&self, name: &str) -> Result<Session> {
        let _lock = self.lock(name)?;
        let profile = self.load_profile(name)?;
        let path = self.profile_dir(name)?.join("session.vault");
        if !path.exists() {
            return Err(Error::new(
                ErrorCode::AuthMissing,
                "No imported session. Sign in normally and use auth import with a private cookie file or stdin",
            ));
        }
        let envelope = private_files::read(&path, MAX_COOKIE_BYTES * 2)?;
        if envelope.len() < MAGIC.len() + 40 || !envelope.starts_with(MAGIC) {
            return Err(Error::storage());
        }
        let key = self.key(name, false)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| Error::storage())?;
        let aad = self.key_identity(name)?;
        let nonce = XNonce::from_slice(&envelope[MAGIC.len()..MAGIC.len() + 24]);
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: &envelope[MAGIC.len() + 24..],
                        aad: aad.as_bytes(),
                    },
                )
                .map_err(|_| Error::storage())?,
        );
        let session: Session = serde_json::from_slice(&plain).map_err(|_| Error::storage())?;
        if session.web_origin != profile.web_origin {
            return Err(Error::new(
                ErrorCode::ScopeConflict,
                "Session and profile origins differ; import into a separate profile",
            ));
        }
        Ok(session)
    }

    pub fn logout(&self, name: &str) -> Result<bool> {
        let _lock = self.lock(name)?;
        let path = self.profile_dir(name)?.join("session.vault");
        if !path.exists() {
            return Ok(false);
        }
        private_files::remove(&path)?;
        if self.key_file.is_none() {
            let entry = keyring::Entry::new("lark-user-v1", &self.key_identity(name)?)
                .map_err(|_| credential_error())?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => (),
                Err(_) => return Err(credential_error()),
            }
        }
        Ok(true)
    }
}

fn credential_error() -> Error {
    Error::new(
        ErrorCode::CredentialStoreUnavailable,
        "OS keyring is unavailable or locked. Unlock it, or explicitly use a private key generated by auth keygen and --key-file; no plaintext fallback is used",
    )
}

fn origin(value: &str) -> Result<String> {
    let url = Url::parse(value)
        .map_err(|_| Error::invalid("Provide the exact HTTPS Lark web URL from the browser"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::invalid(
            "Web URL requires HTTPS without credentials, query strings or fragments",
        ));
    }
    Ok(url.origin().ascii_serialization())
}

/// Reads a bounded private input without including operating-system details in errors.
pub fn read_private_file(path: &Path, max_bytes: usize) -> Result<Zeroizing<Vec<u8>>> {
    private_files::read(path, max_bytes)
}
