//! Locates the `#[error("...")]` template that produced an error message.
//!
//! Errors constructed directly instead of through a traced `From` conversion carry no
//! backtrace; the template location still points readers at the defining enum variant.

use crate::corpus;
use std::{fs, path::Path, sync::OnceLock};
use walkdir::WalkDir;

/// Shortest literal text a template must contribute for a match to be meaningful.
const MIN_LITERAL: usize = 8;

struct Template {
    fragments: Vec<String>,
    location: String,
    variant: String,
}

fn templates() -> &'static [Template] {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let root = corpus::workspace_root();
        let mut templates = Vec::new();
        for entry in WalkDir::new(root.join("crates")).into_iter().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                scan_file(&root, path, &mut templates);
            }
        }
        templates
    })
}

fn scan_file(root: &Path, path: &Path, templates: &mut Vec<Template>) {
    let Ok(source) = fs::read_to_string(path) else {
        return;
    };
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/");
    let lines: Vec<&str> = source.lines().collect();
    for (number, line) in lines.iter().enumerate() {
        let Some(literal) = line
            .split_once("#[error(\"")
            .and_then(|(_, rest)| rest.rsplit_once("\")"))
            .map(|(literal, _)| literal.replace("\\\"", "\""))
        else {
            continue;
        };
        let fragments = fragments(&literal);
        if fragments.iter().map(String::len).sum::<usize>() < MIN_LITERAL {
            continue;
        }
        let variant = lines
            .iter()
            .skip(number.saturating_add(1))
            .map(|line| line.trim())
            .find(|line| !line.is_empty() && !line.starts_with("#[") && !line.starts_with("//"))
            .map(|line| {
                line.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect()
            })
            .unwrap_or_default();
        templates.push(Template {
            fragments,
            location: format!("{relative}:{}", number.saturating_add(1)),
            variant,
        });
    }
}

/// Splits a format string into its literal parts.
fn fragments(literal: &str) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in literal.chars() {
        match c {
            '{' => {
                depth = depth.saturating_add(1);
                if !current.is_empty() {
                    fragments.push(std::mem::take(&mut current));
                }
            }
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => current.push(c),
            _ => {}
        }
    }
    if !current.is_empty() {
        fragments.push(current);
    }
    fragments
}

fn matches(template: &Template, message: &str) -> bool {
    let mut rest = message;
    for fragment in &template.fragments {
        match rest.find(fragment.as_str()) {
            Some(position) => {
                rest = rest
                    .get(position.saturating_add(fragment.len())..)
                    .unwrap_or_default();
            }
            None => return false,
        }
    }
    true
}

/// Returns the templates that best explain `message`, most specific first. Ties are ordered
/// by location so results do not depend on directory walk order.
fn best(message: &str) -> Vec<&'static Template> {
    let mut found: Vec<(usize, &Template)> = templates()
        .iter()
        .filter(|template| matches(template, message))
        .map(|template| (template.fragments.iter().map(String::len).sum(), template))
        .collect();
    let Some(best) = found.iter().map(|(score, _)| *score).max() else {
        return Vec::new();
    };
    found.retain(|(score, _)| *score == best);
    let mut found: Vec<&Template> = found.into_iter().map(|(_, template)| template).collect();
    found.sort_by(|a, b| a.location.cmp(&b.location));
    found
}

/// Returns `path:line (Variant)` of the templates that best explain `message`.
pub fn locate(message: &str) -> Vec<String> {
    best(message)
        .into_iter()
        .take(3)
        .map(|template| format!("{} ({})", template.location, template.variant))
        .collect()
}

/// Returns the crate and enum variant whose template best explains `message`, for example
/// `("pdf-canvas", "PathRequired")`. Line numbers are left out so the result stays stable
/// across unrelated edits.
pub fn origin(message: &str) -> Option<(String, String)> {
    let template = best(message).into_iter().next()?;
    let krate = template
        .location
        .strip_prefix("crates/")?
        .split('/')
        .next()?
        .to_owned();
    (!template.variant.is_empty()).then(|| (krate, template.variant.clone()))
}

/// Returns `path` and `line` of the template that best explains `message`.
pub fn location(message: &str) -> Option<(String, usize)> {
    let template = best(message).into_iter().next()?;
    let (path, line) = template.location.rsplit_once(':')?;
    Some((path.to_owned(), line.parse().ok()?))
}
