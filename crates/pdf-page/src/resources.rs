use std::collections::HashMap;

use pdf_font::font::{Font, FontError};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateError};

pub struct Resources {
    pub fonts: HashMap<String, Font>,
    pub external_graphics_states: HashMap<String, ExternalGraphicsState>,
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

    #[error("Failed to resolve font object reference {obj_num}")]
    FailedResolveFontObjectReference { obj_num: i32 },

    #[error("Failed to resolve external graphics state object reference {obj_num}")]
    FailedResolveExternalGraphicsStateObjectReference { obj_num: i32 },

    #[error("Error processing font: {0}")]
    FontError(#[from] FontError),

    #[error("External Graphics State parsing error: {0}")]
    ExternalGraphicsStateError(#[from] ExternalGraphicsStateError),
}

impl FromDictionary for Resources {
    const KEY: &'static str = "Resources";
    type ResultType = Option<Self>;
    type ErrorType = ResourcesError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(resources) = dictionary.get_dictionary(Self::KEY) else {
            return Ok(None);
        };

        let mut fonts = HashMap::new();

        // Process `/Font` entries.
        if let Some(font_dictionary) = resources.get_dictionary(Font::KEY) {
            for (name, v) in &font_dictionary.dictionary {
                // According to PDF spec, font resource entries must be indirect references.
                let font_obj_num = match v.as_ref() {
                    ObjectVariant::Reference(num) => *num,
                    other => {
                        return Err(ResourcesError::UnexpectedFontEntryType {
                            font_name: name.clone(),
                            expected_type: "IndirectObjectReference",
                            found_type: other.name(),
                        });
                    }
                };

                let font_dict = objects.get_dictionary(font_obj_num).ok_or_else(|| {
                    ResourcesError::FailedResolveFontObjectReference {
                        obj_num: font_obj_num,
                    }
                })?;

                fonts.insert(name.to_owned(), Font::from_dictionary(font_dict, objects)?);
            }
        }

        let mut external_graphics_states = HashMap::new();

        // Process `/ExtGState` entries
        if let Some(eg) = resources.get_dictionary("ExtGState") {
            for (name, v) in &eg.dictionary {
                // Value can be a direct dictionary or an indirect reference to one.
                let v = match v.as_ref() {
                    ObjectVariant::Reference(number) => {
                        objects.get_dictionary(*number).ok_or_else(|| {
                            ResourcesError::FailedResolveExternalGraphicsStateObjectReference {
                                obj_num: *number,
                            }
                        })?
                    }
                    ObjectVariant::Dictionary(obj) => obj,
                    other => {
                        return Err(ResourcesError::UnexpectedExtGStateEntryType {
                            entry_name: name.clone(),
                            expected_type: "Dictionary or IndirectObjectReference",
                            found_type: other.name(),
                        });
                    }
                };

                external_graphics_states.insert(
                    name.to_owned(),
                    ExternalGraphicsState::from_dictionary(v, objects)?,
                );
            }
        }

        Ok(Some(Self {
            fonts,
            external_graphics_states,
        }))
    }
}
