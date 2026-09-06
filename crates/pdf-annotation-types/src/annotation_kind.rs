use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    AnnotationError, CaretAnnotation, CircleAnnotation, FileAttachmentAnnotation,
    FreeTextAnnotation, HighlightAnnotation, InkAnnotation, LineAnnotation, LinkAnnotation,
    MovieAnnotation, PolyLineAnnotation, PolygonAnnotation, PopupAnnotation, PrinterMarkAnnotation,
    ScreenAnnotation, SoundAnnotation, SquareAnnotation, SquigglyAnnotation, StampAnnotation,
    StrikeOutAnnotation, TextAnnotation, ThreeDAnnotation, TrapNetAnnotation, UnderlineAnnotation,
    WatermarkAnnotation, WidgetAnnotation,
};

/// The PDF 1.7 annotation subtype.
#[allow(clippy::large_enum_variant)]
pub enum AnnotationKind {
    /// A text annotation.
    Text(TextAnnotation),
    /// A link annotation.
    Link(LinkAnnotation),
    /// A free text annotation.
    FreeText(FreeTextAnnotation),
    /// A line annotation.
    Line(LineAnnotation),
    /// A square annotation.
    Square(SquareAnnotation),
    /// A circle annotation.
    Circle(CircleAnnotation),
    /// A polygon annotation.
    Polygon(PolygonAnnotation),
    /// A polyline annotation.
    PolyLine(PolyLineAnnotation),
    /// A highlight annotation.
    Highlight(HighlightAnnotation),
    /// An underline annotation.
    Underline(UnderlineAnnotation),
    /// A squiggly annotation.
    Squiggly(SquigglyAnnotation),
    /// A strikeout annotation.
    StrikeOut(StrikeOutAnnotation),
    /// A stamp annotation.
    Stamp(StampAnnotation),
    /// A caret annotation.
    Caret(CaretAnnotation),
    /// An ink annotation.
    Ink(InkAnnotation),
    /// A popup annotation.
    Popup(PopupAnnotation),
    /// A file attachment annotation.
    FileAttachment(FileAttachmentAnnotation),
    /// A sound annotation.
    Sound(SoundAnnotation),
    /// A movie annotation.
    Movie(MovieAnnotation),
    /// A widget annotation.
    Widget(WidgetAnnotation),
    /// A screen annotation.
    Screen(ScreenAnnotation),
    /// A printer mark annotation.
    PrinterMark(PrinterMarkAnnotation),
    /// A trap network annotation.
    TrapNet(TrapNetAnnotation),
    /// A watermark annotation.
    Watermark(WatermarkAnnotation),
    /// A 3D annotation.
    ThreeD(ThreeDAnnotation),
    /// A vendor or future annotation subtype not covered by PDF 1.7 base types.
    Unknown { subtype: Vec<u8> },
}

impl AnnotationKind {
    /// Returns whether this annotation kind supports dragging.
    #[must_use]
    pub fn is_draggable(&self) -> bool {
        matches!(
            self,
            Self::FreeText(_)
                | Self::Text(_)
                | Self::Stamp(_)
                | Self::Line(_)
                | Self::Square(_)
                | Self::Circle(_)
                | Self::Polygon(_)
                | Self::PolyLine(_)
                | Self::Ink(_)
        )
    }

    pub(crate) fn from_dictionary(
        subtype: &[u8],
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(match subtype {
            b"Text" => Self::Text(TextAnnotation::from_dictionary(dictionary, objects)?),
            b"Link" => Self::Link(LinkAnnotation::from_dictionary(dictionary, objects)?),
            b"FreeText" => {
                Self::FreeText(FreeTextAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"Line" => Self::Line(LineAnnotation::from_dictionary(dictionary, objects)?),
            b"Square" => Self::Square(SquareAnnotation::from_dictionary(dictionary, objects)?),
            b"Circle" => Self::Circle(CircleAnnotation::from_dictionary(dictionary, objects)?),
            b"Polygon" => Self::Polygon(PolygonAnnotation::from_dictionary(dictionary, objects)?),
            b"PolyLine" => {
                Self::PolyLine(PolyLineAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"Highlight" => {
                Self::Highlight(HighlightAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"Underline" => {
                Self::Underline(UnderlineAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"Squiggly" => {
                Self::Squiggly(SquigglyAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"StrikeOut" => {
                Self::StrikeOut(StrikeOutAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"Stamp" => Self::Stamp(StampAnnotation::from_dictionary(dictionary, objects)?),
            b"Caret" => Self::Caret(CaretAnnotation::from_dictionary(dictionary, objects)?),
            b"Ink" => Self::Ink(InkAnnotation::from_dictionary(dictionary, objects)?),
            b"Popup" => Self::Popup(PopupAnnotation::from_dictionary(dictionary, objects)?),
            b"FileAttachment" => Self::FileAttachment(FileAttachmentAnnotation::from_dictionary(
                dictionary, objects,
            )?),
            b"Sound" => Self::Sound(SoundAnnotation::from_dictionary(dictionary, objects)?),
            b"Movie" => Self::Movie(MovieAnnotation::from_dictionary(dictionary, objects)?),
            b"Widget" => Self::Widget(WidgetAnnotation::from_dictionary(dictionary, objects)?),
            b"Screen" => Self::Screen(ScreenAnnotation::from_dictionary(dictionary, objects)?),
            b"PrinterMark" => {
                Self::PrinterMark(PrinterMarkAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"TrapNet" => Self::TrapNet(TrapNetAnnotation::from_dictionary(dictionary, objects)?),
            b"Watermark" => {
                Self::Watermark(WatermarkAnnotation::from_dictionary(dictionary, objects)?)
            }
            b"3D" => Self::ThreeD(
                ThreeDAnnotation::from_dictionary(dictionary, objects)?
                    .ok_or(AnnotationError::MissingEntry { entry: b"3D" })?,
            ),
            _ => Self::Unknown {
                subtype: subtype.to_vec(),
            },
        })
    }
}
