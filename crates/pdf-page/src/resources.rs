//! PDF Resources dictionary parsing and management.
//!
//! This module handles the `/Resources` dictionary found in PDF pages and other
//! content streams, providing access to fonts, graphics states, XObjects, patterns,
//! and shadings.

use std::collections::HashMap;
use std::rc::Rc;

use pdf_font::font::{Font, FontError};
use pdf_object::{dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver};
use thiserror::Error;

use crate::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateError},
    pattern::{Pattern, PatternError},
    resource::Resource,
    resource_cache::ResourceCache,
    shading::{Shading, ShadingError},
    xobject::{XObject, XObjectError},
};

/// Contains all resources referenced by a PDF content stream.
///
/// The `Resources` struct holds a unified collection of PDF objects that can be
/// referenced by name within content streams, including fonts, graphics states,
/// XObjects (images/forms), patterns, and shadings.
#[derive(Default)]
pub struct Resources(HashMap<String, Resource>);

/// Errors that can occur while parsing a PDF Resources dictionary.
#[derive(Debug, Error)]
pub enum ResourcesError {
    #[error("Error processing font: {0}")]
    FontError(#[from] FontError),
    #[error("External Graphics State parsing error: {0}")]
    ExternalGraphicsStateError(#[from] ExternalGraphicsStateError),
    #[error("XObject parsing error: {0}")]
    XObjectError(#[from] XObjectError),
    #[error("Pattern parsing error: {0}")]
    PatternError(#[from] PatternError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Shading parsing error: {0}")]
    ShadingError(#[from] ShadingError),
    #[error("Error parsing content stream: {0}")]
    ContentStreamError(#[from] pdf_content_stream::error::PdfOperatorError),
}

/// Attempts to retrieve a sub-dictionary from the resources dictionary.
///
/// # Parameters
///
/// - `resources`: The main resources dictionary to search within.
/// - `key`: The key of the sub-dictionary to retrieve (e.g., "Font", "Pattern").
/// - `objects`: The object resolver to resolve indirect references if necessary.
///
/// # Returns
///
/// Returns `Ok(None)` if the key doesn't exist, `Ok(Some(dict))` if found,
/// or an error if the value exists but isn't a valid dictionary.
fn get_sub_dictionary<'a>(
    resources: &'a Dictionary,
    key: &str,
    objects: &'a dyn ObjectResolver,
) -> Result<Option<&'a Dictionary>, ResourcesError> {
    resources
        .get(key)
        .map(|entry| entry.try_dictionary(objects))
        .transpose()
        .map_err(Into::into)
}

/// Parses all font resources from the `/Font` sub-dictionary.
fn read_fonts(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, ResourcesError> {
    let Some(font_dict) = get_sub_dictionary(resources, Font::KEY, objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &font_dict.dictionary {
        let dict = value.try_dictionary(objects)?;
        if let Some(cached) = cache.get(&dict.object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        let resource = Resource::Font(Rc::new(Font::from_dictionary(dict, objects)?));
        cache.insert(dict.object_number, resource.clone());
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all external graphics state resources from the `/ExtGState` sub-dictionary.
fn read_external_graphics_states(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, ResourcesError> {
    let Some(ext_gstate_dict) = get_sub_dictionary(resources, "ExtGState", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &ext_gstate_dict.dictionary {
        let dict = value.try_dictionary(objects)?;
        if let Some(cached) = cache.get(&dict.object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        let resource = Resource::ExternalGraphicsState(Rc::new(
            ExternalGraphicsState::from_dictionary(dict, objects, cache)?,
        ));
        cache.insert(dict.object_number, resource.clone());
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all pattern resources from the `/Pattern` sub-dictionary.
fn read_patterns(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, ResourcesError> {
    let Some(pattern_dict) = get_sub_dictionary(resources, "Pattern", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &pattern_dict.dictionary {
        let object_number = value.try_object_number()?;
        if let Some(cached) = cache.get(&object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }
        let pattern = Pattern::read(value, objects, cache)?;
        let resource = Resource::Pattern(Rc::new(pattern));
        cache.insert(object_number, resource.clone());

        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all XObject resources from the `/XObject` sub-dictionary.
fn read_xobjects(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, ResourcesError> {
    let Some(xobject_dict) = get_sub_dictionary(resources, "XObject", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &xobject_dict.dictionary {
        let stream = value.try_stream(objects)?;
        if let Some(cached) = cache.get(&stream.object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        let resource = Resource::XObject(Rc::new(XObject::read_xobject(
            &stream.dictionary,
            stream,
            objects,
            cache,
        )?));
        cache.insert(stream.object_number, resource.clone());
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all shading resources from the `/Shading` sub-dictionary.
fn read_shadings(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, ResourcesError> {
    let Some(shading_dict) = get_sub_dictionary(resources, "Shading", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &shading_dict.dictionary {
        let object_number = value.try_object_number()?;
        if let Some(cached) = cache.get(&object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }
        let resource = Resource::Shading(Rc::new(Shading::from_dictionary(value, objects)?));
        cache.insert(object_number, resource.clone());
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

impl Resources {
    /// Returns a reference to a font resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Font`] if found, or `None` if not present or not a font.
    pub fn font(&self, name: &str) -> Option<&Font> {
        match self.0.get(name)? {
            Resource::Font(font) => Some(font),
            _ => None,
        }
    }

    /// Returns a reference to an external graphics state resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`ExternalGraphicsState`] if found, or `None` if not present or not an external graphics state.
    pub fn external_graphics_state(&self, name: &str) -> Option<&ExternalGraphicsState> {
        match self.0.get(name)? {
            Resource::ExternalGraphicsState(state) => Some(state),
            _ => None,
        }
    }

    /// Returns a reference to an XObject resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`XObject`] if found, or `None` if not present or not an XObject.
    pub fn xobject(&self, name: &str) -> Option<&XObject> {
        match self.0.get(name)? {
            Resource::XObject(xobject) => Some(xobject),
            _ => None,
        }
    }

    /// Returns a reference to a pattern resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Pattern`] if found, or `None` if not present or not a pattern.
    pub fn pattern(&self, name: &str) -> Option<&Pattern> {
        match self.0.get(name)? {
            Resource::Pattern(pattern) => Some(pattern),
            _ => None,
        }
    }

    /// Returns a reference to a shading resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Shading`] if found, or `None` if not present or not a shading.
    pub fn shading(&self, name: &str) -> Option<&Shading> {
        match self.0.get(name)? {
            Resource::Shading(shading) => Some(shading),
            _ => None,
        }
    }

    /// Reads the `/Resources` dictionary.
    ///
    /// This function extracts all resource types (fonts, external graphics states, patterns,
    /// XObjects, and shadings) referenced in the provided `dictionary`.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The PDF dictionary potentially containing a `/Resources` entry.
    /// - `objects`: An object resolver for resolving indirect PDF object references.
    /// - `cache`: A mutable resource cache for storing and retrieving parsed resources.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Resources))` if resources are found and parsed successfully, `Ok(None)`
    /// if no `/Resources` entry exists, or an error if parsing fails for any resource type.
    ///
    /// # Errors
    ///
    /// Returns a [`ResourcesError`] if any resource fails to parse or resolve.
    pub fn read(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Option<Self>, ResourcesError> {
        const KEY: &str = "Resources";

        let Some(resources_entry) = dictionary.get(KEY) else {
            return Ok(None);
        };

        let resources = resources_entry.try_dictionary(objects)?;

        let mut map = HashMap::new();
        map.extend(read_fonts(resources, objects, cache)?);
        map.extend(read_external_graphics_states(resources, objects, cache)?);
        map.extend(read_patterns(resources, objects, cache)?);
        map.extend(read_xobjects(resources, objects, cache)?);
        map.extend(read_shadings(resources, objects, cache)?);

        Ok(Some(Self(map)))
    }
}
