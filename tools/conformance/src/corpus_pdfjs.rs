//! pdf.js test corpus described by `test/test_manifest.json`.

use crate::model::Case;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

/// One manifest entry; unknown keys are pdf.js viewer options and are ignored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    /// Unique test id.
    pub id: String,
    /// PDF path relative to `test/`.
    pub file: String,
    /// Expected MD5 of the PDF.
    pub md5: String,
    /// True when the PDF is downloaded from the URL in `<file>.link`.
    #[serde(default, deserialize_with = "truthy")]
    pub link: bool,
    /// Test type: `eq`, `fbf`, `load`, `text`, `other`, ...
    #[serde(rename = "type")]
    pub kind: String,
    /// First page to test, one-based.
    pub first_page: Option<usize>,
    /// Last page to test, one-based.
    pub last_page: Option<usize>,
    /// Document password.
    pub password: Option<String>,
}

/// Accepts `true` and `"true"`; the manifest uses both.
fn truthy<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(value) => value,
        serde_json::Value::String(value) => value == "true",
        _ => false,
    })
}

/// Test types that render pages; the rest exercise viewer features Safe-PDF has no equivalent for.
const RENDERED_TYPES: [&str; 4] = ["eq", "fbf", "load", "text"];

/// Reads the manifest entries.
pub fn manifest(root: &Path) -> Result<Vec<ManifestEntry>> {
    let path = root.join("test/test_manifest.json");
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Lists rendered manifest entries, dropping entries that differ only in viewer options.
pub fn cases(root: &Path) -> Result<Vec<Case>> {
    let mut seen = HashSet::new();
    let mut cases = Vec::new();
    for entry in manifest(root)? {
        if !RENDERED_TYPES.contains(&entry.kind.as_str()) {
            continue;
        }
        let key = (
            entry.file.clone(),
            entry.first_page,
            entry.last_page,
            entry.password.clone(),
        );
        if !seen.insert(key) {
            continue;
        }
        cases.push(Case {
            path: root.join("test").join(&entry.file),
            password: entry.password,
            first_page: entry.first_page.map(|page| page.saturating_sub(1)),
            last_page: entry.last_page.map(|page| page.saturating_sub(1)),
            expected_md5: Some(entry.md5),
            note: Some(format!(
                "pdf.js {} test{}",
                entry.kind,
                if entry.link { ", linked file" } else { "" }
            )),
            id: entry.id,
        });
    }
    Ok(cases)
}
