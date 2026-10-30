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
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    object_kind::ObjectKind, object_variant::ObjectVariant, pdf_array::PdfArray,
    stream::StreamObject,
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

/// Reads an image stream's payload by applying its filter chain.
impl FromPdfObject for ImagePayload {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(context.decode::<EncodedPayload>()?.decode()?)
    }
}

/// A stream's bytes with its filter chain read but not yet applied.
///
/// Reading resolves every entry the chain decodes with, including indirect
/// `/DecodeParms` and `/JBIG2Globals`, so [`Self::decode`] needs no object
/// source and can run long after the document was read.
#[derive(Clone, Debug)]
pub struct EncodedPayload {
    pipeline: FilterPipeline,
    data: Bytes,
}

/// Holds an already applied payload: samples need no further filter, and a
/// codestream still passes through `JPXDecode`.
impl From<ImagePayload> for EncodedPayload {
    fn from(payload: ImagePayload) -> Self {
        let (stages, data) = match payload {
            ImagePayload::Samples(data) => (Vec::new(), data),
            ImagePayload::Jpx(data) => (vec![(Filter::JPXDecode, Ok(FilterParams::None))], data),
        };
        Self {
            pipeline: FilterPipeline(stages),
            data,
        }
    }
}

/// Reads an image stream's filter chain without applying it.
///
/// A stream whose filter chain was already applied keeps only a `JPXDecode`
/// stage, if it has one: a `JPXDecode` stream is deliberately left encoded,
/// because only the structured image path can read a JPEG 2000 codestream.
impl FromPdfObject for EncodedPayload {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.stream()?;
        let stream = context.stream();
        let data = stream.shared_data();
        if !stream.filters_applied() {
            // An unreadable chain is a filter failure, like a filter that fails to run.
            let pipeline = context.read(object).map_err(FilterError::from)?;
            return Ok(Self { pipeline, data });
        }
        let jpx = context
            .dictionary()
            .optional::<Filters>(Filters::KEY)?
            .is_some_and(|filters| filters.has_jpx_filter());
        let stages = match jpx {
            true => vec![(Filter::JPXDecode, Ok(FilterParams::None))],
            false => Vec::new(),
        };
        Ok(Self {
            pipeline: FilterPipeline(stages),
            data,
        })
    }
}

impl EncodedPayload {
    /// Applies the filter chain, as reading an [`ImagePayload`] would have.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] if a filter fails, is unsupported, or follows
    /// `JPXDecode`, except when an adjacent duplicate filter fails after its
    /// first pass succeeded.
    pub fn decode(&self) -> Result<ImagePayload, FilterError> {
        self.pipeline.apply(self.data.clone())
    }

    /// Returns shared ownership of the bytes the chain applies to.
    pub fn shared_data(&self) -> Bytes {
        self.data.clone()
    }

    /// Returns whether the chain includes `JPXDecode`.
    pub fn has_jpx_filter(&self) -> bool {
        self.pipeline
            .0
            .iter()
            .any(|(filter, _)| matches!(filter, Filter::JPXDecode))
    }
}

impl ImagePayload {
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

/// A stream dictionary's `/Filter` chain, each filter paired with its parameters.
///
/// Parameters are read with the chain, but a filter whose parameters cannot be read
/// reports that failure only when it is reached, as if it had been read then.
#[derive(Clone, Debug)]
pub(crate) struct FilterPipeline(Vec<(Filter, Result<FilterParams, FilterError>)>);

/// Reads `/Filter`, `/DecodeParms`, and the image entries some filters need.
impl FromPdfObject for FilterPipeline {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let Some(filters) = context.optional::<Filters>(Filters::KEY)? else {
            return Ok(Self(Vec::new()));
        };
        let decode_parms = context
            .optional::<DecodeParms>(b"DecodeParms")?
            .unwrap_or_default();
        let stages = filters
            .into_iter()
            .enumerate()
            .map(|(index, filter)| {
                let entry = decode_parms.entry(index);
                let params = match filter {
                    Filter::FlateDecode => entry
                        .map(|entry| context.read::<Option<PredictorParams>>(entry))
                        .transpose()
                        .map(|params| FilterParams::Predictor(params.flatten().unwrap_or_default()))
                        .map_err(FilterError::from),
                    Filter::LZWDecode => entry
                        .map(|entry| context.read::<Option<LzwParams>>(entry))
                        .transpose()
                        .map(|params| FilterParams::Lzw(params.flatten().unwrap_or_default()))
                        .map_err(FilterError::from),
                    Filter::CCITTFaxDecode => entry
                        .map(|entry| context.read::<Option<CCITTFaxParams>>(entry))
                        .transpose()
                        .map(|params| FilterParams::Ccitt(params.flatten().unwrap_or_default()))
                        .map_err(FilterError::from),
                    Filter::DCTDecode => context
                        .optional::<u16>(b"Height")
                        .map(|height| FilterParams::Dct { height })
                        .map_err(FilterError::from),
                    Filter::JBIG2Decode => {
                        let missing_dimensions_error = || {
                            FilterError::Decompression(
                                "JBIG2Decode requires positive Width and Height".into(),
                            )
                        };
                        (|| {
                            let width = context
                                .optional::<u16>(b"Width")?
                                .filter(|width| *width != 0)
                                .ok_or_else(missing_dimensions_error)?;
                            let height = context
                                .optional::<u16>(b"Height")?
                                .filter(|height| *height != 0)
                                .ok_or_else(missing_dimensions_error)?;
                            let globals = entry
                                .map(|entry| context.read::<Option<Jbig2Params>>(entry))
                                .transpose()?
                                .flatten()
                                .unwrap_or_default()
                                .globals;
                            Ok(FilterParams::Jbig2 {
                                width,
                                height,
                                globals,
                            })
                        })()
                    }
                    _ => Ok(FilterParams::None),
                };
                (filter.clone(), params)
            })
            .collect();
        Ok(Self(stages))
    }
}

