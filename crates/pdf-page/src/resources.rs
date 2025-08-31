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
    xobject::{XObject, XObjectError, XObjectReader},
};

pub struct Resources {
    pub fonts: HashMap<String, Font>,
    pub external_graphics_states: HashMap<String, ExternalGraphicsState>,
    pub xobjects: HashMap<String, XObject>,
    pub patterns: HashMap<String, Pattern>,
}

/// Defines errors that can occur while reading Resources object.
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
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
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
        let resources = objects.resolve_dictionary(resources)?;

        let mut fonts = HashMap::new();

        // Process `/Font` entries.
        if let Some(font_dictionary) = resources
            .get(Font::KEY)
            .map(|d| d.try_dictionary())
            .transpose()?
        {
            for (name, v) in &font_dictionary.dictionary {
                // Each font value should be a dictionary or reference to one.
                let font_dict = objects.resolve_dictionary(v)?;

                // Parse the font and insert it into the fonts map.
                fonts.insert(name.to_owned(), Font::from_dictionary(font_dict, objects)?);
            }
        }

        let mut external_graphics_states = HashMap::new();

        // Process `/ExtGState` entries
        if let Some(eg) = resources
            .get("ExtGState")
            .map(|d| d.try_dictionary())
            .transpose()?
        {
            for (name, v) in &eg.dictionary {
                // Each value can be a direct dictionary or an indirect reference to one.
                let dictionary = objects.resolve_dictionary(v)?;
                // Parse the external graphics state and insert it into the map.
                external_graphics_states.insert(
                    name.to_owned(),
                    ExternalGraphicsState::from_dictionary(dictionary, objects)?,
                );
            }
        }
        let mut patterns = HashMap::new();

        // Process `/Pattern` entries
        if let Some(eg) = resources
            .get("Pattern")
            .map(|d| d.try_dictionary())
            .transpose()?
        {
            for (name, v) in &eg.dictionary {
                // Parse the pattern and insert it into the map.
                match objects.resolve_object(v)? {
                    ObjectVariant::Dictionary(dictionary) => {
                        patterns.insert(
                            name.to_owned(),
                            Pattern::from_dictionary(dictionary, objects, None)?,
                        );
                    }
                    ObjectVariant::Stream(stream) => {
                        patterns.insert(
                            name.to_owned(),
                            Pattern::from_dictionary(
                                &stream.dictionary,
                                objects,
                                Some(&stream.data),
                            )?,
                        );
                    }
                    obj => {
                        return Err(ResourcesError::InvalidEntryType {
                            entry_name: "Pattern",
                            expected_type: "Dictionary or Stream",
                            found_type: obj.name(),
                        });
                    }
                }
            }
        }

        let mut xobjects = HashMap::new();

        // Process `/XObject` entries
        if let Some(xobject_dict) = resources
            .get("XObject")
            .map(|d| d.try_dictionary())
            .transpose()?
        {
            for (name, v) in &xobject_dict.dictionary {
                let StreamObject {
                    dictionary, data, ..
                } = objects.resolve_stream(v)?;

                // Parse the XObject and insert it into the map.
                xobjects.insert(
                    name.to_owned(),
                    XObject::read_xobject(dictionary, data.as_slice(), objects)?,
                );
            }
        }

        Ok(Some(Self {
            fonts,
            external_graphics_states,
            xobjects,
            patterns,
        }))
    }
}
