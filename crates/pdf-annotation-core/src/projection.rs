//! Project borrowed Core values into viewport entries without touching PDF objects.
use crate::{
    engine::DocumentView,
    entry::AnnotationEntry,
    fields::{Field, FieldKind},
    import::LayoutHint,
    kind::AnnotationKind as Kind,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::{Annotation, Quad, Revision},
    optional_content::OptionalContentState,
    pdf_data::{AnnotationAction, SourceAnnotation},
    shapes,
    style::{ResolvedStyle, resolve_source_style},
    widget_control,
};
use num_traits::ToPrimitive;
use pdf_graphics::{
    point::Point,
    rect::Rect,
    transform::{Transform, TransformError},
};

/// Style resolved from the retained source; default for host-created annotations.
fn source_style(annotation: &Annotation) -> AnnotationLayerResult<ResolvedStyle> {
    annotation
        .source()
        .map(resolve_source_style)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(Into::into)
}

/// Decoded source `/Contents` for kinds whose Core payload carries no text.
fn source_text(annotation: &Annotation) -> AnnotationLayerResult<String> {
    annotation
        .source()
        .map(SourceAnnotation::contents)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(Into::into)
}

/// Live payload style; free text overlays its own style on the retained source.
fn style(annotation: &Annotation) -> AnnotationLayerResult<ResolvedStyle> {
    match &annotation.content {
        Kind::FreeText(v) => {
            let mut style = source_style(annotation)?;
            v.style.apply_to(&mut style);
            Ok(style)
        }
        content => content
            .live_style()
            .map_or_else(|| source_style(annotation), Ok),
    }
}

/// Live payload text, else the retained source contents.
fn text(annotation: &Annotation) -> AnnotationLayerResult<String> {
    match annotation.content.text() {
        Some(text) => Ok(text.to_owned()),
        None => source_text(annotation),
    }
}

/// The shared field a widget is bound to; `None` for other kinds.
fn widget_field<'a>(
    annotation: &Annotation,
    view: &DocumentView<'a>,
) -> AnnotationLayerResult<Option<&'a Field>> {
    let Kind::Widget(widget) = &annotation.content else {
        return Ok(None);
    };
    view.field(widget.binding.field())
        .map(Some)
        .ok_or(AnnotationLayerError::InvalidInput("widget field"))
}

/// Why the host should show a placeholder: the payload's reason, else unsupported field semantics.
fn unsupported_reason(annotation: &Annotation, field: Option<&Field>) -> Option<String> {
    annotation
        .content
        .unsupported_reason()
        .map(String::from)
        .or_else(|| {
            field
                .filter(|field| matches!(field.definition.content, FieldKind::Unsupported(_)))
                .map(|_| "Unsupported field semantics".into())
        })
}

/// Whether the host may edit the content: plain free text, or a widget's field value.
fn editable(annotation: &Annotation, field: Option<&Field>) -> bool {
    match (&annotation.content, field) {
        (Kind::FreeText(_), _) => annotation.can_edit_free_text(),
        (Kind::Widget(_), Some(field)) => annotation.can_edit_field(field),
        _ => false,
    }
}

/// Narrows an `f64` page coordinate to a finite `f32` page unit.
fn page_unit(value: f64) -> Result<f32, TransformError> {
    value
        .to_f32()
        .filter(|value| value.is_finite())
        .ok_or(TransformError::NonFinite)
}

/// Maps an `f64` page point into logical device coordinates using `f32` page units.
/// Rejects coordinates that cannot be represented as finite `f32` values.
fn device_point(
    page_to_device: &Transform,
    point: Point<f64>,
) -> AnnotationLayerResult<Point<f64>> {
    let mapped =
        page_to_device.try_map_point(Point::new(page_unit(point.x)?, page_unit(point.y)?))?;
    Ok(Point::new(f64::from(mapped.x), f64::from(mapped.y)))
}

/// Maps an `f64` page rectangle into normalized logical device bounds using `f32`
/// page units. Rejects edges that cannot be represented as finite `f32` values or
/// a rectangle without positive extent.
fn device_rect(page_to_device: &Transform, rect: &Rect<f64>) -> AnnotationLayerResult<Rect<f64>> {
    let rect = Rect {
        left: page_unit(rect.left)?,
        top: page_unit(rect.top)?,
        right: page_unit(rect.right)?,
        bottom: page_unit(rect.bottom)?,
    }
    .normalized();
    if !rect.is_valid() {
        return Err(AnnotationLayerError::InvalidInput("annotation bounds"));
    }
    for point in [
        Point::new(rect.left, rect.top),
        Point::new(rect.right, rect.top),
        Point::new(rect.right, rect.bottom),
        Point::new(rect.left, rect.bottom),
    ] {
        page_to_device.try_map_point(point)?;
    }
    let mapped = page_to_device.map_rect(&rect).normalized();
    Ok(Rect {
        left: f64::from(mapped.left),
        top: f64::from(mapped.top),
        right: f64::from(mapped.right),
        bottom: f64::from(mapped.bottom),
    })
}

