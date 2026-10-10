use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult,
    object_kind::ObjectKind, object_variant::ObjectVariant,
};

use crate::{error::PdfPagesError, resource::Resource, soft_mask::SoftMask};
use num_traits::FromPrimitive;
use pdf_graphics::{BlendMode, DashPattern, LineCap, LineJoin};

/// One parsed entry of a graphics state parameter dictionary (`ExtGState`).
///
/// An `ExtGState` dictionary, selected with the `gs` operator, sets several graphics state
/// parameters at once. Only the entries the renderer applies have a variant; other keys,
/// and entries whose value is null or a missing object, are skipped while reading.
pub enum ExternalGraphicsStateKey {
    /// Line width (`LW`) for stroked paths, in user space units.
    LineWidth(f32),
    /// Line cap style (`LC`) applied to the ends of open stroked subpaths.
    LineCap(LineCap),
    /// Line join style (`LJ`) applied to the corners of stroked paths.
    LineJoin(LineJoin),
    /// Miter limit (`ML`): the largest ratio of miter length to line width before a miter
    /// join is drawn as a bevel.
    MiterLimit(f32),
    /// Dash pattern (`D`), read from a `[dash_array dash_phase]` array.
    ///
    /// An empty dash array means a solid line and produces no entry.
    DashPattern(DashPattern),
    /// Text font (`Font`), read from a `[font size]` array: the shared font resource and
    /// the font size.
    Font(pdf_object_reader::ObjectHandle<Resource>, f32),
    /// Blend mode (`BM`) used when compositing.
    ///
    /// For an array of names this is the first one recognized, or [`BlendMode::Normal`]
    /// when none is.
    BlendMode(BlendMode),
    /// Soft mask (`SMask`): `Some` for a soft mask dictionary, `None` for the name `/None`,
    /// which removes the current soft mask.
    SoftMask(Option<pdf_object_reader::ObjectHandle<SoftMask>>),
    /// Stroking alpha constant (`CA`): the opacity of stroking operations, from 0.0 to 1.0.
    StrokingAlpha(f32),
    /// Non-stroking alpha constant (`ca`): the opacity of all other painting operations,
    /// from 0.0 to 1.0.
    NonStrokingAlpha(f32),
}

pub struct ExternalGraphicsState {
    pub params: Vec<ExternalGraphicsStateKey>,
}

impl FromPdfObject for ExternalGraphicsState {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let mut params = Vec::new();
        for (name, value) in &context.dictionary().dictionary {
            if name == b"Type" {
                continue;
            }
            if let Some(param) = parse_entry(name, value, &mut context)? {
                params.push(param);
            }
        }
        Ok(Self { params })
    }
}

/// A `/D` entry, `[dash_array dash_phase]`; `None` when the empty dash array draws a
/// solid line.
struct DashEntry(Option<DashPattern>);

impl FromPdfObject for DashEntry {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut array = context.array()?;
        let found = array.array().len();
        if found != 2 {
            return Err(PdfPagesError::InvalidExtGStateArrayLength { entry: "D", found }.into());
        }
        let intervals: Vec<f32> = array.at(0)?;
        let phase: f32 = array.at(1)?;
        Ok(Self(
            DashPattern::new(&intervals, phase).map_err(PdfPagesError::from)?,
        ))
    }
}

/// A `/Font` entry, `[font size]`: the shared font resource and the font size.
struct FontEntry(ObjectHandle<Resource>, f32);

impl FromPdfObject for FontEntry {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut array = context.array()?;
        let found = array.array().len();
        if found != 2 {
            return Err(PdfPagesError::InvalidExtGStateArrayLength {
                entry: "Font",
                found,
            }
            .into());
        }
        Ok(Self(array.shared_at(0)?, array.at(1)?))
    }
}

/// A `/BM` entry: a blend mode name, or an array listing blend modes in order of
/// preference, where the first recognized one applies and `Normal` is used when none is.
struct BlendModeEntry(BlendMode);

impl FromPdfObject for BlendModeEntry {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let objects = context.source();
        let value = context.object().value();
        let ObjectVariant::Array(names) = value else {
            return Ok(Self(BlendMode::from(value.try_bytes(objects)?)));
        };
        for name in names.iter() {
            let mode = BlendMode::from(name.try_bytes(objects)?);
            if !matches!(mode, BlendMode::Unknown(_)) {
                return Ok(Self(mode));
            }
        }
        Ok(Self(BlendMode::Normal))
    }
}

