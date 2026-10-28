//! On-disk snapshots of external indexes, so other processes can search an
//! app that is closed. See `design/search.md` 8.7.
//!
//! File layout (little endian):
//!
//! ```text
//! "BXSI"  u32 format  u32 tokenizer  u32 header_len  header JSON
//! u64 payload_len  payload  u64 fnv1a(header ++ payload)
//! ```
//!
//! The header is `{app, app_version, index, saved_at, documents, schema}`; the
//! payload is one JSON document per line (`{"id", "fields", "external"}`).
//! Only external documents of an external index are written. The reader
//! rebuilds the in-memory index from them.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::index::Index;
use crate::schema::Schema;
use crate::tokenizer::TOKENIZER_VERSION;

pub const FORMAT_VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"BXSI";
/// Largest snapshot read: a corrupt length must not allocate gigabytes.
const MAX_FILE_BYTES: u64 = 256 << 20;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub app: String,
    pub app_version: String,
    pub index: String,
    /// Unix seconds.
    pub saved_at: u64,
    pub documents: usize,
    pub schema: Schema,
}

/// `$XDG_STATE_HOME/bingux/search`, or `~/.local/state/bingux/search`.
pub fn state_root() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".local/state"),
    };
    Some(base.join("bingux/search"))
}

/// `<root>/<app>/<index>.idx`.
pub fn path_for(root: &Path, app: &str, index: &str) -> PathBuf {
    root.join(app).join(format!("{index}.idx"))
}

fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes `index` to `path`: a temporary file in the same directory, fsynced,
/// then renamed over the target, mode 0600. Returns `Ok(None)` and writes
/// nothing when the index is not external.
pub fn save(index: &Index, app: &str, app_version: &str, path: &Path) -> Result<Option<Meta>> {
    if !index.schema().external {
        return Ok(None);
    }
    let docs = index.export(true);
    let meta = Meta {
        app: app.to_string(),
        app_version: app_version.to_string(),
        index: index.name().to_string(),
        saved_at: unix_now(),
        documents: docs.len(),
        schema: index.schema().clone(),
    };
    let header = serde_json::to_vec(&meta).map_err(|e| Error::Corrupt(e.to_string()))?;
    let mut payload = Vec::new();
    for d in &docs {
        serde_json::to_writer(&mut payload, d).map_err(|e| Error::Corrupt(e.to_string()))?;
        payload.push(b'\n');
    }
    let sum = fnv1a(&[&header, &payload]);

    let dir = path
        .parent()
        .ok_or_else(|| Error::Corrupt("snapshot path has no directory".into()))?;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("snapshot"),
        std::process::id()
    ));
    let write = || -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(MAGIC)?;
        f.write_all(&FORMAT_VERSION.to_le_bytes())?;
        f.write_all(&TOKENIZER_VERSION.to_le_bytes())?;
        f.write_all(&(header.len() as u32).to_le_bytes())?;
        f.write_all(&header)?;
        f.write_all(&(payload.len() as u64).to_le_bytes())?;
        f.write_all(&payload)?;
        f.write_all(&sum.to_le_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(Some(meta))
}

/// Reads the header of a snapshot without building the index.
pub fn read_meta(path: &Path) -> Result<Meta> {
    let (meta, _) = read(path)?;
    Ok(meta)
}

fn read(path: &Path) -> Result<(Meta, Vec<Document>)> {
    let len = fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(Error::Corrupt(format!("snapshot {} is too large", path.display())));
    }
    let bytes = fs::read(path)?;
    let bad = |what: &str| Error::Corrupt(format!("snapshot {}: {what}", path.display()));
    if bytes.len() < 4 + 4 + 4 + 4 || &bytes[..4] != MAGIC {
        return Err(bad("not a Bingux search snapshot"));
    }
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    if u32_at(4) != FORMAT_VERSION {
        return Err(bad("unknown format version"));
    }
    if u32_at(8) != TOKENIZER_VERSION {
        return Err(bad("written by another tokenizer version"));
    }
    let header_len = u32_at(12) as usize;
    let header_end = 16 + header_len;
    if bytes.len() < header_end + 8 {
        return Err(bad("truncated"));
    }
    let header = &bytes[16..header_end];
    let payload_len =
        u64::from_le_bytes(bytes[header_end..header_end + 8].try_into().unwrap()) as usize;
    let payload_start = header_end + 8;
    if bytes.len() != payload_start + payload_len + 8 {
        return Err(bad("length does not match"));
    }
    let payload = &bytes[payload_start..payload_start + payload_len];
    let sum = u64::from_le_bytes(bytes[payload_start + payload_len..].try_into().unwrap());
    if sum != fnv1a(&[header, payload]) {
        return Err(bad("checksum mismatch"));
    }
    let meta: Meta = serde_json::from_slice(header).map_err(|e| bad(&e.to_string()))?;
    let mut docs = Vec::with_capacity(meta.documents);
    for line in payload.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        docs.push(serde_json::from_slice::<Document>(line).map_err(|e| bad(&e.to_string()))?);
    }
    Ok((meta, docs))
}

/// Loads a snapshot into a new index. `expect_version` is the version of the
/// app the caller knows is installed; a snapshot of another version is
/// refused (`None` accepts any).
pub fn load(path: &Path, expect_version: Option<&str>) -> Result<(Meta, Index)> {
    let (meta, docs) = read(path)?;
    if let Some(v) = expect_version
        && meta.app_version != v
    {
        return Err(Error::Corrupt(format!(
            "snapshot {} is of app version {}, installed is {v}",
            path.display(),
            meta.app_version
        )));
    }
    let index = Index::new(&meta.index, meta.schema.clone())?;
    index.upsert(docs)?;
    Ok((meta, index))
}
