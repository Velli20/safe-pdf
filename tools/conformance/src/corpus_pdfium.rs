//! PDFium test corpus: every `*.pdf` below the checkout, identified by relative path.

use crate::model::Case;
use anyhow::Result;
use std::path::Path;
use walkdir::WalkDir;

/// Lists every PDF below `root`; `.in` templates are not PDFs and are skipped.
pub fn cases(root: &Path) -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().is_none_or(|ext| ext != "pdf") {
            continue;
        }
        if path.components().any(|part| part.as_os_str() == ".git") {
            continue;
        }
        let id = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        cases.push(Case {
            id,
            path: path.to_owned(),
            password: None,
            first_page: None,
            last_page: None,
            expected_md5: None,
            note: None,
        });
    }
    Ok(cases)
}
