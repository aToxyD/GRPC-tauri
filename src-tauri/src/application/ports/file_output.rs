//! Outbound ports: encrypt serialized payloads and persist bytes (no business rules).
//!
//! **Sync envelopes:** plaintext package bytes are assembled in `infrastructure::sync::packages`
//! (`PackageBuilder`), then encrypted via [`ExportPayloadEncryptor`] without mixing file I/O.

use crate::errors::AppResult;
use std::io::{Read, Write};

/// Transform export bytes before storage (encryption, signing wrapper, etc.).
/// Keep separate from [`ExportFileSink`] so callers can encrypt without writing, or write plain bytes.
pub trait ExportPayloadEncryptor: Send + Sync {
    fn encrypt_for_storage(&self, plaintext: &[u8]) -> AppResult<Vec<u8>>;

    /// Encrypt a stream from input to output using simple age file encryption
    fn encrypt_stream(&self, input: &mut dyn Read, output: &mut dyn Write) -> AppResult<()>;
}

/// Persist bytes to a path (filesystem today; could be cloud or IPC later).
pub trait ExportFileSink: Send + Sync {
    fn write_all(&self, path: &str, data: &[u8]) -> AppResult<()>;
}
