//! Loading of the registry, rule packs and profiles from a data root.
//!
//! Loading never fails because a data file is wrong. A file that is missing,
//! unreadable, not JSON or not shaped like the typed model is recorded on its
//! [`DataFile`] so that [`crate::validate`] can report it the way the Node
//! validator does. [`LoadError`] is reserved for failures that make the data
//! root itself unusable, such as a directory that cannot be listed.

use std::cmp::Ordering;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::model::{AuthorityRegistry, Profile, RulePack};

/// Why the data root as a whole could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot list {path}: {source}")]
    ListDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// A problem reading one data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileProblem {
    /// The file does not exist.
    Missing,
    /// The file exists but is not valid JSON. The parser message is dropped
    /// so the report matches the Node output byte for byte.
    NotJson,
    /// Any other read failure. Carries the I/O message.
    Unreadable(String),
}

/// One data file: where it came from, the raw JSON, and the typed view.
#[derive(Debug, Clone)]
pub struct DataFile<T> {
    /// File name only, for example `registry.json`.
    pub file_name: String,
    /// Full path used to read the file.
    pub path: PathBuf,
    /// The parsed JSON document, or why it could not be read.
    pub raw: Result<Value, FileProblem>,
    /// The typed model, when the raw document deserializes cleanly.
    pub typed: Option<T>,
    /// Why the typed model could not be built, when the raw document is valid JSON.
    pub typed_error: Option<String>,
}

impl<T: DeserializeOwned> DataFile<T> {
    fn read(path: PathBuf) -> Self {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let raw = read_json(&path);
        let (typed, typed_error) = match &raw {
            Ok(value) => match serde_json::from_value::<T>(value.clone()) {
                Ok(model) => (Some(model), None),
                Err(error) => (None, Some(error.to_string())),
            },
            Err(_) => (None, None),
        };
        DataFile {
            file_name,
            path,
            raw,
            typed,
            typed_error,
        }
    }
}

/// Everything under one data root: registry, packs, profiles.
#[derive(Debug, Clone)]
pub struct DataSet {
    pub root: PathBuf,
    /// `authorities/registry.json`.
    pub registry: DataFile<AuthorityRegistry>,
    /// Every `rules/packs/*.json`, sorted by file name.
    pub packs: Vec<DataFile<RulePack>>,
    /// Every `profiles/*.json`, sorted by file name.
    pub profiles: Vec<DataFile<Profile>>,
}

impl DataSet {
    /// Typed registry, when it loaded cleanly.
    pub fn registry(&self) -> Option<&AuthorityRegistry> {
        self.registry.typed.as_ref()
    }

    /// Typed packs that loaded cleanly.
    pub fn typed_packs(&self) -> impl Iterator<Item = &RulePack> {
        self.packs.iter().filter_map(|f| f.typed.as_ref())
    }

    /// Typed profiles that loaded cleanly.
    pub fn typed_profiles(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter_map(|f| f.typed.as_ref())
    }

    /// Typed-model problems, one `file: message` line per file that did not
    /// deserialize. Empty when every readable file matches the typed model.
    pub fn typed_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.registry.typed_error {
            out.push(format!("authorities/{}: {e}", self.registry.file_name));
        }
        for f in &self.packs {
            if let Some(e) = &f.typed_error {
                out.push(format!("rules/packs/{}: {e}", f.file_name));
            }
        }
        for f in &self.profiles {
            if let Some(e) = &f.typed_error {
                out.push(format!("profiles/{}: {e}", f.file_name));
            }
        }
        out
    }
}

/// Load the registry, all packs and all profiles under `root`.
pub fn load_data_root(root: &Path) -> Result<DataSet, LoadError> {
    let registry = DataFile::read(root.join("authorities").join("registry.json"));
    let packs = list_json_files(&root.join("rules").join("packs"))?
        .into_iter()
        .map(DataFile::read)
        .collect();
    let profiles = list_json_files(&root.join("profiles"))?
        .into_iter()
        .map(DataFile::read)
        .collect();
    Ok(DataSet {
        root: root.to_path_buf(),
        registry,
        packs,
        profiles,
    })
}

fn read_json(path: &Path) -> Result<Value, FileProblem> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(FileProblem::Missing),
        Err(e) => return Err(FileProblem::Unreadable(e.to_string())),
    };
    // Node decodes with U+FFFD replacement for invalid UTF-8, so do the same.
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str(&text).map_err(|_| FileProblem::NotJson)
}

/// Names ending in `.json`, sorted by UTF-16 code units like JavaScript's
/// default sort. A missing directory yields an empty list.
fn list_json_files(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(LoadError::ListDir {
                path: dir.to_path_buf(),
                source,
            })
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::ListDir {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            names.push(name);
        }
    }
    names.sort_by(|a, b| cmp_utf16(a, b));
    Ok(names.into_iter().map(|n| dir.join(n)).collect())
}

fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
