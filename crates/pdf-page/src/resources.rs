//! PDF Resources dictionary parsing and management.
//!
//! This module handles the `/Resources` dictionary found in PDF pages and other
//! content streams, providing access to fonts, graphics states, XObjects, patterns,
//! and shadings.

use std::collections::HashMap;

use pdf_font::font::{Font, FontError};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateError},
    pattern::{Pattern, PatternError},
    shading::{Shading, ShadingError},
    xobject::{XObject, XObjectError, XObjectReader},
};

/// Contains all resources referenced by a PDF content stream.
///
/// The `Resources` struct holds collections of various PDF objects that can be
/// referenced by name within content streams, including fonts, graphics states,
/// XObjects (images/forms), patterns, and shadings.
///
/// # PDF Reference
/// See PDF 32000-1:2008 Section 7.8.3 "Resource Dictionaries"
#[derive(Default)]
pub struct Resources {
    /// Named font resources (key: `/Font`)
    pub fonts: HashMap<String, Font>,
    /// Named external graphics state resources (key: `/ExtGState`)
    pub external_graphics_states: HashMap<String, ExternalGraphicsState>,
    /// Named XObject resources such as images and forms (key: `/XObject`)
    pub xobjects: HashMap<String, XObject>,
    /// Named pattern resources (key: `/Pattern`)
    pub patterns: HashMap<String, Pattern>,
    /// Named shading resources (key: `/Shading`)
    pub shadings: HashMap<String, Shading>,
}

/// Errors that can occur while parsing a PDF Resources dictionary.
#[derive(Debug, Error)]
pub enum ResourcesError {
    /// Error occurred while parsing a font resource.
    #[error("Error processing font: {0}")]
    FontError(#[from] FontError),
    /// Error occurred while parsing an external graphics state.
    #[error("External Graphics State parsing error: {0}")]
    ExternalGraphicsStateError(#[from] ExternalGraphicsStateError),
    /// Error occurred while parsing an XObject.
    #[error("XObject parsing error: {0}")]
    XObjectError(#[from] XObjectError),
    /// Error occurred while parsing a pattern.
    #[error("Pattern parsing error: {0}")]
    PatternError(#[from] PatternError),
    /// General PDF object error.
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    /// Error occurred while parsing a shading.
    #[error("Shading parsing error: {0}")]
    ShadingError(#[from] ShadingError),
    /// A resource entry had an unexpected type.
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
}

impl Resources {
    /// Attempts to retrieve a sub-dictionary from the resources dictionary.
    ///
    /// Returns `Ok(None)` if the key doesn't exist, `Ok(Some(dict))` if found,
    /// or an error if the value exists but isn't a valid dictionary.
    fn get_sub_dictionary<'a>(
        resources: &'a Dictionary,
        key: &str,
        objects: &'a ObjectCollection,
    ) -> Result<Option<&'a Dictionary>, ResourcesError> {
        resources
            .get(key)
            .map(|entry| entry.try_dictionary(objects))
            .transpose()
            .map_err(Into::into)
    }

    /// Parses all font resources from the `/Font` sub-dictionary.
    fn parse_fonts(
        resources: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<HashMap<String, Font>, ResourcesError> {
        let Some(font_dict) = Self::get_sub_dictionary(resources, Font::KEY, objects)? else {
            return Ok(HashMap::new());
        };

        font_dict
            .dictionary
            .iter()
            .map(|(name, value)| {
                let dict = objects.resolve_dictionary(value)?;
                let font = Font::from_dictionary(dict, objects)?;
                Ok((name.clone(), font))
            })
            .collect()
    }

    /// Parses all external graphics state resources from the `/ExtGState` sub-dictionary.
    fn parse_external_graphics_states(
        resources: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<HashMap<String, ExternalGraphicsState>, ResourcesError> {
        let Some(ext_gstate_dict) = Self::get_sub_dictionary(resources, "ExtGState", objects)?
        else {
            return Ok(HashMap::new());
        };

        ext_gstate_dict
            .dictionary
            .iter()
            .map(|(name, value)| {
                let dict = objects.resolve_dictionary(value)?;
                let state = ExternalGraphicsState::from_dictionary(dict, objects)?;
                Ok((name.clone(), state))
            })
            .collect()
    }

    /// Parses all pattern resources from the `/Pattern` sub-dictionary.
    ///
    /// Patterns can be either dictionaries (Type 2 shading patterns) or
    /// streams (Type 1 tiling patterns), so both cases are handled.
    fn parse_patterns(
        resources: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<HashMap<String, Pattern>, ResourcesError> {
        let Some(pattern_dict) = Self::get_sub_dictionary(resources, "Pattern", objects)? else {
            return Ok(HashMap::new());
        };

        pattern_dict
            .dictionary
            .iter()
            .map(|(name, value)| {
                let pattern = match objects.resolve_object(value)? {
                    ObjectVariant::Dictionary(dict) => {
                        Pattern::from_dictionary(dict, objects, None)?
                    }
                    ObjectVariant::Stream(stream) => {
                        Pattern::from_dictionary(&stream.dictionary, objects, Some(&stream.data))?
                    }
                    other => {
                        return Err(ResourcesError::InvalidEntryType {
                            entry_name: "Pattern",
                            expected_type: "Dictionary or Stream",
                            found_type: other.name(),
                        });
                    }
                };
                Ok((name.clone(), pattern))
            })
            .collect()
    }

    /// Parses all XObject resources from the `/XObject` sub-dictionary.
    ///
    /// XObjects are always streams containing either images or form content.
    fn parse_xobjects(
        resources: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<HashMap<String, XObject>, ResourcesError> {
        let Some(xobject_dict) = Self::get_sub_dictionary(resources, "XObject", objects)? else {
            return Ok(HashMap::new());
        };

        xobject_dict
            .dictionary
            .iter()
            .map(|(name, value)| {
                let StreamObject {
                    dictionary, data, ..
                } = objects.resolve_stream(value)?;
                let xobject = XObject::read_xobject(dictionary, data.as_slice(), objects)?;
                Ok((name.clone(), xobject))
            })
            .collect()
    }

    /// Parses all shading resources from the `/Shading` sub-dictionary.
    fn parse_shadings(
        resources: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<HashMap<String, Shading>, ResourcesError> {
        let Some(shading_dict) = Self::get_sub_dictionary(resources, "Shading", objects)? else {
            return Ok(HashMap::new());
        };

        shading_dict
            .dictionary
            .iter()
            .map(|(name, value)| {
                let dict = objects.resolve_object(value)?;
                let shading = Shading::from_dictionary(dict, objects)?;
                Ok((name.clone(), shading))
            })
            .collect()
    }
}

impl FromDictionary for Resources {
    const KEY: &'static str = "Resources";
    type ResultType = Option<Self>;
    type ErrorType = ResourcesError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(resources_entry) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        // Resolve the `/Resources` dictionary (may be a direct dict or indirect reference).
        let resources = objects.resolve_dictionary(resources_entry)?;

        // Parse each resource category independently.
        // Using separate methods improves readability and allows for easier
        // error tracking when debugging resource loading issues.
        let fonts = Self::parse_fonts(resources, objects)?;
        let external_graphics_states = Self::parse_external_graphics_states(resources, objects)?;
        let patterns = Self::parse_patterns(resources, objects)?;
        let xobjects = Self::parse_xobjects(resources, objects)?;
        let shadings = Self::parse_shadings(resources, objects)?;

        Ok(Some(Self {
            fonts,
            external_graphics_states,
            xobjects,
            patterns,
            shadings,
        }))
    }
}
