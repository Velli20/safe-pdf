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
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    dictionary::Dictionary, object_kind::ObjectKind, object_variant::ObjectVariant,
    pdf_array::PdfArray, stream::StreamObject,
};

use crate::{
    error::FilterError,
    filter::{Filter, Filters},
    lzw::LzwParams,
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
        access: &mut (impl ObjectAccess + ?Sized),
    ) -> Result<Self, FilterError> {
        let mut context = DictionaryContext::new(dictionary, access);
        let Some(filters) = context.optional::<Filters>(Filters::KEY)? else {
            return Ok(Self::Samples(stream_data));
        };
        let decode_parms = context
            .optional::<DecodeParms>(b"DecodeParms")?
            .unwrap_or_default();

        let mut data = stream_data;
        let mut codestream = None;
        let mut previous = None;
        for (index, filter) in filters.into_iter().enumerate() {
            if codestream.is_some() {
                return Err(FilterError::UnsupportedFilter(format!(
                    "{filter} after JPXDecode"
                )));
            }

            if let Filter::JPXDecode = filter {
                codestream = Some(data.clone());
            } else {
                match filter.apply(data.as_ref(), &decode_parms, index, &mut context) {
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

/// The `/DecodeParms` entry of a stream dictionary.
#[derive(Default)]
enum DecodeParms {
    /// No parameters; every filter uses its defaults.
    #[default]
    Absent,
    /// One parameter dictionary, applied to every filter of the chain.
    Shared(ObjectVariant),
    /// One entry per filter, matched by position; null entries keep the defaults.
    PerFilter(PdfArray),
}

impl FromPdfObject for DecodeParms {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            dictionary @ ObjectVariant::Dictionary(_) => Ok(Self::Shared(dictionary.clone())),
            ObjectVariant::Array(array) => Ok(Self::PerFilter(array.clone())),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Dictionary,
                actual: context.object().kind(),
            }),
        }
    }
}

impl DecodeParms {
    /// Reads the parameters of the filter at `index`, falling back to their defaults.
    fn read<T: FromPdfObject + Default>(
        &self,
        index: usize,
        context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<T> {
        let entry = match self {
            Self::Absent => None,
            Self::Shared(dictionary) => Some(dictionary),
            Self::PerFilter(array) => array.get(index),
        };
        match entry {
            Some(entry) => Ok(context.read::<Option<T>>(entry)?.unwrap_or_default()),
            None => Ok(T::default()),
        }
    }
}

/// `/DecodeParms` of the JBIG2Decode filter (PDF spec §7.4.7, Table 12).
#[derive(Default)]
struct Jbig2Params {
    /// The stream of segments shared by several JBIG2 images.
    globals: Option<StreamObject>,
}

impl FromPdfObject for Jbig2Params {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self {
            globals: context.dictionary()?.optional(b"JBIG2Globals")?,
        })
    }
}

impl Filter {
    /// Applies this byte-to-byte filter, the `index`-th of its chain, to `data`.
    ///
    /// `context` reads the stream dictionary, whose image entries some filters
    /// need. `JPXDecode` is not a byte filter; [`ImagePayload::decode`] handles
    /// it before reaching this method, and it is rejected here.
    fn apply(
        &self,
        data: &[u8],
        decode_parms: &DecodeParms,
        index: usize,
        context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
    ) -> Result<Bytes, FilterError> {
        let decoded = match self {
            Self::FlateDecode => {
                let predictor = decode_parms.read::<PredictorParams>(index, context)?;
                let decoded = Self::decode_flate(data)?;
                if predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, &predictor)?
                }
            }
            Self::LZWDecode => {
                let params = decode_parms.read::<LzwParams>(index, context)?;
                let decoded = crate::lzw::decode(data, params.early_change)?;
                if params.predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, &params.predictor)?
                }
            }
            Self::DCTDecode => {
                Self::decode_jpeg_baseline(data, context.optional::<u16>(b"Height")?)?
            }
            Self::ASCII85Decode => crate::ascii85::decode_ascii85(data)?,
            Self::ASCIIHexDecode => crate::asciihex::decode_ascii_hex(data),
            Self::RunLengthDecode => crate::runlength::decode_run_length(data)?,
            Self::JBIG2Decode => {
                let missing_dimensions_error = || {
                    FilterError::Decompression(
                        "JBIG2Decode requires positive Width and Height".into(),
                    )
                };
                let width = context
                    .optional::<u16>(b"Width")?
                    .filter(|width| *width != 0)
                    .ok_or_else(missing_dimensions_error)?;
                let height = context
                    .optional::<u16>(b"Height")?
                    .filter(|height| *height != 0)
                    .ok_or_else(missing_dimensions_error)?;
                let params = decode_parms.read::<Jbig2Params>(index, context)?;
                let globals = params.globals.as_ref().map(StreamObject::raw_data);
                pdf_jbig2::decode(data, width, height, globals)?
            }
            Self::CCITTFaxDecode => {
                let params = decode_parms.read::<CCITTFaxParams>(index, context)?;
                pdf_ccitt::decode(data, &params)?
            }
            Self::JPXDecode => {
                return Err(FilterError::UnsupportedFilter("JPXDecode".to_string()));
            }
            Self::Unsupported(name) => {
                return Err(FilterError::UnsupportedFilter(
                    String::from_utf8_lossy(name).into_owned(),
                ));
            }
        };
        Ok(decoded.into())
    }
}
