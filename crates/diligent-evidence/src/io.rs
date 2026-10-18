//! Durable file helpers: content hashing, atomic whole-file writes and
//! single-write appends.

use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // The counter keeps two threads that read the same clock tick apart.
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{nanos}-{n}", std::process::id()))
}

fn sync_parent(path: &Path) {
    // Best effort: not every platform lets a directory be opened and synced.
    if let Some(parent) = path.parent() {
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

/// Writes whatever `fill` writes to a temp file next to `path`, flushes it to
/// disk and renames it over `path`.
///
/// If `fill` or any later step fails, the temp file is removed and `path` is
/// left exactly as it was: either the old content, or absent. A reader never
/// sees a partial file under the final name.
pub fn atomic_write_with<F>(path: &Path, fill: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let mut attempt = 0;
    let (tmp, mut file) = loop {
        let tmp = temp_path(path);
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 8 => attempt += 1,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        fill(&mut file)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    match result {
        Ok(()) => {
            sync_parent(path);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Atomically replaces `path` with `bytes`. See [`atomic_write_with`].
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(path, |file| file.write_all(bytes))
}

/// Atomically creates `path` with `bytes` unless it already exists.
///
/// Returns `Ok(true)` when this call created the file and `Ok(false)` when it
/// was already there; an existing file is never overwritten, so concurrent
/// callers agree on one winner.
pub fn atomic_write_new(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    let mut attempt = 0;
    let (tmp, mut file) = loop {
        let tmp = temp_path(path);
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 8 => attempt += 1,
            Err(e) => return Err(e),
        }
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let linked = written.and_then(|()| match fs::hard_link(&tmp, path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) if !path.exists() => fs::rename(&tmp, path).map(|()| true),
        Err(e) => Err(e),
    });
    let _ = fs::remove_file(&tmp);
    if matches!(linked, Ok(true)) {
        sync_parent(path);
    }
    linked
}

/// Appends one line to `path` (creating it) with a single `write` on a file
/// opened for append, then flushes it with `sync_data`.
///
/// `line` must not contain a newline; the trailing `\n` is added here and
/// goes out in the same write, so two processes appending to one file never
/// interleave their lines.
pub fn append_line(path: &Path, line: &str) -> io::Result<()> {
    if line.contains('\n') || line.contains('\r') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a JSON Lines record must not contain a newline",
        ));
    }
    let mut buffer = Vec::with_capacity(line.len() + 1);
    buffer.extend_from_slice(line.as_bytes());
    buffer.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let written = file.write(&buffer)?;
    if written < buffer.len() {
        // A short write is rare; finish it rather than leave a torn line.
        file.write_all(&buffer[written..])?;
    }
    file.sync_data()
}
