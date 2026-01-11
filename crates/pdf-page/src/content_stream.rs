use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContentStreamReadError {
    #[error("Unsupported entry type for Content Stream: '{found_type}'")]
    UnsupportedEntryType { found_type: &'static str },
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

/// Represents the content stream of a PDF page, containing a sequence
/// of drawing operators.
pub struct ContentStream {
    /// Flat, ordered list of all PDF content stream operators that belong to a page.
    pub operations: Vec<PdfOperatorVariant>,
}

// Helper function to process an array whose elements should be streams or references to streams
fn process_content_stream_array(
    array: &[ObjectVariant],
    objects: &ObjectCollection,
) -> Result<Vec<PdfOperatorVariant>, ContentStreamReadError> {
    let mut concatenated_ops = Vec::new();
    for value_in_array in array.iter() {
        let stream = objects.resolve_stream(value_in_array)?;
        let data = stream.data()?;
        let stream_ops = PdfOperatorVariant::from(&data)?;
        concatenated_ops.extend(stream_ops);
    }
    Ok(concatenated_ops)
}

impl FromDictionary for ContentStream {
    const KEY: &'static str = "Contents";
    type ResultType = Option<ContentStream>;
    type ErrorType = ContentStreamReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Get the optional `/Contents` entry from the page dictionary.
        let Some(contents) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        // Resolve the /Contents entry if it's an indirect reference.
        let contents = objects.resolve_object(contents)?;

        // Process the resolved /Contents object.
        // It should be a Stream, an Array, or an IndirectObject whose payload is one of these.
        let operations = match &contents {
            ObjectVariant::Stream(stream) => {
                let data = stream.data()?;
                PdfOperatorVariant::from(&data)?
            }
            ObjectVariant::Array(array_obj) => {
                // The /Contents entry is an array of streams.
                process_content_stream_array(array_obj, objects)?
            }
            ObjectVariant::IndirectObject(s) => match &s.object {
                Some(ObjectVariant::Stream(stream)) => {
                    let stream = stream.data()?;
                    PdfOperatorVariant::from(&stream)?
                }
                Some(ObjectVariant::Array(array_val)) => {
                    process_content_stream_array(array_val, objects)?
                }
                Some(other) => {
                    return Err(ContentStreamReadError::UnsupportedEntryType {
                        found_type: other.name(),
                    });
                }
                None => return Ok(None),
            },

            other => {
                return Err(ContentStreamReadError::UnsupportedEntryType {
                    found_type: other.name(),
                });
            }
        };

        Ok(Some(ContentStream { operations }))
    }
}
