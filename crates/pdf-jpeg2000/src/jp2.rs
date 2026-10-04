//! JP2-family file identification and borrowed image metadata.

use thiserror::Error;

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    box_reader::{BoxKind, Jp2Box, Jp2BoxReader},
    color_specifications::ColorSpecifications,
    compositing_layer::CompositingLayer,
    composition::Composition,
    container::{CodestreamAssociation, InputFormat, Jp2Metadata},
    fragment_list::{FragmentLayout, FragmentList},
    jp2_header::{HeaderBoxes, Jp2Header},
    opacity::Opacity,
    reader_requirements::ReaderRequirements,
    size::SizeHeader,
};

/// Required signature box payload.
const SIGNATURE_VALUE: [u8; 4] = [0x0d, 0x0a, 0x87, 0x0a];
/// Part 1 JP2 file-type brand.
const JP2_BRAND: [u8; 4] = *b"jp2 ";
/// JPX file-type brand.
const JPX_BRAND: [u8; 4] = *b"jpx ";
/// JPX baseline compatibility brand.
const JPX_BASELINE: [u8; 4] = *b"jpxb";
/// Fixed bytes before compatibility brands in a File Type box.
const FILE_TYPE_PREFIX_BYTES: usize = 8;

/// Invalid JP2-family file structure or metadata.
#[derive(Debug, Error)]
pub enum ContainerError {
    /// The file omits or corrupts its signature.
    #[error("invalid JPEG 2000 file signature")]
    Signature,
    /// A required box is absent or out of order.
    #[error("required box {0} is absent or out of order")]
    Required(BoxKind),
    /// A box body violates its field layout.
    #[error("invalid box {kind} at byte {offset}")]
    Invalid {
        /// Byte offset of the box header.
        offset: usize,
        /// Type of the invalid box.
        kind: BoxKind,
    },
    /// A duplicate singleton box was found.
    #[error("duplicate box {kind} at byte {offset}")]
    Duplicate {
        /// Byte offset of the repeated box header.
        offset: usize,
        /// Type of the repeated box.
        kind: BoxKind,
    },
    /// The caller required a different container format.
    #[error("container format does not match the requested format")]
    Format,
    /// An output channel is absent from the JP2 channel chain.
    #[error("output channel {channel} is not defined by the JP2 channel chain")]
    Channel {
        /// Zero-based index of the undefined channel.
        channel: u16,
    },
    /// A reconstructed sample indexes outside the JP2 palette.
    #[error("palette index {index} is outside the {entries}-entry palette")]
    PaletteIndex {
        /// The out-of-range index carried by the reconstructed sample.
        index: i64,
        /// Number of entries the Palette box defines.
        entries: u16,
    },
}

impl From<Jp2Box<'_>> for ContainerError {
    fn from(box_view: Jp2Box<'_>) -> Self {
        Self::Invalid {
            offset: box_view.offset(),
            kind: box_view.kind(),
        }
    }
}

impl TryFrom<Jp2Box<'_>> for InputFormat {
    type Error = ContainerError;

    /// Reads the brand and compatibility list of a File Type box.
    fn try_from(box_view: Jp2Box<'_>) -> Result<Self, Self::Error> {
        let Some((prefix, compatibility)) =
            box_view.payload().split_at_checked(FILE_TYPE_PREFIX_BYTES)
        else {
            return Err(box_view.into());
        };
        let (brands, remainder) = compatibility.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err(box_view.into());
        }
        let brand = prefix.get(..4);
        let jpx = brand == Some(JPX_BRAND.as_slice())
            || brands
                .iter()
                .any(|candidate| matches!(candidate, &JPX_BRAND | &JPX_BASELINE));
        let jp2 = brand == Some(JP2_BRAND.as_slice()) || brands.contains(&JP2_BRAND);
        match (jpx, jp2) {
            (true, _) => Ok(Self::Jpx),
            (false, true) => Ok(Self::Jp2),
            (false, false) => Err(ContainerError::Format),
        }
    }
}

