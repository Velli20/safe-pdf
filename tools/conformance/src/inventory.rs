//! Lists the PDF features a document uses and the crates that implement them.

use crate::model::Inventory;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    ObjectAccess, ObjectReader, ReadSession, dictionary::Dictionary, object_variant::ObjectVariant,
    pdf_name::PdfName,
};
use std::collections::{BTreeMap, BTreeSet};

/// Color space families worth reporting; device spaces are omitted as ubiquitous.
const COLOR_SPACES: [&[u8]; 9] = [
    b"ICCBased",
    b"Indexed",
    b"Separation",
    b"DeviceN",
    b"Lab",
    b"CalRGB",
    b"CalGray",
    b"Pattern",
    b"NChannel",
];

/// Feature prefixes and the crates that implement them.
const CRATES: [(&str, &str); 22] = [
    ("font:Type3", "pdf-canvas (Type3 glyph procedures)"),
    ("font:", "pdf-font"),
    ("font_program:", "pdf-font"),
    ("non_embedded_font", "pdf-text-engine (font substitution)"),
    ("filter:JBIG2Decode", "pdf-jbig2"),
    ("filter:CCITTFaxDecode", "pdf-ccitt"),
    ("filter:JPXDecode", "pdf-image"),
    ("filter:DCTDecode", "pdf-image"),
    ("filter:", "pdf-filter"),
    ("image", "pdf-image"),
    ("shading:", "pdf-shading"),
    ("pattern:tiling", "pdf-canvas (tiling_shader)"),
    ("pattern:shading", "pdf-shading"),
    ("function:type4", "pdf-postscript"),
    ("function:", "pdf-function"),
    ("colorspace:", "pdf-color-space"),
    ("blend:", "pdf-graphics-skia (blend modes)"),
    ("soft_mask:", "pdf-canvas (mask_layer)"),
    ("transparency_group", "pdf-canvas (mask_layer)"),
    (
        "annotation:",
        "pdf-annotation-core (not part of the compared render)",
    ),
    ("optional_content", "pdf-document (optional content)"),
    ("xfa", "unsupported (XFA forms)"),
];

/// The traversal through which scanned values are resolved.
type Objects<'reader> = ReadSession<'reader, ObjectCollection>;

