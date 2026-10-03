//! Project borrowed Core values into viewport entries without touching PDF objects.
use crate::{
    engine::DocumentView,
    entry::AnnotationEntry,
    fields::{Field, FieldKind},
    import::LayoutHint,
    kind::AnnotationKind as Kind,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::{Annotation, Revision},
    optional_content::OptionalContentState,
    pdf_data::AnnotationAction,
    shapes, widget_control,
};
use pdf_graphics::{
    point::Point,
    rect::Rect,
    viewport::{PageViewport, ViewportError},
};

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

/// Device-space interaction rectangles: one per quad, else the payload bounds.
fn device_regions(
    annotation: &Annotation,
    bounds: &Rect<f64>,
    viewport: &PageViewport,
) -> AnnotationLayerResult<Vec<[f64; 4]>> {
    let regions = match annotation.content.quads().filter(|q| !q.is_empty()) {
        Some(quads) => quads
            .iter()
            .map(|quad| viewport.map_quad(quad))
            .collect::<Result<Vec<_>, _>>()?,
        None => {
            let rect = bounds.to_f32().ok_or(ViewportError::Bounds)?;
            vec![viewport.map_rect(&rect)?.into()]
        }
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
    viewport: &PageViewport,
    modified_revision: Revision,
    has_popup: bool,
) -> AnnotationLayerResult<AnnotationEntry> {
    let bounds = annotation
        .content
        .bounds(hint.and_then(LayoutHint::source_layout).as_ref())?;
    let field = widget_field(annotation, view)?;
    let style = annotation.style()?;
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
        text: annotation.text()?,
        subtype: annotation.content.subtype_name().into_owned(),
        unsupported: unsupported_reason(annotation, field),
        bounds: device_regions(annotation, &bounds, viewport)?,
        origin: [bounds.left, bounds.top],
        size: [bounds.width(), bounds.height()],
        transform: viewport
            .local_frame(
                Point::new(bounds.left, bounds.bottom)
                    .to_f32()
                    .ok_or(ViewportError::Bounds)?,
            )?
            .to_row()
            .map(f64::from),
        no_zoom: annotation.metadata.no_zoom(),
        no_rotate: annotation.metadata.no_rotate(),
        draggable: annotation.can_translate(),
        editable: annotation.editable(field),
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
