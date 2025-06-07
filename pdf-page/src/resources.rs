use std::collections::HashMap;

use pdf_font::font::Font;
use pdf_object::{
    ObjectVariant, Value, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::{
    error::PageError,
    external_graphics_state::{self, ExternalGraphicsState},
};

pub struct Resources {
    pub fonts: HashMap<String, Font>,
    pub external_graphics_states: HashMap<String, ExternalGraphicsState>,
}

impl FromDictionary for Resources {
    const KEY: &'static str = "Resources";
    type ResultType = Self;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let resources = dictionary.get_dictionary(Self::KEY).unwrap();

        let mut fonts = HashMap::new();

        if let Some(font_dictionary) = resources.get_dictionary(Font::KEY) {
            for (name, v) in &font_dictionary.dictionary {
                let font_object = v.as_object().unwrap();
                let font_object = objects.get2(font_object).unwrap();

                if let ObjectVariant::IndirectObject(f0_obj) = &font_object {
                    if let Some(Value::Dictionary(f0_dict)) = &f0_obj.object {
                        fonts.insert(name.to_owned(), Font::from_dictionary(f0_dict, objects)?);
                    }
                }
            }
        }

        let mut external_graphics_states = HashMap::new();

        if let Some(eg) = resources.get_dictionary("ExtGState") {
            for (name, v) in &eg.dictionary {
                let v = if let Value::IndirectObject(ObjectVariant::Reference(number)) = v.as_ref()
                {
                    objects
                        .get_dictionary(*number)
                        .ok_or(PageError::ObjectError("Object not found".to_string()))?
                } else if let Value::Dictionary(obj) = v.as_ref() {
                    obj
                } else {
                    panic!();
                };
                let eg = ExternalGraphicsState::from_dictionary(v, objects)?;
                external_graphics_states.insert(name.to_owned(), eg);
            }
        }

        Ok(Self {
            fonts,
            external_graphics_states,
        })
    }
}
