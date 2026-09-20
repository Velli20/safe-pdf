//! Backend-neutral entries extended with the CSS values a DOM host applies verbatim.
use num_traits::ToPrimitive;
use pdf_annotation_core::{AnnotationEntry, kind::AnnotationKind as Kind};
use pdf_graphics::{color::Color, transform::Transform};
use serde::Serialize;

/// Ready-to-assign CSS for one annotation container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct WebCss {
    /// `matrix(a,b,c,d,e,f)` from annotation-local top-left units to container CSS pixels,
    /// honoring the annotation's no-zoom and no-rotate flags.
    pub transform: String,
    /// Text or stroke color.
    pub color: String,
    /// Interior color, or `transparent`.
    pub background: String,
    /// Border shorthand for kinds without vector shapes; empty for none.
    pub border: String,
}

/// One projected entry plus its DOM placement.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct WebAnnotationEntry {
    /// The backend-neutral entry.
    #[serde(flatten)]
    pub entry: AnnotationEntry,
    /// CSS derived from `entry.style`, `entry.transform`, and the viewport.
    pub css: WebCss,
}

impl WebAnnotationEntry {
    /// Places `entry` through `device_to_css` and formats its style for the DOM.
    pub fn new(entry: AnnotationEntry, device_to_css: &Transform) -> Self {
        let css = WebCss {
            transform: css_matrix(&entry, device_to_css),
            color: css_color(Some(entry.style.color)),
            background: css_color(entry.style.background),
            border: css_border(&entry),
        };
        Self { entry, css }
    }
}

/// Kinds whose outline comes from `shapes` rather than a CSS border.
fn drawn_from_shapes(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Ink(_)
            | Kind::Highlight(_)
            | Kind::Underline(_)
            | Kind::Squiggly(_)
            | Kind::StrikeOut(_)
            | Kind::Line(_)
            | Kind::Square(_)
            | Kind::Circle(_)
            | Kind::Polygon(_)
            | Kind::PolyLine(_)
    )
}

fn css_border(entry: &AnnotationEntry) -> String {
    let style = &entry.style;
    if drawn_from_shapes(&entry.content) || style.border_width <= 0.0 {
        return String::new();
    }
    let line = if style.dash.is_empty() {
        "solid"
    } else {
        "dashed"
    };
    format!(
        "{}px {line} {}",
        style.border_width,
        css_color(Some(style.border_color.unwrap_or(style.color)))
    )
}

/// Formats an opaque color as `rgb(r g b)`; `None` is `transparent`.
pub fn css_color(color: Option<Color>) -> String {
    let Some(color) = color else {
        return "transparent".into();
    };
    let channel = |v: f32| (v * 255.0).round().to_u8().unwrap_or(0);
    format!(
        "rgb({} {} {})",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

/// Local-to-CSS matrix; no-zoom keeps unit scale and no-rotate drops rotation and skew.
fn css_matrix(entry: &AnnotationEntry, device_to_css: &Transform) -> String {
    let d = [
        device_to_css.sx,
        device_to_css.ky,
        device_to_css.kx,
        device_to_css.sy,
        device_to_css.tx,
        device_to_css.ty,
    ]
    .map(f64::from);
    let t = entry.transform;
    // device_to_css × transform, in CSS `matrix(a,b,c,d,e,f)` component order.
    let [mut a, mut b, mut c, mut dd] = [
        d[0] * t[0] + d[2] * t[1],
        d[1] * t[0] + d[3] * t[1],
        d[0] * t[2] + d[2] * t[3],
        d[1] * t[2] + d[3] * t[3],
    ];
    let e = d[0] * t[4] + d[2] * t[5] + d[4];
    let f = d[1] * t[4] + d[3] * t[5] + d[5];
    if entry.no_zoom {
        let (x, y) = (a.hypot(b), c.hypot(dd));
        if x > 0.0 {
            a /= x;
            b /= x;
        }
        if y > 0.0 {
            c /= y;
            dd /= y;
        }
    }
    if entry.no_rotate {
        let (x, y) = (a.hypot(b), c.hypot(dd));
        a = x;
        b = 0.0;
        c = 0.0;
        dd = y;
    }
    format!("matrix({a},{b},{c},{dd},{e},{f})")
}