/// Maps a y-down local frame anchored at page point `origin` into logical device
/// coordinates: local `(x, y)` is page `(origin.x + x, origin.y - y)`. Rejects an
/// origin without a finite `f32` representation or a nonfinite result.
fn local_to_device(
    page_to_device: &Transform,
    origin: Point<f64>,
) -> AnnotationLayerResult<Transform> {
    let local_to_page = Transform::from_row(
        1.0,
        0.0,
        0.0,
        -1.0,
        page_unit(origin.x)?,
        page_unit(origin.y)?,
    );
    let local_to_device = page_to_device.post_concatenated(&local_to_page);
    local_to_device.validate()?;
    Ok(local_to_device)
}

/// Device-space bounds enclosing one page-space quad.
fn quad_region(page_to_device: &Transform, quad: &Quad) -> AnnotationLayerResult<Rect<f64>> {
    let [a, b, c, d] = quad.corners;
    let device = Quad {
        corners: [
            device_point(page_to_device, a)?,
            device_point(page_to_device, b)?,
            device_point(page_to_device, c)?,
            device_point(page_to_device, d)?,
        ],
    };
    Ok(device.bounds())
}

/// Device-space interaction rectangles: one per quad, else the payload bounds.
fn device_regions(
    annotation: &Annotation,
    bounds: &Rect<f64>,
    page_to_device: &Transform,
) -> AnnotationLayerResult<Vec<[f64; 4]>> {
    let regions = match annotation.content.quads().filter(|q| !q.is_empty()) {
        Some(quads) => quads
            .iter()
            .map(|quad| quad_region(page_to_device, quad))
            .collect::<AnnotationLayerResult<Vec<_>>>()?,
        None => vec![device_rect(page_to_device, bounds)?],
    };
    regions
        .into_iter()
        .map(|region| {
            if !region.is_valid() {
                return Err(AnnotationLayerError::InvalidInput("device bounds"));
            }
            Ok([region.left, region.top, region.right, region.bottom])
        })
        .collect()
}

pub(crate) fn project(
    annotation: &Annotation,
    view: &DocumentView<'_>,
    optional_content: &OptionalContentState,
    hint: Option<&LayoutHint>,
    page_to_device: &Transform,
    modified_revision: Revision,
    has_popup: bool,
) -> AnnotationLayerResult<AnnotationEntry> {
    let bounds = annotation
        .content
        .bounds(hint.and_then(LayoutHint::source_layout).as_ref())?;
    let field = widget_field(annotation, view)?;
    let style = style(annotation)?;
    let action = annotation.action();
    // Optional content hides an annotation without altering its flags, so the two
    // conditions are independent: either one alone suppresses display.
    let visible = annotation.is_visible()
        && annotation
            .source()
            .and_then(|source| source.optional_content.as_ref())
            .is_none_or(|content| optional_content.is_visible(content));
    Ok(AnnotationEntry {
        id: annotation.id,
        page: annotation.page.0,
        content: annotation.content.clone(),
        href: action.as_ref().and_then(AnnotationAction::href),
        action,
        text: text(annotation)?,
        subtype: annotation.content.subtype_name().into_owned(),
        unsupported: unsupported_reason(annotation, field),
        bounds: device_regions(annotation, &bounds, page_to_device)?,
        origin: [bounds.left, bounds.top],
        size: [bounds.width(), bounds.height()],
        transform: local_to_device(page_to_device, Point::new(bounds.left, bounds.bottom))?
            .to_row()
            .map(f64::from),
        no_zoom: annotation.metadata.no_zoom(),
        no_rotate: annotation.metadata.no_rotate(),
        draggable: annotation.can_translate(),
        editable: editable(annotation, field),
        visible,
        interactive: visible,
        modified_revision,
        label: annotation.content.label(),
        has_popup,
        control: field.and_then(|field| widget_control::control(annotation, field)),
        shapes: shapes::shapes(annotation, &bounds, &style),
        style,
    })
}
