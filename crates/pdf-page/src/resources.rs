use std::collections::HashMap;

use pdf_font::font::{Font, FontError};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
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
    #[error(
        "Unexpected object type in `/Fonts` dictionary: expected 'Object' or 'ObjectReference', found '{found_type}'"
    )]
    UnexpectedObjectTypeInFonts { found_type: &'static str },

    #[error(
        "Unexpected object type in `/Fonts` dictionary: expected 'Object' or 'ObjectReference', found '{found_type}'"
    )]
    UnexpectedFontEntryType {
        font_name: String,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error(
        "Unexpected type for ExtGState entry '{entry_name}': expected {expected_type}, found {found_type}"
    )]
    UnexpectedExtGStateEntryType {
        entry_name: String,
        expected_type: &'static str,
        found_type: &'static str,
    },

    #[error("Failed to resolve /Resources dictionary object reference {obj_num}")]
    FailedResolveResourcesObjectReference { obj_num: i32 },

    #[error("Failed to resolve font object reference {obj_num}")]
    FailedResolveFontObjectReference { obj_num: i32 },

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

        let resources = objects
            .get_dictionary(resources.as_ref())
            .ok_or_else(|| ResourcesError::FailedResolveResourcesObjectReference { obj_num: 0 })?;

        let mut fonts = HashMap::new();

        // Process `/Font` entries.
        if let Some(font_dictionary) = resources.get_dictionary(Font::KEY) {
            for (name, v) in &font_dictionary.dictionary {
                let font_dict = objects.get_dictionary(v).ok_or_else(|| {
                    ResourcesError::FailedResolveFontObjectReference { obj_num: 0 }
                })?;

                fonts.insert(
                    name.to_owned(),
                    Font::from_dictionary(font_dict.as_ref(), objects)?,
                );
            }
        }

        let mut external_graphics_states = HashMap::new();

        // Process `/ExtGState` entries
        if let Some(eg) = resources.get_dictionary("ExtGState") {
            for (name, v) in &eg.dictionary {
                // Value can be a direct dictionary or an indirect reference to one.
                let v = objects.get_dictionary(v.as_ref()).ok_or_else(|| {
                    ResourcesError::FailedResolveExternalGraphicsStateObjectReference { obj_num: 0 }
                })?;

                external_graphics_states.insert(
                    name.to_owned(),
                    ExternalGraphicsState::from_dictionary(v.as_ref(), objects)?,
                );
            }
        }

        let mut xobjects = HashMap::new();

        // Process `/XObject` entries
        if let Some(xobject_dict) = resources.get_dictionary("XObject") {
            for (name, v) in &xobject_dict.dictionary {
                let (obj_dict, stream_data) = match v.as_ref() {
                    ObjectVariant::Reference(number) => {
                        let resolved_obj = objects.get(*number).ok_or_else(|| {
                            ResourcesError::FailedResolveResourcesObjectReference {
                                obj_num: *number,
                            }
                        })?;
                        match resolved_obj {
                            ObjectVariant::Stream(s) => (s.dictionary.clone(), s.data.clone()),
                            _ => {
                                return Err(ResourcesError::UnexpectedObjectTypeInFonts {
                                    found_type: resolved_obj.name(),
                                });
                            }
                        }
                    }
                    ObjectVariant::Stream(s) => (s.dictionary.clone(), s.data.clone()),
                    _ => {
                        return Err(ResourcesError::UnexpectedObjectTypeInFonts {
                            found_type: v.name(),
                        });
                    }
                };

                xobjects.insert(
                    name.to_owned(),
                    XObject::read_xobject(&obj_dict, stream_data.as_slice(), objects)?,
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
