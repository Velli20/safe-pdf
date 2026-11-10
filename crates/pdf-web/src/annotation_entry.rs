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
    /// honoring the annotation's no-rotate flag. No-zoom annotations scale with the page
    /// like any other annotation.
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
            background: if painted_on_canvas(&entry) {
                css_color(None)
            } else {
                css_color(entry.style.background)
            },
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

/// Whether `entry` is painted from its appearance on the page canvas, which already draws
/// any border and background: supported caret and unknown entries, and widgets without a
/// control or placeholder.
fn painted_on_canvas(entry: &AnnotationEntry) -> bool {
    entry.unsupported.is_none()
        && match entry.content {
            Kind::Caret(_) | Kind::Unknown(_) => true,
            Kind::Widget(_) => entry.control.is_none(),
            _ => false,
        }
}

/// The CSS border for `entry`, or empty when nothing draws one: a link is only an
/// interaction region, and canvas-painted or shape-drawn kinds carry their own outline.
fn css_border(entry: &AnnotationEntry) -> String {
    let style = &entry.style;
    if matches!(entry.content, Kind::Link(_))
        || painted_on_canvas(entry)
        || drawn_from_shapes(&entry.content)
        || style.border_width <= 0.0
    {
        return String::new();
    }
    let line = if style.dash.as_ref().is_some_and(|dash| !dash.is_empty()) {
        "dashed"
    } else {
        "solid"
    };
    format!(
        "{}px {line} {}",
        style.border_width,
        css_color(Some(style.border_color.unwrap_or(style.color)))
    )
}

/// Formats a color as `rgb(r g b / a)`; `None` is `transparent`.
pub fn css_color(color: Option<Color>) -> String {
    let Some(color) = color else {
        return "transparent".into();
    };
    let channel = |v: f32| (v * 255.0).round().to_u8().unwrap_or(0);
    format!(
        "rgb({} {} {} / {})",
        channel(color.r),
        channel(color.g),
        channel(color.b),
        color.a
    )
}

/// Local-to-CSS matrix; no-rotate drops rotation and skew.
fn css_matrix(entry: &AnnotationEntry, device_to_css: &Transform) -> String {
    // device_to_css × transform, in CSS `matrix(a,b,c,d,e,f)` component order.
    let Transform {
        sx: mut a,
        ky: mut b,
        kx: mut c,
        sy: mut dd,
        tx: e,
        ty: f,
    } = device_to_css.post_concatenated(&entry.transform);
    if entry.no_rotate {
        let (x, y) = (a.hypot(b), c.hypot(dd));
        a = x;
        b = 0.0;
        c = 0.0;
        dd = y;
    }
    format!("matrix({a},{b},{c},{dd},{e},{f})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_annotation_core::{
        models::{AnnotationId, Revision},
        style::ResolvedStyle,
        subtypes::Link,
    };

    #[test]
    fn link_has_no_css_border() {
        let style = ResolvedStyle {
            border_color: Some(Color::from_rgb(0.0, 0.0, 1.0)),
            border_width: 112.0,
            ..ResolvedStyle::default()
        };
        let link = Link {
            bounds: Default::default(),
            quads: None,
            highlight_mode: None,
            destination: None,
            action: None,
            style: style.clone(),
        };
        let entry = AnnotationEntry {
            id: AnnotationId(1),
            page: 0,
            content: Kind::Link(Box::new(link)),
            style,
            action: None,
            triggers: Vec::new(),
            text: String::new(),
            subtype: "Link".into(),
            unsupported: None,
            origin: Default::default(),
            size: Default::default(),
            transform: Transform::identity(),
            no_zoom: false,
            no_rotate: false,
            draggable: false,
            editable: false,
            visible: true,
            interactive: true,
            modified_revision: Revision(0),
            label: String::new(),
            href: None,
            has_popup: false,
            control: None,
            artwork: false,
            shapes: Vec::new(),
        };
        let web = WebAnnotationEntry::new(entry, &Transform::identity());
        assert!(web.css.border.is_empty());
    }
}
