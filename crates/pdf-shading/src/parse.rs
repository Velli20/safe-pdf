//! Top-level parsing for PDF shading dictionaries and streams.
//!
//! Mesh stream parsing lives in the dedicated free-form and patch-mesh
//! modules. This module is responsible only for dispatching by shading type
//! and parsing the non-mesh shading dictionaries.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    color_stops::ColorStops,
    error::PdfShadingError,
    free_form_mesh::parse_free_form_triangle_mesh,
    model::{Shading, ShadingType},
    patch_mesh::parse_patch_mesh,
};

/// Parses a PDF shading object from a dictionary or stream object.
pub fn shading_from_dictionary(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let dictionary = object.try_dictionary(objects)?;
    let shading_type = dictionary
        .required_number::<i32>("ShadingType", objects)?
        .try_into()?;

    match shading_type {
        ShadingType::FunctionBased => parse_function_based(dictionary, objects),
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
}

/// Parses a single function or function array from a shading dictionary.
pub(crate) fn parse_functions(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Vec<Function>, PdfShadingError> {
    let function = objects.resolve_object(dictionary.get_or_err("Function")?)?;

    match function {
        ObjectVariant::Array(functions) => functions
            .iter()
            .map(|value| Function::parse(value, objects).map_err(PdfShadingError::from))
            .collect(),
        value => Ok(vec![Function::parse(value, objects)?]),
    }
}

/// Parses a Type 1 function-based shading dictionary.
fn parse_function_based(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let color_space = ColorSpace::from_dictionary(dictionary, objects)?;
    let background = dictionary.optional_vec_of::<f32>("Background", objects)?;
    let bbox = dictionary.optional_bbox(objects)?;
    let domain = dictionary.optional_vec_of::<f32>("Domain", objects)?;
    let anti_alias = dictionary.optional_boolean("AntiAlias", objects)?;
    let functions = parse_functions(dictionary, objects)?;

    Ok(Shading::FunctionBased {
        color_space,
        background,
        bbox,
        anti_alias,
        domain,
        functions,
    })
}

/// Parses a Type 2 axial shading dictionary.
fn parse_axial(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let coords = dictionary.required_array_of::<f32, 4>("Coords", objects)?;
    let color_space = required_color_space(dictionary, objects)?;
    let function = Function::parse(dictionary.get_or_err("Function")?, objects)?;
    let color_stops = ColorStops::from_function(&function, &color_space)?;

    Ok(Shading::Axial {
        color_space,
        coords,
        color_stops,
    })
}

/// Parses a Type 3 radial shading dictionary.
fn parse_radial(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let coords = dictionary.required_array_of::<f32, 6>("Coords", objects)?;
    let color_space = required_color_space(dictionary, objects)?;
    let bbox = dictionary.optional_bbox(objects)?;
    let function = Function::parse(dictionary.get_or_err("Function")?, objects)?;
    let color_stops = ColorStops::from_function(&function, &color_space)?;

    Ok(Shading::Radial {
        color_space,
        coords,
        color_stops,
        bbox,
    })
}

/// Reads the required color space shared by shading types 2 through 7.
pub(crate) fn required_color_space(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<ColorSpace, PdfShadingError> {
    ColorSpace::from_dictionary(dictionary, objects)?.ok_or(PdfShadingError::MissingRequiredEntry {
        entry: "ColorSpace",
    })
}
