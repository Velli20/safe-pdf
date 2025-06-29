use std::collections::HashMap;

use pdf_font::font::{Font, FontError};
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateError},
    xobject::{XObject, XObjectError, XObjectReader},
};

pub struct Resources {
    pub fonts: HashMap<String, Font>,
    pub external_graphics_states: HashMap<String, ExternalGraphicsState>,
    pub xobjects: HashMap<String, XObject>,
}

/// Defines errors that can occur while reading Resources object.
#[derive(Debug, Error)]
pub enum ResourcesError {
    #[error("Failed to resolve /Resources dictionary object reference")]
    FailedResolveResourcesObjectReference,
    #[error("Failed to resolve font object reference for font '{font_name}' (object {obj_num})")]
    FailedResolveFontObjectReference { font_name: String, obj_num: i32 },
    #[error("Failed to resolve stream object reference ")]
    FailedResolveStreamObjectReference,
    #[error("Failed to resolve external graphics state object reference {obj_num}")]
    FailedResolveExternalGraphicsStateObjectReference { obj_num: i32 },
    #[error("Error processing font: {0}")]
    FontError(#[from] FontError),
    #[error("External Graphics State parsing error: {0}")]
    ExternalGraphicsStateError(#[from] ExternalGraphicsStateError),
    #[error("XObject parsing error: {0}")]
    XObjectError(#[from] XObjectError),
}

impl FromDictionary for Resources {
    const KEY: &'static str = "Resources";
    type ResultType = Option<Self>;
    type ErrorType = ResourcesError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(resources) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        // Resolve the actual `/Resources` dictionary.
        let resources = objects
            .resolve_dictionary(resources.as_ref())
            .ok_or_else(|| ResourcesError::FailedResolveResourcesObjectReference)?;

        let mut fonts = HashMap::new();

        // Process `/Font` entries.
        if let Some(font_dictionary) = resources.get_dictionary(Font::KEY) {
            for (name, v) in &font_dictionary.dictionary {
                // Each font value should be a dictionary or reference to one.
                let font_dict = objects.resolve_dictionary(v).ok_or_else(|| {
                    ResourcesError::FailedResolveFontObjectReference {
                        font_name: name.clone(),
                        obj_num: v.as_object_number().unwrap_or(0),
                    }
                })?;

                // Parse the font and insert it into the fonts map.
                fonts.insert(name.to_owned(), Font::from_dictionary(font_dict, objects)?);
            }
        }

        let mut external_graphics_states = HashMap::new();

        // Process `/ExtGState` entries
        if let Some(eg) = resources.get_dictionary("ExtGState") {
            for (name, v) in &eg.dictionary {
                // Each value can be a direct dictionary or an indirect reference to one.
                let dictionary = objects.resolve_dictionary(v.as_ref()).ok_or_else(|| {
                    ResourcesError::FailedResolveExternalGraphicsStateObjectReference {
                        obj_num: v.as_object_number().unwrap_or(0),
                    }
                })?;
                // Parse the external graphics state and insert it into the map.
                external_graphics_states.insert(
                    name.to_owned(),
                    ExternalGraphicsState::from_dictionary(dictionary, objects)?,
                );
            }
        }

        let mut xobjects = HashMap::new();

        // Process `/XObject` entries
        if let Some(xobject_dict) = resources.get_dictionary("XObject") {
            for (name, v) in &xobject_dict.dictionary {
                let stream_object = objects
                    .resolve_stream(v.as_ref())
                    .ok_or(ResourcesError::FailedResolveStreamObjectReference)?;

                // Parse the XObject and insert it into the map.
                xobjects.insert(
                    name.to_owned(),
                    XObject::read_xobject(
                        &stream_object.dictionary,
                        &stream_object.data.as_slice(),
                        objects,
                    )?,
                );
            }
        }

        Ok(Some(Self {
            fonts,
            external_graphics_states,
            xobjects,
        }))
    }
}
