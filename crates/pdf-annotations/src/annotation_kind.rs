use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

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
#[derive(Debug, Clone, PartialEq)]
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
    Unknown { subtype: String },
}

impl AnnotationKind {
    pub(crate) fn from_dictionary(
        subtype: &str,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(match subtype {
            "Text" => Self::Text(TextAnnotation::from_dictionary(dictionary, objects)?),
            "Link" => Self::Link(LinkAnnotation::from_dictionary(dictionary, objects)?),
            "FreeText" => Self::FreeText(FreeTextAnnotation::from_dictionary(dictionary, objects)?),
            "Line" => Self::Line(LineAnnotation::from_dictionary(dictionary, objects)?),
            "Square" => Self::Square(SquareAnnotation::from_dictionary(dictionary, objects)?),
            "Circle" => Self::Circle(CircleAnnotation::from_dictionary(dictionary, objects)?),
            "Polygon" => Self::Polygon(PolygonAnnotation::from_dictionary(dictionary, objects)?),
            "PolyLine" => Self::PolyLine(PolyLineAnnotation::from_dictionary(dictionary, objects)?),
            "Highlight" => {
                Self::Highlight(HighlightAnnotation::from_dictionary(dictionary, objects)?)
            }
            "Underline" => {
                Self::Underline(UnderlineAnnotation::from_dictionary(dictionary, objects)?)
            }
            "Squiggly" => Self::Squiggly(SquigglyAnnotation::from_dictionary(dictionary, objects)?),
            "StrikeOut" => {
                Self::StrikeOut(StrikeOutAnnotation::from_dictionary(dictionary, objects)?)
            }
            "Stamp" => Self::Stamp(StampAnnotation::from_dictionary(dictionary, objects)?),
            "Caret" => Self::Caret(CaretAnnotation::from_dictionary(dictionary, objects)?),
            "Ink" => Self::Ink(InkAnnotation::from_dictionary(dictionary, objects)?),
            "Popup" => Self::Popup(PopupAnnotation::from_dictionary(dictionary, objects)?),
            "FileAttachment" => Self::FileAttachment(FileAttachmentAnnotation::from_dictionary(
                dictionary, objects,
            )?),
            "Sound" => Self::Sound(SoundAnnotation::from_dictionary(dictionary, objects)?),
            "Movie" => Self::Movie(MovieAnnotation::from_dictionary(dictionary, objects)?),
            "Widget" => Self::Widget(WidgetAnnotation::from_dictionary(dictionary, objects)?),
            "Screen" => Self::Screen(ScreenAnnotation::from_dictionary(dictionary, objects)?),
            "PrinterMark" => {
                Self::PrinterMark(PrinterMarkAnnotation::from_dictionary(dictionary, objects)?)
            }
            "TrapNet" => Self::TrapNet(TrapNetAnnotation::from_dictionary(dictionary, objects)?),
            "Watermark" => {
                Self::Watermark(WatermarkAnnotation::from_dictionary(dictionary, objects)?)
            }
            "3D" => Self::ThreeD(
                ThreeDAnnotation::from_dictionary(dictionary, objects)?
                    .ok_or(AnnotationError::MissingEntry { entry: "3D" })?,
            ),
            _ => Self::Unknown {
                subtype: subtype.to_owned(),
            },
        })
    }
}