impl FilterPipeline {
    /// Applies the chain to `stream_data`.
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
    pub(crate) fn apply(&self, stream_data: Bytes) -> Result<ImagePayload, FilterError> {
        let mut data = stream_data;
        let mut codestream = None;
        let mut previous = None;
        for (filter, params) in &self.0 {
            if codestream.is_some() {
                return Err(FilterError::UnsupportedFilter(format!(
                    "{filter} after JPXDecode"
                )));
            }

            if let Filter::JPXDecode = filter {
                codestream = Some(data.clone());
            } else {
                let decoded = params
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|params| filter.apply(data.as_ref(), params));
                match decoded {
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
            Some(codestream) => ImagePayload::Jpx(codestream),
            None => ImagePayload::Samples(data),
        })
    }
}

/// The parameters one filter of a chain decodes with.
#[derive(Clone, Debug)]
pub(crate) enum FilterParams {
    /// The filter takes no parameters.
    None,
    /// FlateDecode's predictor.
    Predictor(PredictorParams),
    /// LZWDecode's code-width rule and predictor.
    Lzw(LzwParams),
    /// CCITTFaxDecode's `/DecodeParms`.
    Ccitt(CCITTFaxParams),
    /// DCTDecode's replacement for a placeholder frame height: the image's `/Height`.
    Dct { height: Option<u16> },
    /// JBIG2Decode's page size, the image's dimensions, and its shared segments.
    Jbig2 {
        width: u16,
        height: u16,
        globals: Option<StreamObject>,
    },
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
        match context.object() {
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
    /// Returns the parameter entry of the filter at `index`, if it has one.
    fn entry(&self, index: usize) -> Option<&ObjectVariant> {
        match self {
            Self::Absent => None,
            Self::Shared(dictionary) => Some(dictionary),
            Self::PerFilter(array) => array.get(index),
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
    /// Applies this byte-to-byte filter to `data` with the parameters read for it.
    ///
    /// `JPXDecode` is not a byte filter; [`FilterPipeline::apply`] handles it before
    /// reaching this method, and it is rejected here.
    fn apply(&self, data: &[u8], params: &FilterParams) -> Result<Bytes, FilterError> {
        let decoded = match (self, params) {
            (Self::FlateDecode, FilterParams::Predictor(predictor)) => {
                let decoded = Self::decode_flate(data)?;
                if predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, predictor)?
                }
            }
            (Self::LZWDecode, FilterParams::Lzw(params)) => {
                let decoded = crate::lzw::decode(data, params.early_change)?;
                if params.predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, &params.predictor)?
                }
            }
            (Self::DCTDecode, FilterParams::Dct { height }) => {
                Self::decode_jpeg_baseline(data, *height)?
            }
            (Self::ASCII85Decode, _) => crate::ascii85::decode_ascii85(data)?,
            (Self::ASCIIHexDecode, _) => crate::asciihex::decode_ascii_hex(data),
            (Self::RunLengthDecode, _) => crate::runlength::decode_run_length(data)?,
            (
                Self::JBIG2Decode,
                FilterParams::Jbig2 {
                    width,
                    height,
                    globals,
                },
            ) => pdf_jbig2::decode(
                data,
                *width,
                *height,
                globals.as_ref().map(StreamObject::raw_data),
            )?,
            (Self::CCITTFaxDecode, FilterParams::Ccitt(params)) => pdf_ccitt::decode(data, params)?,
            (Self::Unsupported(name), _) => {
                return Err(FilterError::UnsupportedFilter(
                    String::from_utf8_lossy(name).into_owned(),
                ));
            }
            (filter, _) => {
                return Err(FilterError::UnsupportedFilter(filter.to_string()));
            }
        };
        Ok(decoded.into())
    }
}
