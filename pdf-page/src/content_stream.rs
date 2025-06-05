use pdf_object::{
    ObjectVariant, Value, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use pdf_operator::pdf_operator::PdfOperatorVariant;

use crate::error::PageError;

pub struct ContentStream {
    pub operations: Vec<PdfOperatorVariant>,
}

impl ContentStream {
    pub fn from(input: &[u8]) -> Result<Self, PageError> {
        let operations = PdfOperatorVariant::from(input)?;
        Ok(Self { operations })
    }
}

impl FromDictionary for ContentStream {
    const KEY: &'static str = "Contents";
    type ResultType = ContentStream;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, PageError> {
        // Get the optional `/Contents` entry from the page dictionary.
        let contents = if let Some(contents) = dictionary.get_object(Self::KEY) {
            // The `/Contents` entry can be either:
            // 1. A direct stream object.
            // 2. An array of direct stream objects.
            // 3. An indirect reference to a stream object.
            // 4. An indirect reference to an array of stream objects.
            if let ObjectVariant::Reference(object_number) = contents {
                // The object is an indirect reference; resolve it from the `objects` collection.
                objects
                    .get(*object_number)
                    .ok_or(PageError::MissingContent)?
            } else {
                // The object is directly available (not an indirect reference that needs resolving here).
                contents.clone()
            }
        } else {
            return Err(PageError::MissingContent);
        };

        if let ObjectVariant::IndirectObject(s) = &contents {
            if let Some(Value::Array(array)) = &s.object {
                for obj in array.0.iter() {
                    if let Value::IndirectObject(s) = obj {
                        if let Some(ss) = objects.get(s.object_number()) {
                            if let ObjectVariant::Stream(s) = ss {
                                return ContentStream::from(s.data.as_slice());
                            }
                        }
                    }
                }
            }
        } else if let ObjectVariant::Stream(s) = &contents {
            return ContentStream::from(s.data.as_slice());
        }

        Err(PageError::MissingContent)
    }
}
