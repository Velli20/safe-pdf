use std::collections::{HashMap, HashSet};

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    version: u8,
    sources: HashMap<String, Source>,
    assets: Vec<Asset>,
    derived_pdf: DerivedPdf,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Source {
    repository: String,
    raw_base: String,
    revision: String,
    notice: String,
    terms: String,
}

#[derive(Deserialize)]
struct Asset {
    id: String,
    source: String,
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Deserialize)]
struct DerivedPdf {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    input: String,
    format: String,
    width: u32,
    height: u32,
    components: Vec<Component>,
    encoding: String,
    coverage: Vec<String>,
    #[serde(default)]
    embedded_images: Vec<String>,
    expected: Expected,
    provenance: String,
}

#[derive(Deserialize)]
struct Component {
    precision: u8,
    signed: bool,
    subsampling: [u8; 2],
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Expected {
    Samples {
        source_components: Vec<String>,
        display_pixels: Option<String>,
        display_rect: Option<[u32; 4]>,
        source_comparison: Option<SourceComparison>,
        display_comparison: Option<DisplayComparison>,
    },
    Error {
        category: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SourceComparison {
    Exact,
    T803 { peak: Vec<u32>, mse: Vec<f64> },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DisplayComparison {
    T803 { peak: u32 },
    ReferenceOnly,
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_assets(manifest: &Manifest) -> HashSet<&str> {
    let mut ids = HashSet::new();
    for (name, source) in &manifest.sources {
        assert!(source.repository.starts_with("https://"), "{name}");
        assert!(source.raw_base.starts_with("https://"), "{name}");
        assert!(source.revision.len() == 40, "{name}");
        assert!(!source.terms.is_empty(), "{name}");
        assert!(
            manifest
                .assets
                .iter()
                .any(|asset| asset.source == *name && asset.path == source.notice),
            "missing notice for {name}"
        );
    }

    for asset in &manifest.assets {
        assert!(
            ids.insert(asset.id.as_str()),
            "duplicate asset: {}",
            asset.id
        );
        assert!(manifest.sources.contains_key(&asset.source), "{}", asset.id);
        assert_eq!(asset.id, format!("{}/{}", asset.source, asset.path));
        assert!(!asset.path.starts_with('/'), "{}", asset.id);
        assert!(
            !asset.path.split('/').any(|part| part == ".."),
            "{}",
            asset.id
        );
        assert!(is_sha256(&asset.sha256), "{}", asset.id);
        assert!(asset.bytes > 0, "{}", asset.id);
    }
    ids
}

fn validate_reference(
    case: &Case,
    assets: &HashSet<&str>,
    source_components: &[String],
    display_pixels: Option<&String>,
    display_rect: Option<[u32; 4]>,
    source_comparison: Option<&SourceComparison>,
    display_comparison: Option<&DisplayComparison>,
) {
    assert!(source_components.len() == case.components.len() || source_components.is_empty());
    assert!(
        !source_components.is_empty() || display_pixels.is_some(),
        "{}",
        case.id
    );
    for reference in source_components {
        assert!(assets.contains(reference.as_str()), "{}", case.id);
        assert!(reference.ends_with(".pgx"), "{}", case.id);
    }
    assert_eq!(source_comparison.is_some(), !source_components.is_empty());
    assert_eq!(display_comparison.is_some(), display_pixels.is_some());

    if let Some(reference) = display_pixels {
        assert!(assets.contains(reference.as_str()), "{}", case.id);
        assert!(!source_components.contains(reference), "{}", case.id);
    }
    if let Some([x, y, width, height]) = display_rect {
        assert!(display_pixels.is_some(), "{}", case.id);
        assert!(width > 0 && height > 0, "{}", case.id);
        assert!(x.checked_add(width).is_some_and(|right| right <= 88));
        assert!(y.checked_add(height).is_some_and(|bottom| bottom <= 128));
    }
    if let Some(SourceComparison::T803 { peak, mse }) = source_comparison {
        assert_eq!(peak.len(), source_components.len(), "{}", case.id);
        assert_eq!(mse.len(), source_components.len(), "{}", case.id);
        assert!(mse.iter().all(|value| value.is_finite() && *value >= 0.0));
    }
    if let Some(DisplayComparison::T803 { peak }) = display_comparison {
        assert!(*peak > 0, "{}", case.id);
    }
}

fn validate_case(case: &Case, assets: &HashSet<&str>, derived_pdf: &DerivedPdf) {
    assert!(!case.id.is_empty());
    assert!(!case.provenance.is_empty(), "{}", case.id);
    assert!(!case.coverage.is_empty(), "{}", case.id);
    assert!(matches!(
        case.format.as_str(),
        "codestream" | "jp2" | "jpx" | "pdf"
    ));
    assert!(matches!(
        case.encoding.as_str(),
        "reversible" | "irreversible" | "unknown"
    ));
    assert!(assets.contains(case.input.as_str()) || case.input == "derived_pdf");
    if case.input == "derived_pdf" {
        assert_eq!(case.format, "pdf");
        assert!(derived_pdf.path.ends_with(".pdf"));
    } else {
        assert!(!case.components.is_empty(), "{}", case.id);
    }
    for component in &case.components {
        assert!((1..=38).contains(&component.precision), "{}", case.id);
        assert!(component.subsampling.iter().all(|factor| *factor > 0));
    }
    if case.coverage.iter().any(|feature| feature == "signed") {
        assert!(case.components.iter().any(|component| component.signed));
    }
    match &case.expected {
        Expected::Samples {
            source_components,
            display_pixels,
            display_rect,
            source_comparison,
            display_comparison,
        } => {
            assert!(case.width > 0 && case.height > 0, "{}", case.id);
            validate_reference(
                case,
                assets,
                source_components,
                display_pixels.as_ref(),
                *display_rect,
                source_comparison.as_ref(),
                display_comparison.as_ref(),
            );
        }
        Expected::Error { category } => {
            assert!(matches!(
                category.as_str(),
                "truncated"
                    | "invalid_box"
                    | "invalid_marker"
                    | "limit_exceeded"
                    | "unsupported_feature"
            ));
            assert_eq!(case.encoding, "unknown");
        }
    }
}

#[test]
fn fixture_manifest_is_an_offline_contract() {
    let manifest: Manifest = serde_json::from_str(include_str!("../fixtures/manifest.json"))
        .expect("fixture manifest must parse without invoking the decoder");
    assert_eq!(manifest.version, 1);
    assert!(is_sha256(&manifest.derived_pdf.sha256));
    assert!(manifest.derived_pdf.bytes > 0);
    let assets = validate_assets(&manifest);

    let mut ids = HashSet::new();
    let mut coverage = HashSet::new();
    for case in &manifest.cases {
        assert!(ids.insert(case.id.as_str()), "duplicate case: {}", case.id);
        validate_case(case, &assets, &manifest.derived_pdf);
        coverage.extend(case.coverage.iter().map(String::as_str));
    }
    for case in &manifest.cases {
        for embedded in &case.embedded_images {
            assert!(
                ids.contains(embedded.as_str()),
                "missing embedded image: {embedded}"
            );
        }
    }
    for required in [
        "reversible_grayscale",
        "irreversible_rgb",
        "subsampled_components",
        "multiple_tiles",
        "multiple_tile_parts",
        "signed",
        "high_precision",
        "jp2_color_boxes",
        "jp2_palette",
        "jpx_baseline",
        "pdf_jpx_image",
        "truncated_box",
        "malformed_box_length",
        "malformed_siz",
        "malformed_cod",
        "malformed_qcd",
        "absurd_dimensions",
        "unsupported_extension",
    ] {
        assert!(coverage.contains(required), "missing coverage: {required}");
    }
}
