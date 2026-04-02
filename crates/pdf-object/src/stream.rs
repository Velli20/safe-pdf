use crate::ccitt_fax_params::CCITTFaxParams;
use crate::error::ObjectError;
use crate::object_resolver::{ObjectResolver, PassthroughResolver};
use crate::object_variant::ObjectVariant;
use crate::{dictionary::Dictionary, filter::Filter};
use std::borrow::Cow;

/// Represents a PDF stream object.
///
/// A stream object, like a string object, is a sequence of bytes. However, PDF
/// can store large amounts of data in a stream that it would not be practical
/// to store in a string. Streams are used for objects such as images, page content descriptions,
/// and font definitions.
#[derive(Debug, PartialEq, Clone)]
pub struct StreamObject {
    /// The object number, identifying this stream as an indirect object.
    pub object_number: usize,
    /// The generation number, used for PDF incremental updates.
    pub generation_number: usize,
    /// The dictionary associated with this stream.
    pub dictionary: Box<Dictionary>,
    /// The raw, uncompressed, byte data of the stream.
    data: Vec<u8>,
    /// The filters applied to the stream data.
    filters: Option<Vec<Filter>>,
}

impl StreamObject {
    /// Creates a new [`StreamObject`].
    pub fn new(
        object_number: usize,
        generation_number: usize,
        dictionary: Box<Dictionary>,
        data: Vec<u8>,
        filters: Option<Vec<Filter>>,
    ) -> Self {
        StreamObject {
            object_number,
            generation_number,
            dictionary,
            data,
            filters,
        }
    }

    /// Returns the raw stream bytes before any filter decoding.
    ///
    /// This is useful for encryption/decryption operations which need
    /// access to the raw bytes before decompression filters are applied.
    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// Returns a reference to the filters applied to this stream.
    pub fn filters(&self) -> Option<&Vec<Filter>> {
        self.filters.as_ref()
    }

    /// Returns the fully decoded stream bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if decompression fails or if the stream has unsupported
    /// filter chains.
    pub fn data(&self) -> Result<Cow<'_, [u8]>, ObjectError> {
        let mut data: Cow<'_, [u8]> = Cow::Borrowed(&self.data);

        let Some(filters) = &self.filters else {
            return Ok(data);
        };

        for (filter_idx, filter) in filters.iter().enumerate() {
            match filter {
                Filter::FlateDecode => {
                    let decoded = Filter::decode_flate(&data)?;
                    data = Cow::Owned(decoded);
                }
                Filter::JPXDecode => {
                    let decoded = Filter::decode_jpeg2000(&data)?;
                    data = Cow::Owned(decoded);
                }
                Filter::DCTDecode => {
                    let decoded = Filter::decode_jpeg_baseline(&data)?;
                    data = Cow::Owned(decoded);
                }
                Filter::ASCII85Decode => {
                    let decoded = Filter::decode_ascii85(&data)?;
                    data = Cow::Owned(decoded);
                }
                Filter::CCITTFaxDecode => {
                    let objects = PassthroughResolver;
                    let params = ccitt_params_for_filter(&self.dictionary, filter_idx, &objects);
                    let decoded = Filter::decode_ccitt_fax(&data, &params)?;
                    data = Cow::Owned(decoded);
                }
                _ => {
                    println!(
                        "Unsupported filter in data_with_remaining_filter: {:?}",
                        filter
                    );
                    break;
                }
            }
        }
        Ok(data)
    }
}

/// Extract [`CCITTFaxParams`][crate::ccitt::CCITTFaxParams] for the filter at
/// `filter_idx` from the stream's `/DecodeParms` dictionary entry.
///
/// Per PDF spec §7.3.8.2, `/DecodeParms` is either a single dictionary (when
/// there is one filter) or an array of dictionaries (one per filter). Values
/// are always inline objects, so no object resolver is needed.
fn ccitt_params_for_filter(
    dict: &Dictionary,
    filter_idx: usize,
    objects: &dyn ObjectResolver,
) -> CCITTFaxParams {
    match dict.get("DecodeParms") {
        Some(ObjectVariant::Dictionary(d)) => {
            CCITTFaxParams::from_dictionary(d, objects).unwrap_or_default()
        }
        Some(ObjectVariant::Array(arr)) => arr
            .get(filter_idx)
            .and_then(|v| {
                if let ObjectVariant::Dictionary(d) = v {
                    Some(CCITTFaxParams::from_dictionary(d, objects).unwrap_or_default())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => CCITTFaxParams::default(),
    }
}
