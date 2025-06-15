use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    ObjectVariant, Value, array::Array, dictionary::Dictionary,
    object_collection::ObjectCollection, traits::FromDictionary,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContentStreamReadError {
    #[error("Failed to resolve foncontent stream object reference {obj_num}")]
    FailedResolveFontObjectReference { obj_num: i32 },
    #[error("Unsupported entry type for Content Stream: '{found_type}'")]
    UnsupportedEntryType { found_type: &'static str },

    #[error("Failed to resolve content stream object reference {obj_num}")]
    FailedToResolveReference { obj_num: i32 },
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Unsupported entry type in Content Stream array: '{found_type}'")]
    UnsupportedEntryTypeInArray { found_type: &'static str },
    #[error("Expected a Stream object in Content Stream array, found other type")]
    ExpectedStreamInArray,
    #[error(
        "Expected a Stream object after resolving an indirect reference from an IndirectObject payload, found '{found_type}'"
    )]
    ExpectedStreamAfterIndirectReference { found_type: &'static str },
    #[error("Unsupported type in IndirectObject payload for Content Stream: '{found_type}'")]
    UnsupportedTypeInIndirectObjectPayload { found_type: &'static str },
}

pub struct ContentStream {
    pub operations: Vec<PdfOperatorVariant>,
}

// Helper function to process an array whose elements should be streams or references to streams
fn process_content_stream_array(
    array: &Array,
    objects: &ObjectCollection,
) -> Result<Vec<PdfOperatorVariant>, ContentStreamReadError> {
    let mut concatenated_ops = Vec::new();
    for value_in_array in array.0.iter() {
        let stream_ops = match value_in_array {
            Value::IndirectObject(indirect_ref) => {
                let stream_obj = objects.get(indirect_ref.object_number()).ok_or_else(|| {
                    ContentStreamReadError::FailedToResolveReference {
                        obj_num: indirect_ref.object_number(),
                    }
                })?;
                // Expect this resolved object to be a stream
                if let ObjectVariant::Stream(s) = stream_obj {
                    PdfOperatorVariant::from(s.data.as_slice())?
                } else {
                    return Err(ContentStreamReadError::ExpectedStreamInArray);
                }
            }
            Value::Stream(direct_stream_val) => {
                PdfOperatorVariant::from(direct_stream_val.data.as_slice())?
            }
            other => {
                return Err(ContentStreamReadError::UnsupportedEntryTypeInArray {
                    found_type: other.name(),
                });
            }
        };
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
        let Some(contents) = dictionary.get_object(Self::KEY) else {
            return Ok(None);
        };

        // Resolve the /Contents entry if it's an indirect reference.
        let contents = match contents {
            ObjectVariant::Reference(num) => {
                // The object is an indirect reference; resolve it from the `objects` collection.
                objects.get(*num).ok_or(
                    ContentStreamReadError::FailedResolveFontObjectReference { obj_num: *num },
                )?
            }
            _ => contents.clone(),
        };

        // Process the resolved /Contents object.
        // It should be a Stream, an Array, or an IndirectObject whose payload is one of these.
        let operations = match &contents {
            ObjectVariant::Stream(s) => PdfOperatorVariant::from(s.data.as_slice())?,
            //ObjectVariant::Array(array_obj) => {
            //    // The /Contents entry is an array of streams.
            //    Self::process_content_stream_array(array_obj, objects)?
            //}
            ObjectVariant::IndirectObject(s) => match &s.object {
                Some(Value::Stream(stream_val)) => {
                    PdfOperatorVariant::from(stream_val.data.as_slice())?
                }
                Some(Value::Array(array_val)) => process_content_stream_array(array_val, objects)?,
                Some(Value::IndirectObject(ObjectVariant::Stream(s))) => {
                    PdfOperatorVariant::from(s.data.as_slice())?
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
