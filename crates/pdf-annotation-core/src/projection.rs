//! Project borrowed Core values into viewport entries without touching PDF objects.
use crate::{
    appearance_paint::PaintedAppearance,
    engine::DocumentView,
    entry::AnnotationEntry,
    fields::{Field, FieldKind},
    import::LayoutHint,
    layer_error::AnnotationLayerResult,
    models::{Annotation, Revision},
    pdf_data::AnnotationAction,
    shapes,
};
use pdf_graphics::{point::Point, viewport::PageViewport};
use pdf_optional_content::OptionalContentState;

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
    let field = annotation.widget_field(view)?;
    let style = annotation.style()?;
    let action = annotation.action();
    let visible = annotation.is_displayed(optional_content);
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
        control: field.and_then(|field| annotation.widget_control(field)),
        artwork,
        shapes: shapes::shapes(annotation, &bounds, &style),
        style,
    })
}
