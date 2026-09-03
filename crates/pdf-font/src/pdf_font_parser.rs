use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::base_encoding::BaseEncoding;
use crate::font::{
    FontMetadata, FontProgramFormat, FontSlant, FontSource, FontStretch, FontWeight, GlyphId,
    GlyphName,
};
use crate::pdf::{
    CidFontKind, CidFontSpec, CidSystemInfo, PdfFontDescriptor, PdfGlyphMetric, PdfMetrics,
    SimpleEncoding, SimpleFontSpec, ToUnicodeMap, Type0FontSpec, Type3FontSpec,
};
use crate::pdf_font_spec::PdfFontSpec;
use bytes::Bytes;
use pdf_cmap::{
    IdentityToUnicodeMap, ToUnicodeCMap, Type0EncodingCMap, predefined::PredefinedCMap,
};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    PDF_GLYPH_SPACE_UNITS_PER_EM, cff_builder::build_cff_font, encoding::Encoding,
    error::FontError, fallback::fallback_standard14_font, flags::FontFlags,
    glyph_widths_map::GlyphWidthsMap, simple_font_glyph_map::SimpleFontGlyphWidthsMap, standard14,
};

/// Parses a PDF font dictionary, substituting a normalized fallback spec when needed.
#[must_use]
pub fn from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    id_allocator: &mut ContentStreamIdAllocator,
) -> PdfFontSpec {
    match try_from_dictionary(dictionary, objects, id_allocator) {
        Ok(spec) => spec,
        Err(_) => fallback_spec(dictionary, objects),
    }
}

fn try_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<PdfFontSpec, FontError> {
    match dictionary.required_bytes(b"Subtype", objects)? {
        b"Type0" => parse_type0(dictionary, objects),
        b"Type1" | b"MMType1" => parse_type1(dictionary, objects),
        b"TrueType" => parse_true_type(dictionary, objects),
        b"Type3" => parse_type3(dictionary, objects, id_allocator),
        other => Err(FontError::UnsupportedFontSubtype {
            subtype: String::from_utf8_lossy(other).into_owned(),
        }),
    }
}

