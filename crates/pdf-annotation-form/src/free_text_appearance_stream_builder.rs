//! Emits the normal appearance stream for generated free-text annotations.

use std::rc::Rc;

use pdf_annotation_types::{AppearanceDictionary, AppearanceField};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::{
    PdfTextItem,
    color_operators::{SetRGBFill, SetRGBStroke},
    graphics_state_operators::{RestoreGraphicsState, SaveGraphicsState, SetLineWidth},
    path_operators::Rectangle,
    path_paint_operators::{FillPathNonZero, StrokePath},
    text_object_operators::{BeginText, EndText},
    text_positioning_operators::SetTextMatrix,
    text_showing_operators::ShowText,
    text_state_operators::SetFont,
    variants::PdfOperatorVariant,
};
use pdf_graphics::{color::Color, rect::Rect};
use pdf_resources::{form::FormXObject, resource::Resource, resources::Resources};

use crate::{FreeTextStyle, free_text_layout::FreeTextLayout};

/// Builds one self-contained normal appearance stream from a completed layout.
pub(super) struct FreeTextAppearanceStreamBuilder<'a> {
    /// Validated, encoded, and wrapped text used for measurement and rendering.
    layout: FreeTextLayout<'a>,
    /// Bounding box of the appearance in annotation-local coordinates.
    bounds: Rect,
    /// Validated style shared with the layout.
    style: &'a FreeTextStyle,
    /// Operators accumulated in PDF execution order.
    operators: Vec<PdfOperatorVariant>,
}

impl<'a> FreeTextAppearanceStreamBuilder<'a> {
    /// Creates an empty builder and opens a balanced PDF graphics-state scope.
    pub(super) fn new(layout: FreeTextLayout<'a>, bounds: Rect, style: &'a FreeTextStyle) -> Self {
        Self {
            layout,
            bounds,
            style,
            operators: vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)],
        }
    }

    /// Emits every visual layer and packages the resulting Form XObject.
    pub(super) fn build(mut self) -> AppearanceDictionary {
        self.paint_background();
        self.paint_border();
        self.paint_text();
        self.close_graphics_state();
        self.into_appearance_dictionary()
    }

    /// Emits the optional background as a filled annotation-sized rectangle.
    fn paint_background(&mut self) {
        let Some(background) = self.style.background_color else {
            return;
        };
        self.operators.extend([
            Self::fill_color(background),
            PdfOperatorVariant::Rectangle(Rectangle::new(
                0.0,
                0.0,
                self.bounds.width(),
                self.bounds.height(),
            )),
            PdfOperatorVariant::FillPathNonZero(FillPathNonZero),
        ]);
    }

    /// Emits the optional border inside the appearance bounds.
    fn paint_border(&mut self) {
        let Some(border) = self.style.border else {
            return;
        };
        let inset = border.width / 2.0;
        self.operators.extend([
            PdfOperatorVariant::SetRGBStroke(SetRGBStroke::new(
                border.color.r,
                border.color.g,
                border.color.b,
            )),
            PdfOperatorVariant::SetLineWidth(SetLineWidth::new(border.width)),
            PdfOperatorVariant::Rectangle(Rectangle::new(
                inset,
                inset,
                self.bounds.width() - border.width,
                self.bounds.height() - border.width,
            )),
            PdfOperatorVariant::StrokePath(StrokePath),
        ]);
    }

    /// Emits a text object containing every wrapped line in visual order.
    fn paint_text(&mut self) {
        self.open_text_object();
        let mut baseline = self.bounds.height() - self.style.insets.top - self.style.font_size;
        for line in self.layout.lines() {
            let line_width = self.layout.line_width(line);
            self.operators.extend([
                PdfOperatorVariant::SetTextMatrix(SetTextMatrix::new([
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    self.layout.line_x(self.bounds.width(), line_width),
                    baseline,
                ])),
                PdfOperatorVariant::ShowText(ShowText::new(PdfTextItem::Text(
                    line.bytes().to_owned(),
                ))),
            ]);
            baseline -= self.style.line_height;
        }
        self.operators.push(PdfOperatorVariant::EndText(EndText));
    }

    /// Selects the text color and font before line-positioning operators run.
    fn open_text_object(&mut self) {
        self.operators.extend([
            Self::fill_color(self.style.text_color),
            PdfOperatorVariant::BeginText(BeginText),
            PdfOperatorVariant::SetFont(SetFont::new(
                self.style.font.resource_name.clone(),
                self.style.font_size,
            )),
        ]);
    }

    /// Closes the graphics-state scope opened by [`Self::new`].
    fn close_graphics_state(&mut self) {
        self.operators
            .push(PdfOperatorVariant::RestoreGraphicsState(
                RestoreGraphicsState,
            ));
    }

    /// Converts an RGB color into a PDF nonstroking-color operator.
    fn fill_color(color: Color) -> PdfOperatorVariant {
        PdfOperatorVariant::SetRGBFill(SetRGBFill::new(color.r, color.g, color.b))
    }

    /// Consumes the builder and wraps its stream as the normal appearance.
    fn into_appearance_dictionary(self) -> AppearanceDictionary {
        let Self {
            layout,
            bounds,
            style,
            operators,
        } = self;
        let mut resources = Resources::default();
        resources.fonts.insert(
            style.font.resource_name.clone(),
            Resource::Font {
                font: Rc::new(layout.into_font()),
                resources: None,
            },
        );
        let form = FormXObject {
            bbox: Rect::new(bounds.width(), bounds.height()),
            matrix: None,
            resources: Some(Rc::new(resources)),
            content_stream: ContentStream { operators, id: 0 },
        };
        AppearanceDictionary {
            normal: Some(AppearanceField::Stream(Box::new(form))),
            rollover: None,
            down: None,
        }
    }
}
