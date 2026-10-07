use serde::Serialize;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArgument,
    AuthMissing,
    AuthExpired,
    ProtocolUnverified,
    CredentialStoreUnavailable,
    UnsafeFile,
    StorageError,
    ScopeConflict,
    ArchiveConflict,
    NotFound,
    AmbiguousId,
    InvalidCursor,
    OutputError,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn invalid(message: &'static str) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn storage() -> Self {
        Self::new(
            ErrorCode::StorageError,
            "Local storage operation failed; private contents are redacted",
        )
    }

    pub fn exit_code(&self) -> u8 {
        match self.code {
            ErrorCode::InvalidArgument | ErrorCode::InvalidCursor => 2,
            ErrorCode::AuthMissing | ErrorCode::AuthExpired => 3,
            ErrorCode::ProtocolUnverified => 5,
            ErrorCode::NotFound => 9,
            _ => 7,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}
