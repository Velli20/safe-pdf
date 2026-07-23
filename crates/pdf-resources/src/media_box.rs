use pdf_graphics::rect::Rect;
use pdf_object::error::ObjectError;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

/// Reads the page boundaries from a PDF document.
#[derive(Default, Debug, Clone)]
pub struct MediaBox;

impl MediaBox {
    const KEY: &'static str = "MediaBox";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Rect>, ObjectError> {
        let Some(media_box_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        // PDF MediaBox is an array of four numbers: [LLx, LLy, URx, URy]
        let rect = Rect::from(media_box_obj.try_array_of::<f32, 4>(objects)?);

        Ok(Some(rect))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn missing_media_box_returns_none() {
        let dictionary = Dictionary::new(BTreeMap::new());

        assert_eq!(
            MediaBox::from_dictionary(&dictionary, &PassthroughResolver)
                .expect("missing MediaBox should parse"),
            None
        );
    }

    #[test]
    fn media_box_is_parsed_as_rect() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "MediaBox".to_owned(),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(10),
                ObjectVariant::Integer(20),
                ObjectVariant::Integer(210),
                ObjectVariant::Integer(320),
            ]),
        )]));

        let media_box = MediaBox::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("MediaBox should parse")
            .expect("MediaBox should be present");

        assert_eq!(
            media_box,
            Rect {
                left: 10.0,
                top: 20.0,
                right: 210.0,
                bottom: 320.0,
            }
        );
        assert_eq!(media_box.width(), 200.0);
        assert_eq!(media_box.height(), 300.0);
    }

    #[test]
    fn malformed_media_box_returns_error() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "MediaBox".to_owned(),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(10),
                ObjectVariant::Integer(20),
                ObjectVariant::Integer(210),
            ]),
        )]));

        assert!(
            MediaBox::from_dictionary(&dictionary, &PassthroughResolver).is_err(),
            "MediaBox arrays must contain four numbers"
        );
    }
}