/// Scans every loaded object.
pub fn scan(reader: &ObjectReader<ObjectCollection>) -> Inventory {
    let mut features = BTreeMap::<String, usize>::new();
    let mut non_embedded = BTreeSet::new();
    let objects = &mut reader.session();
    for object in reader.source().map.values() {
        let dictionary = match object {
            ObjectVariant::Dictionary(dictionary) => dictionary,
            ObjectVariant::Stream(stream) => {
                let dictionary = &stream.dictionary;
                for filter in names(objects, dictionary.get(b"Filter")) {
                    count(&mut features, format!("filter:{filter}"));
                }
                dictionary
            }
            _ => continue,
        };
        scan_dictionary(objects, dictionary, &mut features, &mut non_embedded);
        walk(objects, object, 0, &mut features);
    }
    if !non_embedded.is_empty() {
        count(&mut features, "non_embedded_font".to_owned());
    }
    let likely_crates = features
        .keys()
        .filter_map(|feature| {
            CRATES
                .iter()
                .find(|(prefix, _)| feature.starts_with(prefix))
                .map(|(_, name)| (*name).to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Inventory {
        features: features.into_iter().collect(),
        non_embedded_fonts: non_embedded.into_iter().collect(),
        likely_crates,
    }
}

fn count(features: &mut BTreeMap<String, usize>, feature: String) {
    let entry = features.entry(feature).or_default();
    *entry = entry.saturating_add(1);
}

fn name(objects: &mut Objects<'_>, value: &ObjectVariant) -> Option<String> {
    let name = objects.read::<PdfName>(value).ok()?;
    Some(String::from_utf8_lossy(name.as_bytes()).into_owned())
}

/// Returns a name, or the names of an array.
fn names(objects: &mut Objects<'_>, value: Option<&ObjectVariant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(resolved) = objects.resolve(value) else {
        return Vec::new();
    };
    match resolved.value() {
        ObjectVariant::Array(items) => items
            .iter()
            .filter_map(|item| name(objects, item))
            .collect(),
        _ => name(objects, value).into_iter().collect(),
    }
}

fn get_name(objects: &mut Objects<'_>, dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    name(objects, dictionary.get(key)?)
}

fn get_integer(objects: &mut Objects<'_>, dictionary: &Dictionary, key: &[u8]) -> Option<i64> {
    objects.read(dictionary.get(key)?).ok()
}

/// Returns the `/S` name of the dictionary held by `key`, such as a soft mask or a group.
fn get_dictionary_kind(
    objects: &mut Objects<'_>,
    dictionary: &Dictionary,
    key: &[u8],
) -> Option<String> {
    let resolved = objects.resolve(dictionary.get(key)?).ok()?;
    match resolved.value() {
        ObjectVariant::Dictionary(entry) => get_name(objects, entry, b"S"),
        _ => None,
    }
}

fn scan_dictionary(
    objects: &mut Objects<'_>,
    dictionary: &Dictionary,
    features: &mut BTreeMap<String, usize>,
    non_embedded: &mut BTreeSet<String>,
) {
    let kind = get_name(objects, dictionary, b"Type");
    let subtype = get_name(objects, dictionary, b"Subtype");
    match (kind.as_deref(), subtype.as_deref()) {
        (Some("Font"), Some(subtype)) => {
            count(features, format!("font:{subtype}"));
            // Simple fonts without a descriptor are the standard 14, never embedded.
            if matches!(subtype, "Type1" | "MMType1" | "TrueType")
                && dictionary.get(b"FontDescriptor").is_none()
                && let Some(font) = get_name(objects, dictionary, b"BaseFont")
            {
                non_embedded.insert(font);
            }
        }
        (Some("Annot"), Some(subtype)) => count(features, format!("annotation:{subtype}")),
        (_, Some("Image")) => {
            count(features, "image".to_owned());
            if dictionary.get(b"SMask").is_some() {
                count(features, "image:soft_mask".to_owned());
            }
            if matches!(
                dictionary.get(b"ImageMask"),
                Some(ObjectVariant::Boolean(true))
            ) {
                count(features, "image:stencil_mask".to_owned());
            }
            if let Some(bits) = get_integer(objects, dictionary, b"BitsPerComponent")
                && bits != 8
            {
                count(features, format!("image:{bits}_bits"));
            }
        }
        _ => {}
    }
    if kind.as_deref() == Some("FontDescriptor") {
        let program = [&b"FontFile"[..], b"FontFile2", b"FontFile3"]
            .into_iter()
            .find(|key| dictionary.get(key).is_some());
        match program {
            Some(key) => count(
                features,
                format!("font_program:{}", String::from_utf8_lossy(key)),
            ),
            None => {
                if let Some(font) = get_name(objects, dictionary, b"FontName") {
                    non_embedded.insert(font);
                }
            }
        }
    }
    if let Some(shading) = get_integer(objects, dictionary, b"ShadingType") {
        count(features, format!("shading:type{shading}"));
    }
    match get_integer(objects, dictionary, b"PatternType") {
        Some(1) => count(features, "pattern:tiling".to_owned()),
        Some(2) => count(features, "pattern:shading".to_owned()),
        _ => {}
    }
    if let Some(function) = get_integer(objects, dictionary, b"FunctionType") {
        count(features, format!("function:type{function}"));
    }
    for mode in names(objects, dictionary.get(b"BM")) {
        if mode != "Normal" && mode != "Compatible" {
            count(features, format!("blend:{mode}"));
        }
    }
    if let Some(kind) = get_dictionary_kind(objects, dictionary, b"SMask") {
        count(features, format!("soft_mask:{kind}"));
    }
    if get_dictionary_kind(objects, dictionary, b"Group").as_deref() == Some("Transparency") {
        count(features, "transparency_group".to_owned());
    }
    if dictionary.get(b"OCProperties").is_some() {
        count(features, "optional_content".to_owned());
    }
    if dictionary.get(b"XFA").is_some() {
        count(features, "xfa".to_owned());
    }
}

/// Finds color space arrays nested anywhere in an object without following references
/// into nested values.
fn walk(
    objects: &mut Objects<'_>,
    object: &ObjectVariant,
    depth: usize,
    features: &mut BTreeMap<String, usize>,
) {
    if depth > 8 {
        return;
    }
    let next = depth.saturating_add(1);
    match object {
        ObjectVariant::Array(items) => {
            if let Some(family) = items.first().and_then(|item| name(objects, item))
                && COLOR_SPACES.contains(&family.as_bytes())
            {
                count(features, format!("colorspace:{family}"));
            }
            for item in items.iter() {
                walk(objects, item, next, features);
            }
        }
        ObjectVariant::Dictionary(dictionary) => {
            for value in dictionary.dictionary.values() {
                walk(objects, value, next, features);
            }
        }
        ObjectVariant::Stream(stream) => {
            for value in stream.dictionary.dictionary.values() {
                walk(objects, value, next, features);
            }
        }
        _ => {}
    }
}
