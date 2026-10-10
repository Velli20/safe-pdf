//! Top-level parsing for PDF shading dictionaries and streams.
//!
//! Mesh stream parsing lives in the dedicated free-form and patch-mesh
//! modules. This module is responsible only for dispatching by shading type
//! and parsing the non-mesh shading dictionaries.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::{color::Color, interval::Interval};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, dictionary::Dictionary,
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    color_stops::ColorStops,
    error::PdfShadingError,
    free_form_mesh::parse_free_form_triangle_mesh,
    function_shading::FunctionShading,
    model::{Shading, ShadingType},
    patch_mesh::parse_patch_mesh,
};

impl FromPdfObject for Shading {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object().value();
        let objects = context.source();
        let dictionary = object.try_dictionary(objects)?;
        let shading_type = dictionary
            .required_number::<i32>(b"ShadingType", objects)?
            .try_into()?;

        match shading_type {
            ShadingType::FunctionBased => {
                FunctionShading::parse(dictionary, objects).map(Shading::FunctionBased)
            }
            ShadingType::Axial => parse_axial(dictionary, objects),
            ShadingType::Radial => parse_radial(dictionary, objects),
            ShadingType::FreeFormTriangleMesh => parse_free_form_triangle_mesh(object, objects),
            ShadingType::CoonsPatchMesh | ShadingType::TensorProductPatchMesh => {
                parse_patch_mesh(object, objects, shading_type)
            }
            unsupported => Ok(Shading::Unsupported {
                name: unsupported.to_string(),
            }),
        }
        .map_err(pdf_object_reader::ObjectReadError::from)
    }
}

/// Parses a single function or function array from a shading dictionary.
pub(crate) fn parse_functions(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Vec<Function>, PdfShadingError> {
    let function = objects.resolve_object(dictionary.get_or_err(b"Function")?)?;

    match function {
        ObjectVariant::Array(functions) => functions
            .iter()
            .map(|value| Function::parse(value, objects).map_err(PdfShadingError::from))
            .collect(),
        value => Ok(vec![Function::parse(value, objects)?]),
    }
}

/// Parses a Type 2 axial shading dictionary.
fn parse_axial(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let coords = dictionary.required_array_of::<f32, 4>(b"Coords", objects)?;
    let domain = dictionary
        .optional_interval(b"Domain", objects)?
        .unwrap_or(Interval::UNIT);
    let color_space = required_color_space(dictionary, objects)?;
    let function = Function::parse(dictionary.get_or_err(b"Function")?, objects)?;
    let color_stops = ColorStops::from_function_domain(&function, &color_space, domain)?;
    let extend = read_extend(dictionary, objects)?;
    let background = read_background(dictionary, objects, &color_space)?;
    let bbox = dictionary.optional_bbox(objects)?;

    Ok(Shading::Axial {
        color_space,
        coords,
        color_stops,
        extend,
        background,
        bbox,
    })
}

/// Parses a Type 3 radial shading dictionary.
fn parse_radial(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let coords = dictionary.required_array_of::<f32, 6>(b"Coords", objects)?;
    let domain = dictionary
        .optional_interval(b"Domain", objects)?
        .unwrap_or(Interval::UNIT);
    let color_space = required_color_space(dictionary, objects)?;
    let bbox = dictionary.optional_bbox(objects)?;
    let function = Function::parse(dictionary.get_or_err(b"Function")?, objects)?;
    let color_stops = ColorStops::from_function_domain(&function, &color_space, domain)?;
    let extend = read_extend(dictionary, objects)?;
    let background = read_background(dictionary, objects, &color_space)?;

    Ok(Shading::Radial {
        color_space,
        coords,
        color_stops,
        extend,
        background,
        bbox,
    })
}

/// Reads `/Extend`, which says whether an axial or radial shading continues past its
/// start and end. Both ends default to not extending.
fn read_extend(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<[bool; 2], PdfShadingError> {
    let Some(extend) = dictionary.optional_array(b"Extend", objects)? else {
        return Ok([false; 2]);
    };
    let extend = extend.as_slice();
    Ok([
        extend.optional_boolean(0, objects)?.unwrap_or(false),
        extend.optional_boolean(1, objects)?.unwrap_or(false),
    ])
}

/// Reads `/Background` as a color in the shading color space.
///
/// A background whose components the color space rejects is ignored rather than
/// failing the shading.
pub(crate) fn read_background(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    color_space: &ColorSpace,
) -> Result<Option<Color>, PdfShadingError> {
    Ok(dictionary
        .optional_vec_of::<f32>(b"Background", objects)?
        .and_then(|components| color_space.apply(&components).ok()))
}

/// Reads the required color space shared by shading types 1 through 7.
pub(crate) fn required_color_space(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<ColorSpace, PdfShadingError> {
    ColorSpace::from_dictionary(dictionary, objects)?.ok_or(PdfShadingError::MissingRequiredEntry {
        entry: "ColorSpace",
    })
}