fn parse_type1(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<PdfFontSpec, FontError> {
    let descriptor = descriptor(dictionary, objects)?;
    let encoding = simple_encoding(dictionary, objects, false)?;
    let metrics = simple_metrics(dictionary, objects, descriptor.missing_width)?;
    let to_unicode = to_unicode(dictionary, objects)?;
    let standard14 = base_font(dictionary, objects)
        .and_then(|name| standard14::from_base_font_name(name.as_ref()));
    let program = dictionary
        .optional_dictionary(b"FontDescriptor", objects)?
        .and_then(|font_descriptor| {
            if font_descriptor.get(b"FontFile3").is_some() {
                read_simple_cff_program(font_descriptor, objects).ok()
            } else {
                read_type1_program(font_descriptor, objects).ok()
            }
        });
    let font = SimpleFontSpec {
        base_font: base_font(dictionary, objects).unwrap_or_default(),
        descriptor,
        program,
        standard14,
        encoding,
        metrics,
        to_unicode,
    };
    let spec = if dictionary.required_bytes(b"Subtype", objects)? == b"MMType1" {
        PdfFontSpec::MultipleMasterType1(font)
    } else {
        PdfFontSpec::Type1(font)
    };
    Ok(spec)
}

fn parse_true_type(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<PdfFontSpec, FontError> {
    let descriptor = descriptor(dictionary, objects)?;
    let symbolic = descriptor.metadata.symbolic;
    let encoding = simple_encoding(dictionary, objects, symbolic)?;
    let metrics = simple_metrics(dictionary, objects, descriptor.missing_width)?;
    let to_unicode = to_unicode(dictionary, objects)?;
    let program = dictionary
        .optional_dictionary(b"FontDescriptor", objects)?
        .and_then(|font_descriptor| {
            font_descriptor
                .optional_stream(b"FontFile2", objects)
                .ok()
                .flatten()
        })
        .map(|stream| FontSource::Memory {
            data: stream.shared_data(),
            format: FontProgramFormat::TrueType,
            face_index: 0,
        });
    Ok(PdfFontSpec::TrueType(SimpleFontSpec {
        base_font: base_font(dictionary, objects).unwrap_or_default(),
        descriptor,
        program,
        standard14: base_font(dictionary, objects)
            .and_then(|name| standard14::from_base_font_name(name.as_ref())),
        encoding,
        metrics,
        to_unicode,
    }))
}

fn parse_type0(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<PdfFontSpec, FontError> {
    let descendants = dictionary.required_array(b"DescendantFonts", objects)?;
    if descendants.len() != 1 {
        return Err(FontError::InvalidDescendantFonts(
            "Expected exactly one descendant font",
        ));
    }
    let descendant = descendants
        .first()
        .ok_or(FontError::InvalidDescendantFonts("Array is empty"))?
        .try_dictionary(objects)?;
    let kind = match descendant.required_bytes(b"Subtype", objects)? {
        b"CIDFontType0" => CidFontKind::Type0,
        b"CIDFontType2" => CidFontKind::Type2,
        other => {
            return Err(FontError::UnsupportedCidFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            });
        }
    };
    let descriptor_value = descriptor(descendant, objects)?;
    let program = descendant
        .optional_dictionary(b"FontDescriptor", objects)?
        .and_then(|font_descriptor| match kind {
            CidFontKind::Type0 => read_cid_cff_program(font_descriptor, objects).ok(),
            CidFontKind::Type2 => font_descriptor
                .optional_stream(b"FontFile2", objects)
                .ok()
                .flatten()
                .map(|stream| FontSource::Memory {
                    data: stream.shared_data(),
                    format: FontProgramFormat::TrueType,
                    face_index: 0,
                }),
        });
    let encoding = Type0EncodingCMap::from_dictionary(dictionary, objects)?
        .unwrap_or(Type0EncodingCMap::from_name(b"Identity-H")?);
    let metrics = cid_metrics(descendant, objects)?;
    let system_info = cid_system_info(descendant, objects)?;
    let cid_to_unicode = cid_to_unicode_map(&system_info);
    Ok(PdfFontSpec::Type0(Type0FontSpec {
        base_font: base_font(dictionary, objects).unwrap_or_default(),
        encoding: Arc::new(encoding),
        descendant: CidFontSpec {
            kind,
            descriptor: descriptor_value,
            program,
            system_info,
            metrics,
            cid_to_gid: None,
            cid_to_unicode,
        },
        to_unicode: to_unicode(dictionary, objects)?,
    }))
}

#[allow(clippy::arc_with_non_send_sync)]
fn parse_type3(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<PdfFontSpec, FontError> {
    let [a, b, c, d, e, f] = dictionary.required_array_of::<f32, 6>(b"FontMatrix", objects)?;
    let [left, top, right, bottom] =
        dictionary.required_array_of::<f32, 4>(b"FontBBox", objects)?;
    let encoding = simple_encoding(dictionary, objects, false)?;
    let metrics = simple_metrics(dictionary, objects, Some(0.0))?;
    let char_proc_dictionary = dictionary.required_dictionary(b"CharProcs", objects)?;
    let mut procedures = HashMap::new();
    let mut handles = BTreeMap::new();
    for (name, value) in &char_proc_dictionary.dictionary {
        let stream = ContentStream::new(value, objects, id_allocator)?;
        let handle = GlyphId(u32::try_from(stream.id).map_err(|_| {
            FontError::InvalidDescendantFonts("Type 3 content stream ID does not fit u32")
        })?);
        handles.insert(GlyphName(Arc::from(name.as_slice())), handle);
        procedures.insert(handle, stream);
    }
    Ok(PdfFontSpec::Type3(Type3FontSpec {
        base_font: base_font(dictionary, objects).unwrap_or_default(),
        metadata: FontMetadata::default(),
        font_matrix: Transform::from_row(a, b, c, d, e, f),
        bounds: Rect {
            left,
            top,
            right,
            bottom,
        },
        encoding: encoding.clone(),
        metrics,
        char_procedures: Arc::new(handles),
        type3_procedures: Arc::new(procedures),
        to_unicode: to_unicode(dictionary, objects)?,
    }))
}

fn descriptor(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<PdfFontDescriptor, FontError> {
    let descriptor = dictionary.optional_dictionary(b"FontDescriptor", objects)?;
    let flags = FontFlags::from_dictionary(dictionary, objects)?;
    let family = base_font(dictionary, objects)
        .map(|name| Arc::<str>::from(String::from_utf8_lossy(name.as_ref()).into_owned()));
    let bounds = descriptor
        .and_then(|value| value.get(b"FontBBox"))
        .and_then(|value| value.try_array(objects).ok())
        .and_then(|values| {
            let [left, top, right, bottom] = values else {
                return None;
            };
            Some(Rect {
                left: left.try_number(objects).ok()?,
                top: top.try_number(objects).ok()?,
                right: right.try_number(objects).ok()?,
                bottom: bottom.try_number(objects).ok()?,
            })
        });
    let missing_width = descriptor
        .and_then(|value| value.optional_number::<f32>(b"MissingWidth", objects).ok())
        .flatten();
    let italic_angle = descriptor
        .and_then(|value| value.optional_number::<f32>(b"ItalicAngle", objects).ok())
        .flatten();
    let stem_v = descriptor
        .and_then(|value| value.optional_number::<f32>(b"StemV", objects).ok())
        .flatten();
    Ok(PdfFontDescriptor {
        metadata: FontMetadata {
            postscript_name: family.clone(),
            family,
            subfamily: None,
            weight: FontWeight(if flags.contains(FontFlags::FORCE_BOLD) {
                700
            } else {
                400
            }),
            stretch: FontStretch::Normal,
            slant: if flags.contains(FontFlags::ITALIC) {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
            symbolic: flags.contains(FontFlags::SYMBOLIC),
        },
        bounds,
        missing_width,
        italic_angle,
        stem_v,
    })
}

fn simple_encoding(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    symbolic: bool,
) -> Result<SimpleEncoding, FontError> {
    let encoding = Encoding::from_dictionary(dictionary, objects)?.unwrap_or_else(|| {
        if symbolic {
            Encoding { names: Vec::new() }
        } else {
            Encoding::default()
        }
    });
    let differences = encoding
        .names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            u8::try_from(index)
                .ok()
                .map(|code| (code, GlyphName(Arc::from(name.as_ref()))))
        })
        .collect();
    Ok(SimpleEncoding {
        base: BaseEncoding::BuiltIn,
        differences,
    })
}

fn simple_metrics(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    missing_width: Option<f32>,
) -> Result<PdfMetrics, FontError> {
    let explicit = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?
        .unwrap_or_default()
        .into_iter()
        .map(|(code, width)| (u32::from(code), horizontal_metric(width)))
        .collect();
    Ok(PdfMetrics {
        default: horizontal_metric(missing_width.unwrap_or(500.0)),
        explicit,
    })
}

fn cid_metrics(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<PdfMetrics, FontError> {
    let default = dictionary
        .optional_number::<f32>(b"DW", objects)?
        .unwrap_or(PDF_GLYPH_SPACE_UNITS_PER_EM);
    let mut explicit = BTreeMap::new();
    if let Some(widths) = GlyphWidthsMap::from_dictionary(dictionary, objects)? {
        for code in 0_u16..=u16::MAX {
            if let Some(width) = widths.get_width(code) {
                explicit.insert(u32::from(code), horizontal_metric(width));
            }
        }
    }
    Ok(PdfMetrics {
        default: horizontal_metric(default),
        explicit,
    })
}

const fn horizontal_metric(width: f32) -> PdfGlyphMetric {
    PdfGlyphMetric {
        advance_x: width,
        advance_y: 0.0,
        vertical_origin_x: None,
        vertical_origin_y: None,
    }
}

fn to_unicode(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<Arc<dyn ToUnicodeMap>>, FontError> {
    if let Some(map) = ToUnicodeCMap::from_dictionary(dictionary, objects)? {
        let map: Arc<dyn ToUnicodeMap> = Arc::new(map);
        return Ok(Some(map));
    }

    Ok(dictionary
        .get(b"ToUnicode")
        .and_then(|value| value.try_bytes(objects).ok())
        .and_then(IdentityToUnicodeMap::from_name)
        .map(|map| {
            let map: Arc<dyn ToUnicodeMap> = Arc::new(map);
            map
        }))
}

fn base_font(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> Option<Arc<[u8]>> {
    dictionary
        .get(b"BaseFont")
        .and_then(|value| value.try_bytes(objects).ok())
        .map(Arc::from)
}

fn read_type1_program(
    descriptor: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<FontSource, FontError> {
    let stream = descriptor.required_stream(b"FontFile", objects)?;
    Ok(FontSource::Memory {
        data: stream.shared_data(),
        format: FontProgramFormat::Type1,
        face_index: 0,
    })
}

fn read_simple_cff_program(
    descriptor: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<FontSource, FontError> {
    let stream = descriptor.required_stream(b"FontFile3", objects)?;
    let subtype = stream
        .dictionary
        .optional_bytes(b"Subtype", objects)?
        .unwrap_or(b"OpenType");
    let data = match subtype {
        b"Type1C" | b"CIDFontType0C" => Bytes::from(build_cff_font(stream.raw_data())?),
        b"OpenType" => stream.shared_data(),
        other => {
            return Err(FontError::UnsupportedFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            });
        }
    };
    Ok(FontSource::Memory {
        data,
        format: FontProgramFormat::OpenTypeCff,
        face_index: 0,
    })
}

/// Reads a CID-keyed CFF program without disguising standalone CFF as OpenType.
///
/// Raw `CIDFontType0C` data is handled directly by the Type 0 raw-CFF driver. A
/// `Type1C` label is accepted here for compatibility with producers that use
/// the generic CFF subtype for a CID-keyed descendant; structural validation in
/// the driver still rejects name-keyed data.
fn read_cid_cff_program(
    descriptor: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<FontSource, FontError> {
    let stream = descriptor.required_stream(b"FontFile3", objects)?;
    let subtype = stream
        .dictionary
        .optional_bytes(b"Subtype", objects)?
        .unwrap_or(b"OpenType");
    let format = match subtype {
        b"Type1C" | b"CIDFontType0C" => FontProgramFormat::CidCff,
        b"OpenType" => FontProgramFormat::OpenTypeCff,
        other => {
            return Err(FontError::UnsupportedFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            });
        }
    };
    Ok(FontSource::Memory {
        data: stream.shared_data(),
        format,
        face_index: 0,
    })
}

fn cid_system_info(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<CidSystemInfo, FontError> {
    let info = dictionary.optional_dictionary(b"CIDSystemInfo", objects)?;
    Ok(CidSystemInfo {
        registry: info
            .and_then(|value| value.optional_bytes(b"Registry", objects).ok())
            .flatten()
            .map(Arc::from)
            .unwrap_or_default(),
        ordering: info
            .and_then(|value| value.optional_bytes(b"Ordering", objects).ok())
            .flatten()
            .map(Arc::from)
            .unwrap_or_default(),
        supplement: info
            .and_then(|value| value.optional_number::<u32>(b"Supplement", objects).ok())
            .flatten()
            .unwrap_or_default(),
    })
}

fn cid_to_unicode_map(system_info: &CidSystemInfo) -> Option<Arc<HashMap<u16, char>>> {
    let cmap_name: &[u8] = match system_info.ordering.as_ref() {
        b"Japan1" => b"UniJIS-UCS2-HW-H",
        b"GB1" => b"UniGB-UCS2-H",
        b"CNS1" => b"UniCNS-UCS2-H",
        b"Korea1" => b"UniKS-UCS2-H",
        _ => return None,
    };
    PredefinedCMap::from_name(cmap_name)
        .ok()
        .flatten()
        .map(|cmap| Arc::new(cmap.cid_to_unicode_map()))
}

fn fallback_spec(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> PdfFontSpec {
    let standard14 = fallback_standard14_font(dictionary, None, objects);
    let name = Arc::from(standard14.to_string().into_bytes());
    PdfFontSpec::Type1(SimpleFontSpec {
        base_font: name,
        descriptor: PdfFontDescriptor::default(),
        program: None,
        standard14: Some(standard14),
        encoding: SimpleEncoding {
            base: BaseEncoding::Standard,
            differences: BTreeMap::new(),
        },
        metrics: PdfMetrics {
            default: horizontal_metric(500.0),
            explicit: BTreeMap::new(),
        },
        to_unicode: None,
    })
}

#[cfg(test)]
#[clippy::allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use pdf_cmap::{PdfCode, UnicodeSequence};
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::{
        FontProgramFormat, FontSource, read_cid_cff_program, read_simple_cff_program, to_unicode,
    };

    fn descriptor_with_cff(subtype: &[u8], data: &'static [u8]) -> Dictionary {
        let stream_dictionary = Dictionary::from_entries([(
            b"Subtype".as_slice(),
            ObjectVariant::Name(subtype.to_vec()),
        )]);
        let stream = StreamObject::new(1, 0, stream_dictionary, data);
        Dictionary::from_entries([(b"FontFile3".as_slice(), ObjectVariant::Stream(stream))])
    }

    fn memory_source(source: FontSource) -> (bytes::Bytes, FontProgramFormat) {
        let FontSource::Memory { data, format, .. } = source else {
            panic!("an embedded CFF program should be an in-memory source");
        };
        (data, format)
    }

    #[test]
    fn cid_font_type0c_remains_raw_cff() {
        let descriptor = descriptor_with_cff(b"CIDFontType0C", b"raw-cff");
        let source = read_cid_cff_program(&descriptor, &PassthroughResolver)
            .expect("the raw CID-CFF source should parse");
        let (data, format) = memory_source(source);

        assert_eq!(format, FontProgramFormat::CidCff);
        assert_eq!(data.as_ref(), b"raw-cff");
    }

    #[test]
    fn cid_open_type_remains_open_type() {
        let descriptor = descriptor_with_cff(b"OpenType", b"open-type");
        let source = read_cid_cff_program(&descriptor, &PassthroughResolver)
            .expect("the OpenType source should parse");
        let (data, format) = memory_source(source);

        assert_eq!(format, FontProgramFormat::OpenTypeCff);
        assert_eq!(data.as_ref(), b"open-type");
    }

    #[test]
    fn simple_type1c_is_still_wrapped_as_open_type() {
        let descriptor = descriptor_with_cff(b"Type1C", b"raw-cff");
        let source = read_simple_cff_program(&descriptor, &PassthroughResolver)
            .expect("the simple CFF source should be wrapped");
        let (data, format) = memory_source(source);

        assert_eq!(format, FontProgramFormat::OpenTypeCff);
        assert_ne!(data.as_ref(), b"raw-cff");
    }

    #[test]
    fn named_identity_to_unicode_map_is_supported() {
        let dictionary = Dictionary::from_entries([(
            b"ToUnicode".as_slice(),
            ObjectVariant::Name(b"Identity-H".to_vec()),
        )]);
        let map = to_unicode(&dictionary, &PassthroughResolver)
            .expect("the identity map should parse")
            .expect("the identity map should be present");
        let code = PdfCode::new(0x11B, 2).expect("the Czech character code should be valid");

        assert_eq!(map.map(code), Some(UnicodeSequence::from('ě')));
    }

    #[test]
    fn unsupported_named_to_unicode_map_is_ignored() {
        let dictionary = Dictionary::from_entries([(
            b"ToUnicode".as_slice(),
            ObjectVariant::Name(b"Unsupported-H".to_vec()),
        )]);

        assert!(
            to_unicode(&dictionary, &PassthroughResolver)
                .expect("unsupported names should remain non-fatal")
                .is_none()
        );
    }
}