/// Parse a single key/value pair of the ExtGState dictionary.
///
/// Returns `Ok(None)` for unrecognized keys, which are silently ignored
/// per the PDF specification.
fn parse_entry<A: ObjectAccess + ?Sized>(
    name: &[u8],
    value: &ObjectVariant,
    context: &mut DictionaryContext<'_, A>,
) -> ReadResult<Option<ExternalGraphicsStateKey>> {
    // A null value or a reference to a nonexistent object is equivalent to
    // an absent entry.
    if context.is_absent(value)? {
        return Ok(None);
    }
    let parsed = match name {
        b"LW" => ExternalGraphicsStateKey::LineWidth(context.read(value)?),
        b"LC" => {
            let cap = context.read::<i32>(value)?;
            ExternalGraphicsStateKey::LineCap(
                LineCap::from_i32(cap).ok_or(PdfPagesError::InvalidExtGStateLineCap(cap))?,
            )
        }
        b"LJ" => {
            let join = context.read::<i32>(value)?;
            ExternalGraphicsStateKey::LineJoin(
                LineJoin::from_i32(join).ok_or(PdfPagesError::InvalidExtGStateLineJoin(join))?,
            )
        }
        b"ML" => ExternalGraphicsStateKey::MiterLimit(context.read(value)?),
        b"D" => match context.read::<DashEntry>(value)? {
            DashEntry(Some(pattern)) => ExternalGraphicsStateKey::DashPattern(pattern),
            DashEntry(None) => return Ok(None),
        },
        b"Font" => {
            let FontEntry(font, size) = context.read(value)?;
            ExternalGraphicsStateKey::Font(font, size)
        }
        b"BM" => ExternalGraphicsStateKey::BlendMode(context.read::<BlendModeEntry>(value)?.0),
        // A soft mask dictionary is read as a shared handle from this dictionary, since a
        // decoder of the value itself cannot read that same value shared.
        b"SMask" => {
            let soft_mask = if context.resolve(value)?.kind() == ObjectKind::Dictionary {
                Some(context.read_shared::<SoftMask>(value)?)
            } else if value.is_named(b"None", context.source()) {
                None
            } else {
                return Err(PdfPagesError::InvalidExtGStateSoftMask.into());
            };
            ExternalGraphicsStateKey::SoftMask(soft_mask)
        }
        b"CA" => ExternalGraphicsStateKey::StrokingAlpha(context.read(value)?),
        b"ca" => ExternalGraphicsStateKey::NonStrokingAlpha(context.read(value)?),
        _ => return Ok(None),
    };

    Ok(Some(parsed))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use crate::error::PdfPagesError;

    use super::{ExternalGraphicsState, ExternalGraphicsStateKey};

    fn dash_dict(dash_array: Vec<ObjectVariant>, dash_phase: f32) -> Dictionary {
        Dictionary::new(BTreeMap::from([(
            Vec::from(b"D"),
            ObjectVariant::Array(
                vec![
                    ObjectVariant::Array(dash_array.into()),
                    ObjectVariant::Real(f64::from(dash_phase)),
                ]
                .into(),
            ),
        )]))
    }

    fn parse_ext_gstate(
        dictionary: &Dictionary,
    ) -> pdf_object_reader::ReadResult<Option<ExternalGraphicsState>> {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        reader.read::<Option<ExternalGraphicsState>>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((dictionary).clone()),
        )
    }

    #[test]
    fn extgstate_dash_entry_is_typed() {
        let dictionary = dash_dict(
            vec![ObjectVariant::Real(3.0), ObjectVariant::Real(1.0)],
            2.0,
        );

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let ext_gstate = reader
            .read::<Option<ExternalGraphicsState>>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert_eq!(ext_gstate.params.len(), 1);
        match &ext_gstate.params[0] {
            ExternalGraphicsStateKey::DashPattern(pattern) => {
                assert_eq!(pattern.intervals, vec![3.0, 1.0]);
                assert_eq!(pattern.phase, 2.0);
            }
            _ => panic!("unexpected extgstate entry"),
        }
    }

    #[test]
    fn invalid_dash_entry_surfaces_as_value_error() {
        let dictionary = dash_dict(
            vec![ObjectVariant::Real(0.0), ObjectVariant::Real(0.0)],
            0.0,
        );

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let error = match reader.read::<Option<ExternalGraphicsState>>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&dictionary).clone()),
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid dash pattern should fail"),
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::InvalidExtGStateDashPattern(_)))
        ));
    }

    #[test]
    fn empty_dash_array_is_ignored() {
        let dictionary = dash_dict(Vec::new(), 0.0);

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let ext_gstate = reader
            .read::<Option<ExternalGraphicsState>>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert!(ext_gstate.params.is_empty());
    }

    #[test]
    fn soft_mask_none_is_preserved() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"SMask"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"None".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]));

        let ext_gstate = parse_ext_gstate(&dictionary)
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert!(matches!(
            ext_gstate.params.as_slice(),
            [ExternalGraphicsStateKey::SoftMask(None)]
        ));
    }

    #[test]
    fn invalid_soft_mask_name_is_rejected() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"SMask"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"Invalid".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]));

        let error = match parse_ext_gstate(&dictionary) {
            Err(error) => error,
            Ok(_) => panic!("invalid soft mask should fail"),
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::InvalidExtGStateSoftMask))
        ));
    }
}