/// What one scan of the top-level boxes found, all of it borrowed.
///
/// A JPX file numbers its codestreams and compositing layers by the order of
/// their boxes, so counting them is what lets a later scan find the one the
/// first compositing layer needs. Nothing is collected per codestream, which
/// keeps the scan free of allocation: the decoder's working-memory budget does
/// not exist yet while the container is being parsed.
#[derive(Debug, Default)]
struct ContainerScan<'a> {
    header: Option<Jp2Box<'a>>,
    layer: Option<Jp2Box<'a>>,
    composition: Option<Jp2Box<'a>>,
    requirements: Option<ReaderRequirements<'a>>,
    codestreams: u32,
    layers: u32,
    first_codestream: Option<usize>,
}

impl<'a> ContainerScan<'a> {
    /// Validates top-level box order and counts codestreams and layers.
    fn read(boxes: &mut Jp2BoxReader<'a>, format: InputFormat) -> Result<Self, Jpeg2000Error> {
        let mut scan = Self::default();
        while let Some(entry) = boxes.next_box()? {
            let kind = entry.kind();
            let duplicate = || ContainerError::Duplicate {
                offset: entry.offset(),
                kind,
            };
            match kind {
                BoxKind::Jp2Header => {
                    if scan.header.is_some() {
                        return Err(duplicate().into());
                    }
                    scan.header = Some(entry);
                }
                // Annex I permits one codestream in a JP2 file, and a fragment
                // table only in a JPX one.
                BoxKind::FragmentTable if format != InputFormat::Jpx => {
                    return Err(ContainerError::from(entry).into());
                }
                BoxKind::Codestream | BoxKind::FragmentTable => {
                    if scan.codestreams > 0 && format != InputFormat::Jpx {
                        return Err(duplicate().into());
                    }
                    scan.first_codestream.get_or_insert_with(|| entry.offset());
                    scan.codestreams = scan.codestreams.saturating_add(1);
                }
                BoxKind::CompositingLayerHeader => {
                    scan.layer.get_or_insert(entry);
                    scan.layers = scan.layers.saturating_add(1);
                }
                BoxKind::Composition => {
                    if scan.composition.is_some() {
                        return Err(duplicate().into());
                    }
                    scan.composition = Some(entry);
                }
                BoxKind::ReaderRequirements => {
                    if scan.requirements.is_some() {
                        return Err(duplicate().into());
                    }
                    scan.requirements = Some(ReaderRequirements::try_from(entry)?);
                }
                _ => {}
            }
        }
        Ok(scan)
    }

    /// Returns the number of compositing layers the file presents.
    ///
    /// A file without a Compositing Layer Header box still has one layer: the
    /// image its first codestream carries.
    fn layer_count(&self) -> u32 {
        self.layers.max(1)
    }
}

/// The boxes belonging to one codestream of a container.
#[derive(Debug)]
struct SelectedCodestream<'a> {
    codestream: Jp2Box<'a>,
    header: Option<Jp2Box<'a>>,
}

impl<'a> SelectedCodestream<'a> {
    /// Finds the codestream with `index` and the Codestream Header beside it.
    ///
    /// Both are numbered by the order of their boxes, so a second scan of the
    /// box chain locates them without an index built during the first.
    fn locate(bytes: &'a [u8], index: u32) -> Result<Self, Jpeg2000Error> {
        let mut boxes = Jp2BoxReader::new(bytes);
        let mut codestreams = 0u32;
        let mut headers = 0u32;
        let mut codestream = None;
        let mut header = None;
        while let Some(entry) = boxes.next_box()? {
            match entry.kind() {
                BoxKind::Codestream | BoxKind::FragmentTable => {
                    if codestreams == index {
                        codestream.get_or_insert(entry);
                    }
                    codestreams = codestreams.saturating_add(1);
                }
                BoxKind::CodestreamHeader => {
                    if headers == index {
                        header.get_or_insert(entry);
                    }
                    headers = headers.saturating_add(1);
                }
                _ => {}
            }
        }
        let codestream = codestream.ok_or(ContainerError::Required(BoxKind::Codestream))?;
        Ok(Self { codestream, header })
    }

