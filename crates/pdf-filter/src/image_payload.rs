//! The result of applying an image stream's filter chain.
//!
//! Every PDF filter but one turns bytes into bytes. `JPXDecode` does not: a
//! JPEG 2000 codestream carries its own component count, precision, colour
//! description, and opacity, none of which survives a flat byte buffer. This
//! module therefore stops the chain at `JPXDecode` and hands the codestream on,
//! so the image path can decode it with that metadata intact.

use bytes::Bytes;
use pdf_ccitt::CCITTFaxParams;
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    error::FilterError,
    filter::{Filter, Filters, resolve_jbig2_dimensions, resolve_jbig2_globals},
    predictor::PredictorParams,
};

/// What is left of a stream once its byte filters have been applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImagePayload {
    /// Component samples a caller can interpret directly.
    Samples(Bytes),
    /// A JPEG 2000 codestream that only the structured image path can decode.
    Jpx(Bytes),
}

impl ImagePayload {
    /// Applies the `/Filter` chain of `dictionary` to `stream_data`.
    ///
    /// Filters are applied in order until the chain ends or `JPXDecode` is
    /// reached. PDF requires `JPXDecode` to be the last filter of an image
    /// stream, so a filter following it is rejected rather than applied to
    /// compressed data. Unfiltered input retains its shared allocation.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] if a filter fails, is unsupported, or follows
    /// `JPXDecode`, except when an adjacent duplicate filter fails after its
    /// first pass succeeded.
    pub fn decode(
        dictionary: &Dictionary,
        stream_data: Bytes,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FilterError> {
        let mut data = stream_data;
        let filters = Filters::from_dictionary(dictionary, objects)?;

        let Some(filters) = &filters else {
            return Ok(Self::Samples(data));
        };

        let decode_parms = dictionary
            .get(b"DecodeParms")
            .map(|entry| objects.resolve_object(entry))
            .transpose()?;

        let mut codestream = None;
        let mut previous = None;
        for (index, filter) in filters.into_iter().enumerate() {
            if codestream.is_some() {
                return Err(FilterError::UnsupportedFilter(format!(
                    "{filter} after JPXDecode"
                )));
            }

            let param_dict = match decode_parms {
                None => None,
                Some(ObjectVariant::Dictionary(dictionary)) => Some(dictionary),
                Some(ObjectVariant::Array(array)) => {
                    array.as_slice().optional_dictionary(index, objects)?
                }
                Some(other) => {
                    return Err(FilterError::from(
                        pdf_object_reader::object_error::ObjectError::TypeMismatch(
                            "Dictionary or Array",
                            other.name(),
                        ),
                    ));
                }
            };

            if let Filter::JPXDecode = filter {
                codestream = Some(data.clone());
            } else {
                match decode_filter(filter, data.as_ref(), param_dict, dictionary, objects) {
                    Ok(decoded) => data = decoded,
                    // A repeated filter can be spurious when its first pass
                    // already produced the binary data expected by the next filter.
                    Err(_) if previous == Some(filter) => {}
                    Err(error) => return Err(error),
                }
            }
            previous = Some(filter);
        }

        Ok(match codestream {
            Some(codestream) => Self::Jpx(codestream),
            None => Self::Samples(data),
        })
    }

    /// Returns the decoded samples, rejecting a JPEG 2000 codestream.
    ///
    /// Callers that want plain bytes cannot interpret a codestream, and
    /// returning it unchanged would pass compressed data off as samples.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::UnsupportedFilter`] for a [`Self::Jpx`] payload.
    pub fn into_samples(self) -> Result<Bytes, FilterError> {
        match self {
            Self::Samples(samples) => Ok(samples),
            Self::Jpx(_) => Err(FilterError::UnsupportedFilter("JPXDecode".to_string())),
        }
    }
}

/// Applies one byte-to-byte `filter` to `data`.
///
/// `JPXDecode` is not a byte filter; [`ImagePayload::decode`] handles it before
/// reaching this function, and it is rejected here.
fn decode_filter(
    filter: &Filter,
    data: &[u8],
    param_dict: Option<&Dictionary>,
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Bytes, FilterError> {
    let decoded = match filter {
        Filter::FlateDecode => {
            let decoded = Filter::decode_flate(data)?;
            let predictor = match param_dict {
                Some(dictionary) => PredictorParams::from_dictionary(dictionary, objects)?,
                None => PredictorParams::default(),
            };
            if predictor.is_none() {
                decoded
            } else {
                crate::predictor::apply_predictor(&decoded, &predictor)?
            }
        }
        Filter::LZWDecode => {
            let (early_change, predictor) = match param_dict {
                Some(dictionary) => (
                    dictionary
                        .optional_number(b"EarlyChange", objects)?
                        .unwrap_or(1)
                        != 0,
                    PredictorParams::from_dictionary(dictionary, objects)?,
                ),
                None => (true, PredictorParams::default()),
            };
            let decoded = crate::lzw::decode(data, early_change)?;
            if predictor.is_none() {
                decoded
            } else {
                crate::predictor::apply_predictor(&decoded, &predictor)?
            }
        }
        Filter::DCTDecode => {
            let height = dictionary.optional_number::<u16>(b"Height", objects)?;
            Filter::decode_jpeg_baseline(data, height)?
        }
        Filter::ASCII85Decode => crate::ascii85::decode_ascii85(data)?,
        Filter::ASCIIHexDecode => crate::asciihex::decode_ascii_hex(data),
        Filter::RunLengthDecode => crate::runlength::decode_run_length(data)?,
        Filter::JBIG2Decode => {
            let (width, height) = resolve_jbig2_dimensions(dictionary, objects)?;
            let globals = match param_dict {
                Some(dictionary) => resolve_jbig2_globals(dictionary, objects)?,
                None => None,
            };
            pdf_jbig2::decode(data, width, height, globals.as_deref())?
        }
        Filter::CCITTFaxDecode => {
            let ccitt_params = match param_dict {
                Some(dictionary) => CCITTFaxParams::from_dictionary(dictionary, objects)?,
                None => CCITTFaxParams::default(),
            };
            pdf_ccitt::decode(data, &ccitt_params)?
        }
        Filter::JPXDecode => {
            return Err(FilterError::UnsupportedFilter("JPXDecode".to_string()));
        }
        Filter::Unsupported(name) => {
            return Err(FilterError::UnsupportedFilter(
                String::from_utf8_lossy(name).into_owned(),
            ));
        }
    };
    Ok(decoded.into())
}
