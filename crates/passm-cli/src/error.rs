//! CLI error type: every failure path maps to a typed variant that `main`
//! renders to stderr and converts to a nonzero exit code (never a panic).

use std::fmt;

/// Errors produced by the passm-cli harness.
#[derive(Debug)]
pub enum CliError {
    /// Bad command-line usage (missing/unknown flag, malformed hex salt).
    Usage(String),
    /// File read/write failure.
    Io(std::io::Error),
    /// Vault JSON parse failure.
    Json(serde_json::Error),
    /// Argon2id key derivation failure.
    Argon2(argon2::Error),
    /// PASSM1 envelope parse/decrypt failure (wrong password, tampered blob).
    Envelope(passm_crypto::envelope::EnvelopeError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "{msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "invalid vault JSON: {err}"),
            Self::Argon2(err) => write!(f, "key derivation failed: {err}"),
            Self::Envelope(err) => write!(f, "envelope error: {err}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<argon2::Error> for CliError {
    fn from(err: argon2::Error) -> Self {
        Self::Argon2(err)
    }
}

impl From<passm_crypto::envelope::EnvelopeError> for CliError {
    fn from(err: passm_crypto::envelope::EnvelopeError) -> Self {
        Self::Envelope(err)
    }
}