    /// Borrows the selected codestream's bytes and their offset in the file.
    ///
    /// A Contiguous Codestream box is already one slice. A fragment table is
    /// one only when its fragments lie in this file, in order, and leave no gap
    /// between them, because every stage above reads the codestream as a single
    /// borrowed slice.
    fn bytes(&self, file: &'a [u8]) -> Result<(&'a [u8], usize), Jpeg2000Error> {
        if self.codestream.kind() != BoxKind::FragmentTable {
            return Ok((self.codestream.payload(), self.codestream.payload_offset()));
        }
        let list =
            FragmentList::try_from(self.codestream.children().require(BoxKind::FragmentList)?)?;
        match list.layout()? {
            FragmentLayout::Contiguous(range) => {
                let offset = range.start;
                let fragment = file.get(range).ok_or(Jpeg2000Error::Truncated {
                    offset,
                    context: "JPX codestream fragment",
                })?;
                Ok((fragment, offset))
            }
            FragmentLayout::External => Err(Jpeg2000Error::UnsupportedFeature {
                feature: "JPX codestream fragments in another file",
            }),
            FragmentLayout::Scattered => Err(Jpeg2000Error::UnsupportedFeature {
                feature: "scattered JPX codestream fragments",
            }),
        }
    }
}

/// What the container scan resolved for the selected codestream.
#[derive(Debug)]
pub(crate) struct Resolution<'a> {
    pub(crate) association: CodestreamAssociation,
    pub(crate) opacity: Option<Opacity<'a>>,
    pub(crate) requirements: Option<ReaderRequirements<'a>>,
    pub(crate) composition: Option<Composition>,
}

/// Validated container and the codestream this decode reconstructs.
pub(crate) struct ParsedContainer<'a> {
    pub(crate) format: InputFormat,
    pub(crate) metadata: Jp2Metadata<'a>,
    pub(crate) codestream_offset: usize,
}

impl<'a> ParsedContainer<'a> {
    /// Validates top-level box order and resolves one codestream to decode.
    ///
    /// A JP2 file resolves to its single codestream. A JPX file resolves to the
    /// codestream its first compositing layer registers, or, when that layer
    /// registers none, to the codestream sharing the layer's index. The layer's
    /// own colour group and Opacity box override the JP2 Header defaults, and
    /// the Codestream Header box of the selected codestream overrides them box
    /// by box.
    pub(crate) fn parse(bytes: &'a [u8], requested: InputFormat) -> Result<Self, Jpeg2000Error> {
        let mut boxes = Jp2BoxReader::new(bytes);
        let signature = boxes.require(BoxKind::Signature)?;
        if signature.payload() != SIGNATURE_VALUE {
            return Err(ContainerError::Signature.into());
        }
        let format = InputFormat::try_from(boxes.require(BoxKind::FileType)?)?;
        if matches!(requested, InputFormat::Jp2 | InputFormat::Jpx) && requested != format {
            return Err(ContainerError::Format.into());
        }
        let scan = ContainerScan::read(&mut boxes, format)?;
        Self::validate_order(&scan, format)?;
        let layer = match scan.layer {
            Some(box_view) => CompositingLayer::parse(box_view, format)?,
            None => CompositingLayer::default(),
        };
        let index = Self::registered_codestream(&layer, &scan)?;
        let selected = SelectedCodestream::locate(bytes, index)?;
        let header = Self::merge_header(&scan, &selected, &layer, format)?;
        let opacity = match layer.opacity() {
            // A chroma key names a transparent colour rather than a channel,
            // which is a display rule this decoder does not carry.
            Some(opacity) if opacity.channel_kind().is_none() => {
                return Err(Jpeg2000Error::UnsupportedFeature {
                    feature: "JPX chroma-key opacity",
                });
            }
            opacity => opacity,
        };
        let composition = Self::validate_composition(&scan, header.shape().size)?;
        let (codestream, codestream_offset) = selected.bytes(bytes)?;
        let resolution = Resolution {
            association: CodestreamAssociation {
                codestream_count: scan.codestreams,
                layer_count: scan.layer_count(),
                codestream: index,
                layer: 0,
            },
            opacity,
            requirements: scan.requirements,
            composition,
        };
        Ok(Self {
            format,
            metadata: header.into_metadata(codestream, resolution),
            codestream_offset,
        })
    }

