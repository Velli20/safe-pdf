//! Project borrowed Core values into viewport entries without touching PDF objects.
use crate::{
    appearance_paint::PaintedAppearance,
    engine::DocumentView,
    entry::AnnotationEntry,
    fields::{Field, FieldKind},
    import::LayoutHint,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::{Annotation, Revision},
    pdf_data::AnnotationAction,
    shapes, widget_control,
};
use pdf_graphics::{point::Point, rect::Rect, viewport::PageViewport};
use pdf_optional_content::OptionalContentState;

/// The shared field a widget is bound to; `None` for other kinds.
pub(crate) fn widget_field<'a>(
    annotation: &Annotation,
    view: &DocumentView<'a>,
) -> AnnotationLayerResult<Option<&'a Field>> {
    annotation
        .content
        .bound_field()
        .map(|id| {
            view.field(id)
                .ok_or(AnnotationLayerError::InvalidInput("widget field"))
        })
        .transpose()
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
    bounds: &Rect,
    viewport: &PageViewport,
) -> AnnotationLayerResult<Vec<Rect>> {
    let regions = match annotation.content.quads().filter(|q| !q.is_empty()) {
        Some(quads) => quads
            .iter()
            .map(|quad| viewport.map_quad(quad))
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![viewport.map_rect(bounds)?],
    };
    if !regions.iter().all(|region| region.is_valid()) {
        return Err(AnnotationLayerError::InvalidInput("device bounds"));
    }
    Ok(regions)
}

/// Whether `annotation` is shown under its flags and `optional_content`.
pub(crate) fn displayed(annotation: &Annotation, optional_content: &OptionalContentState) -> bool {
    // Optional content hides an annotation without altering its flags, so the two
    // conditions are independent: either one alone suppresses display.
    annotation.is_visible()
        && annotation
            .optional_content()
            .is_none_or(|content| optional_content.is_visible(content))
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
    let visible = displayed(annotation, optional_content);
    let artwork = PaintedAppearance::artwork(annotation, optional_content).is_some();
    Ok(AnnotationEntry {
        id: annotation.id,
        page: annotation.page.0,
        content: annotation.content.clone(),
        href: action.as_ref().and_then(AnnotationAction::href),
        action,
        triggers: annotation.triggers(),
        text: annotation.text()?,
        subtype: annotation.content.subtype_name().into_owned(),
        // Source artwork or an appearance painted on the page canvas shows the annotation
        // as authored, so no placeholder is needed.
        unsupported: unsupported_reason(annotation, field)
            .filter(|_| !artwork && !annotation.is_drawn_from_appearance()),
        bounds: device_regions(annotation, &bounds, viewport)?,
        origin: bounds.origin(),
        size: bounds.size(),
        transform: viewport.local_frame(Point::new(bounds.left, bounds.bottom))?,
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
        artwork,
        shapes: shapes::shapes(annotation, &bounds, &style),
        style,
    })
}
