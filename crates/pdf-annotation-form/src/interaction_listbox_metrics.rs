//! Listbox row metrics derived from widget appearance metadata.

use pdf_annotation_types::WidgetAnnotation;

use crate::interaction_viewport::AnnotationViewport;

/// Default listbox row height when no usable font size is declared.
const DEFAULT_LISTBOX_ROW_HEIGHT: f32 = 12.0;
/// Line-height multiplier applied to a listbox appearance font size.
const LISTBOX_LINE_HEIGHT_RATIO: f32 = 1.2;

/// Calculates listbox row metrics from widget appearance metadata.
pub(super) struct ListboxMetrics<'a> {
    /// Widget whose default appearance supplies the font size.
    widget: &'a WidgetAnnotation,
}

impl<'a> ListboxMetrics<'a> {
    /// Creates a metrics calculator for one widget.
    pub(super) const fn new(widget: &'a WidgetAnnotation) -> Self {
        Self { widget }
    }

    /// Returns a validated row height in device units.
    pub(super) fn device_row_height(&self, viewport: AnnotationViewport) -> Option<f32> {
        viewport.map_page_height(self.page_row_height())
    }

    /// Returns the page-space row height declared by the default appearance.
    fn page_row_height(&self) -> f32 {
        self.font_size().map_or(DEFAULT_LISTBOX_ROW_HEIGHT, |size| {
            size * LISTBOX_LINE_HEIGHT_RATIO
        })
    }

    /// Parses the last valid font size preceding a `Tf` operator.
    fn font_size(&self) -> Option<f32> {
        let appearance = self.widget.default_appearance.as_deref()?;
        let mut previous = None;
        let mut font_size = None;
        for token in appearance
            .split(u8::is_ascii_whitespace)
            .filter(|token| !token.is_empty())
        {
            if token == b"Tf"
                && let Some(size) = previous
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .and_then(|value| value.parse::<f32>().ok())
                    .filter(|size| size.is_finite() && *size > 0.0)
            {
                font_size = Some(size);
            }
            previous = Some(token);
        }
        font_size
    }
}
