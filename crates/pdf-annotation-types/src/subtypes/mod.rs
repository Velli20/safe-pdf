mod caret;
mod circle;
mod file_attachment;
mod free_text;
mod highlight;
mod ink;
mod line;
mod link;
mod movie;
mod polygon;
mod polyline;
mod popup;
mod printer_mark;
mod screen;
mod sound;
mod square;
mod squiggly;
mod stamp;
mod strikeout;
mod text;
mod trap_net;
mod underline;
mod watermark;
mod widget;

pub use self::caret::CaretAnnotation;
pub use self::circle::CircleAnnotation;
pub use self::file_attachment::FileAttachmentAnnotation;
pub use self::free_text::{FreeTextAlignment, FreeTextAnnotation};
pub use self::highlight::HighlightAnnotation;
pub use self::ink::InkAnnotation;
pub use self::line::LineAnnotation;
pub use self::link::LinkAnnotation;
pub use self::movie::MovieAnnotation;
pub use self::polygon::PolygonAnnotation;
pub use self::polyline::PolyLineAnnotation;
pub use self::popup::PopupAnnotation;
pub use self::printer_mark::PrinterMarkAnnotation;
pub use self::screen::ScreenAnnotation;
pub use self::sound::SoundAnnotation;
pub use self::square::SquareAnnotation;
pub use self::squiggly::SquigglyAnnotation;
pub use self::stamp::StampAnnotation;
pub use self::strikeout::StrikeOutAnnotation;
pub use self::text::TextAnnotation;
pub use self::trap_net::TrapNetAnnotation;
pub use self::underline::UnderlineAnnotation;
pub use self::watermark::WatermarkAnnotation;
pub use self::widget::{WidgetAnnotation, WidgetChoiceOption, WidgetFieldFlags, WidgetFieldValue};

use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, LineEndingStyle, QuadPoints};

pub(crate) fn line_endings(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<[LineEndingStyle; 2]>, AnnotationError> {
    let Some(value) = dictionary.get(b"LE") else {
        return Ok(None);
    };

    let endings = value.try_array(objects)?;
    if endings.len() != 2 {
        return Err(AnnotationError::InvalidEntry {
            entry: b"LE",
            reason: format!("expected 2 line ending names, found {}", endings.len()),
        });
    }

    let mut parsed = [LineEndingStyle::None, LineEndingStyle::None];
    for (slot, item) in parsed.iter_mut().zip(endings.iter()) {
        *slot = LineEndingStyle::from(item.try_bytes(objects)?);
    }

    Ok(Some(parsed))
}

pub(crate) fn required_quad_points(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<QuadPoints, AnnotationError> {
    QuadPoints::from_dictionary(dictionary, objects)?.ok_or(AnnotationError::MissingEntry {
        entry: b"QuadPoints",
    })
}
