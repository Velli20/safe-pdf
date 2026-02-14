use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

/// Represents the content stream of a PDF page, containing a sequence
/// of drawing operators.
pub struct ContentStream {
    /// Flat, ordered list of all PDF content stream operators that belong to a page.
    pub operations: Vec<PdfOperatorVariant>,
}

// Helper function to process an array whose elements should be streams or references to streams
fn process_content_stream_array(
    array: &[ObjectVariant],
    objects: &dyn ObjectResolver,
) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
    let mut concatenated_ops = Vec::new();
    for value_in_array in array.iter() {
        let data = value_in_array.try_stream(objects)?.data()?;
        let stream_ops = PdfOperatorVariant::from(&data)?;
        concatenated_ops.extend(stream_ops);
    }
    Ok(concatenated_ops)
}

impl ContentStream {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ContentStream>, PdfOperatorError> {
        const KEY: &str = "Contents";

        // Get the optional `/Contents` entry from the page dictionary.
        let Some(contents) = dictionary.get(KEY) else {
            return Ok(None);
        };

        // Process the resolved /Contents object.
        // It should be a Stream or an Array whose payload is one of these.
        let operations = match objects.resolve_object(contents)? {
            ObjectVariant::Stream(stream) => {
                let data = stream.data()?;
                PdfOperatorVariant::from(&data)?
            }
            ObjectVariant::Array(array_obj) => {
                // The /Contents entry is an array of streams.
                process_content_stream_array(array_obj, objects)?
            }
            other => {
                return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
            }
        };

        Ok(Some(ContentStream { operations }))
    }
}
