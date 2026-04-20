use crate::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};

/// Represents the content stream of a PDF page, containing a sequence
/// of drawing operators.
pub struct ContentStream {
    /// The parsed drawing operators from the content stream.
    pub operators: Vec<PdfOperatorVariant>,
    /// The PDF object number that identifies this content stream, if available.
    ///
    /// For streams created via [`from_stream`](Self::from_stream), this is the
    /// stream object's own number. For streams created via
    /// [`from_dictionary`](Self::from_dictionary), this is the object number of
    /// the `/Contents` entry (whether it resolves to a single stream or an array
    /// of streams).
    pub id: Option<usize>,
}

/// Processes an array of PDF objects, each expected to be a stream or reference to a stream,
/// and concatenates their content stream operators into a single vector.
///
/// # Parameters
///
/// - `array`: Slice of PDF objects representing streams or references to streams.
/// - `objects`: Resolver for indirect PDF objects.
///
/// # Returns
///
/// Concatenated list of operators or error.
fn process_content_stream_array(
    array: &[ObjectVariant],
    objects: &dyn ObjectResolver,
) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
    let mut concatenated_ops = Vec::new();
    for value_in_array in array.iter() {
        let data = value_in_array.try_stream(objects)?.data()?;
        PdfOperatorVariant::parse_into(&data, &mut concatenated_ops)?;
    }
    Ok(concatenated_ops)
}

impl ContentStream {
    /// Constructs a [`ContentStream`] from a PDF page dictionary by resolving the `/Contents` entry.
    ///
    /// The `/Contents` entry may be a stream or an array of streams. This function resolves the entry,
    /// parses the content stream operators, and returns a [`ContentStream`] containing all operators.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The page dictionary containing the `/Contents` entry.
    /// - `objects`: Resolver for indirect PDF objects.
    ///
    /// # Returns
    ///
    /// The parsed content stream or None if missing.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ContentStream>, PdfOperatorError> {
        const KEY: &str = "Contents";

        // Get the optional `/Contents` entry from the page dictionary.
        let Some(contents) = dictionary.get(KEY) else {
            return Ok(None);
        };

        // Extract the object number from the raw `/Contents` entry before
        // resolving. This works whether `/Contents` is a reference to a single
        // stream or to an array of streams — the reference target's object
        // number uniquely identifies this content stream.
        let id = contents.try_object_number().ok();

        // Process the resolved /Contents object.
        // It should be a Stream or an Array whose payload is one of these.
        let operators = match objects.resolve_object(contents)? {
            ObjectVariant::Stream(stream) => {
                let data = stream.data()?;
                PdfOperatorVariant::parse(&data)?
            }
            ObjectVariant::Array(array_obj) => {
                // The /Contents entry is an array of streams.
                process_content_stream_array(array_obj, objects)?
            }
            other => {
                return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
            }
        };

        Ok(Some(ContentStream { operators, id }))
    }

    pub fn from_stream(stream: &StreamObject) -> Result<Self, PdfOperatorError> {
        let data = stream.data()?;
        let operators = PdfOperatorVariant::parse(&data)?;
        Ok(ContentStream {
            operators,
            id: Some(stream.object_number),
        })
    }
}