    /// Checks that the file has a codestream and describes it beforehand.
    ///
    /// A JP2 file must carry a JP2 Header box before the codestream it applies
    /// to. A JPX file may describe each codestream in its own Codestream Header
    /// box instead, but a JP2 Header box present at all still comes first.
    fn validate_order(scan: &ContainerScan<'a>, format: InputFormat) -> Result<(), Jpeg2000Error> {
        if scan.codestreams == 0 {
            return Err(ContainerError::Required(BoxKind::Codestream).into());
        }
        match (scan.header, scan.first_codestream) {
            (None, _) if format != InputFormat::Jpx => {
                Err(ContainerError::Required(BoxKind::Jp2Header).into())
            }
            (Some(header), Some(first)) if header.offset() > first => {
                Err(Jpeg2000Error::InvalidBox {
                    offset: header.offset(),
                    kind: BoxKind::Jp2Header,
                    reason: "header box follows the first codestream",
                })
            }
            _ => Ok(()),
        }
    }

    /// Lays the selected codestream's and layer's boxes over the file defaults.
    fn merge_header(
        scan: &ContainerScan<'a>,
        selected: &SelectedCodestream<'a>,
        layer: &CompositingLayer<'a>,
        format: InputFormat,
    ) -> Result<Jp2Header<'a>, Jpeg2000Error> {
        let defaults = match scan.header {
            Some(box_view) => HeaderBoxes::collect(box_view, true)?,
            None => HeaderBoxes::default(),
        };
        let overrides = match selected.header {
            Some(box_view) => HeaderBoxes::collect(box_view, false)?,
            None => HeaderBoxes::default(),
        };
        let colors = layer.colors().or_else(|| {
            scan.header
                .map(|box_view| ColorSpecifications::new(box_view, format))
        });
        defaults
            .overridden_by(overrides)
            .into_header(colors, format)
    }

    /// Reads the Composition box and refuses one that composes anything.
    ///
    /// A single layer already covering the canvas cannot be moved by the
    /// instructions, so the composed image is that layer itself.
    fn validate_composition(
        scan: &ContainerScan<'a>,
        image: Size<u32>,
    ) -> Result<Option<Composition>, Jpeg2000Error> {
        let Some(box_view) = scan.composition else {
            return Ok(None);
        };
        let composition = Composition::parse(box_view)?;
        if scan.layer_count() > 1 || scan.codestreams > 1 || !composition.is_identity_for(image) {
            return Err(Jpeg2000Error::UnsupportedFeature {
                feature: "JPX composition of several compositing layers",
            });
        }
        Ok(Some(composition))
    }

    /// Returns the index of the codestream that supplies the first layer.
    ///
    /// Part 2 registers a layer's codestreams explicitly; a layer without a
    /// Codestream Registration box uses the codestream that shares its index,
    /// which for the first layer is the first codestream.
    fn registered_codestream(
        layer: &CompositingLayer<'a>,
        scan: &ContainerScan<'a>,
    ) -> Result<u32, Jpeg2000Error> {
        let Some(registration) = layer.registration() else {
            return Ok(0);
        };
        let record = registration
            .sole_codestream()
            .ok_or(Jpeg2000Error::UnsupportedFeature {
                feature: "JPX compositing layer composed from several codestreams",
            })?;
        // A codestream sampled onto a coarser layer grid is a resolution change
        // between the codestream and the layer, not a codestream this decoder
        // can hand over as the layer's samples.
        if !record.covers_grid() {
            return Err(Jpeg2000Error::UnsupportedFeature {
                feature: "JPX codestream registered on a coarser grid than its layer",
            });
        }
        let codestream = u32::from(record.codestream);
        if codestream >= scan.codestreams {
            return Err(Jpeg2000Error::InvalidBox {
                offset: scan.layer.map_or(0, |box_view| box_view.offset()),
                kind: BoxKind::CodestreamRegistration,
                reason: "registers a codestream the file does not contain",
            });
        }
        Ok(codestream)
    }

    /// Checks that ihdr describes the same image as the selected codestream.
    pub(crate) fn validate_size(&self, size: &SizeHeader<'_>) -> Result<(), Jpeg2000Error> {
        if self.metadata.shape() == size.shape() {
            return Ok(());
        }
        Err(Jpeg2000Error::InvalidBox {
            offset: self.codestream_offset,
            kind: BoxKind::ImageHeader,
            reason: "Image Header does not match codestream SIZ",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pdf_graphics::Size;

    use crate::{Decoder, DecoderLimits, DecoderOptions, InputFormat, Jpeg2000Error};

    #[test]
    fn reads_cached_jp2_and_jpx_headers() {
        let limits = DecoderLimits {
            max_input_bytes: 2_000_000,
            max_pixels: 2_000_000,
            max_components: 8,
            max_tiles: 1_024,
            max_working_bytes: 2_000_000,
        };
        let cases = [
            (
                "openjpeg-data/input/conformance/file1.jp2",
                InputFormat::Jp2,
                768,
                512,
            ),
            (
                "openjpeg-data/input/conformance/file7.jp2",
                InputFormat::Jpx,
                480,
                640,
            ),
            (
                "openjpeg-data/input/conformance/file9.jp2",
                InputFormat::Jp2,
                768,
                512,
            ),
            ("pdfium/testing/resources/CMYK.jpf", InputFormat::Jpx, 4, 4),
        ];
        for (name, format, width, height) in cases {
            let path = format!("{}/fixtures/cache/{name}", env!("CARGO_MANIFEST_DIR"));
            let Ok(bytes) = fs::read(path) else { continue };
            let options = DecoderOptions {
                format: InputFormat::Auto,
                limits,
            };
            let decoder = Decoder::new(&bytes, options).parse_header().expect(name);
            let header = decoder.header();
            assert_eq!(header.format(), format, "{name}");
            let jp2 = header.jp2().expect("container metadata");
            assert_eq!(jp2.image_size(), Size { width, height }, "{name}");
        }
    }

    #[test]
    fn rejects_cached_malformed_headers() {
        let limits = DecoderLimits {
            max_input_bytes: 4_000_000,
            max_pixels: 1_000_000,
            max_components: 8,
            max_tiles: 1_024,
            max_working_bytes: 4_000_000,
        };
        let cases = [
            "openjpeg-data/input/nonregression/issue364-38.jp2",
            "openjpeg-data/input/nonregression/issue427-null-image-size.jp2",
            "openjpeg-data/input/nonregression/issue400.jp2",
            "openjpeg-data/input/nonregression/gdal_fuzzer_assert_in_opj_j2k_read_SQcd_SQcc.patch.jp2",
            "openjpeg-data/input/nonregression/issue432.jp2",
            "jpylyzer-test-files/files/bitwiser-headerbox-corrupted-boxlength-22181.jp2",
        ];
        for name in cases {
            let path = format!("{}/fixtures/cache/{name}", env!("CARGO_MANIFEST_DIR"));
            let Ok(bytes) = fs::read(path) else { continue };
            let options = DecoderOptions {
                format: InputFormat::Auto,
                limits,
            };
            let result = Decoder::new(&bytes, options).parse_header();
            assert!(result.is_err(), "{name}");
        }
        let name = "openjpeg-data/input/nonregression/htj2k/Bretagne1_ht.j2k";
        let path = format!("{}/fixtures/cache/{name}", env!("CARGO_MANIFEST_DIR"));
        if let Ok(bytes) = fs::read(path) {
            let options = DecoderOptions {
                format: InputFormat::Codestream,
                limits,
            };
            let error = Decoder::new(&bytes, options)
                .parse_header()
                .expect_err(name);
            assert!(
                matches!(error, Jpeg2000Error::UnsupportedFeature { .. }),
                "{error}"
            );
        }
    }
}
