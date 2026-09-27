//! Lists the PDF features a document uses and the crates that implement them.

use crate::model::Inventory;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    string_kind::StringKind,
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

/// Scans every loaded object.
pub fn scan(objects: &ObjectCollection) -> Inventory {
    let mut features = BTreeMap::<String, usize>::new();
    let mut non_embedded = BTreeSet::new();
    for object in objects.map.values() {
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
        walk(object, 0, &mut features);
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

fn resolve<'a>(
    objects: &'a ObjectCollection,
    value: &'a ObjectVariant,
) -> Option<&'a ObjectVariant> {
    objects.resolve_object(value).ok()
}

fn name(value: &ObjectVariant) -> Option<String> {
    match value {
        ObjectVariant::String(text) if text.kind() == StringKind::Name => {
            Some(String::from_utf8_lossy(text.as_bytes()).into_owned())
        }
        _ => None,
    }
}

/// Returns a name, or the names of an array.
fn names(objects: &ObjectCollection, value: Option<&ObjectVariant>) -> Vec<String> {
    match value.and_then(|value| resolve(objects, value)) {
        Some(ObjectVariant::Array(items)) => items
            .iter()
            .filter_map(|item| resolve(objects, item).and_then(name))
            .collect(),
        Some(other) => name(other).into_iter().collect(),
        None => Vec::new(),
    }
}

fn get_name(objects: &ObjectCollection, dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    dictionary
        .get(key)
        .and_then(|value| resolve(objects, value))
        .and_then(name)
}

fn get_integer(objects: &ObjectCollection, dictionary: &Dictionary, key: &[u8]) -> Option<i64> {
    match dictionary
        .get(key)
        .and_then(|value| resolve(objects, value))
    {
        Some(ObjectVariant::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn scan_dictionary(
    objects: &ObjectCollection,
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
    if let Some(ObjectVariant::Dictionary(mask)) = dictionary
        .get(b"SMask")
        .and_then(|value| resolve(objects, value))
        && let Some(kind) = get_name(objects, mask, b"S")
    {
        count(features, format!("soft_mask:{kind}"));
    }
    if let Some(ObjectVariant::Dictionary(group)) = dictionary
        .get(b"Group")
        .and_then(|value| resolve(objects, value))
        && get_name(objects, group, b"S").as_deref() == Some("Transparency")
    {
        count(features, "transparency_group".to_owned());
    }
    if dictionary.get(b"OCProperties").is_some() {
        count(features, "optional_content".to_owned());
    }
    if dictionary.get(b"XFA").is_some() {
        count(features, "xfa".to_owned());
    }
}

/// Finds color space arrays nested anywhere in an object without following references.
fn walk(object: &ObjectVariant, depth: usize, features: &mut BTreeMap<String, usize>) {
    if depth > 8 {
        return;
    }
    let next = depth.saturating_add(1);
    match object {
        ObjectVariant::Array(items) => {
            if let Some(family) = items.first().and_then(name)
                && COLOR_SPACES.contains(&family.as_bytes())
            {
                count(features, format!("colorspace:{family}"));
            }
            for item in items.iter() {
                walk(item, next, features);
            }
        }
        ObjectVariant::Dictionary(dictionary) => {
            for value in dictionary.dictionary.values() {
                walk(value, next, features);
            }
        }
        ObjectVariant::Stream(stream) => {
            for value in stream.dictionary.dictionary.values() {
                walk(value, next, features);
            }
        }
        _ => {}
    }
}
