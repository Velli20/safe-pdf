use std::collections::HashMap;

use pdf_font::font::Font;
use pdf_object::{
    ObjectVariant, Value, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::error::PageError;

pub struct Resources {
    pub fonts: HashMap<String, Font>,
}

impl FromDictionary for Resources {
    const KEY: &'static str = "Resources";
    type ResultType = Self;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let resources = dictionary
            .get_dictionary(Self::KEY)
            .ok_or(PageError::MissingResources)?;

        let mut fonts = HashMap::new();

        if let Some(font_dictionary) = resources.get_dictionary(Font::KEY) {
            for (name, v) in &font_dictionary.dictionary {
                let font_object = v.as_object().ok_or(PageError::NotDictionary("Font"))?;
                let font_object = objects
                    .get2(font_object)
                    .ok_or(PageError::MissingResources)?;

                if let ObjectVariant::IndirectObject(f0_obj) = &font_object {
                    if let Some(Value::Dictionary(f0_dict)) = &f0_obj.object {
                        fonts.insert(name.to_owned(), Font::from_dictionary(f0_dict, objects)?);
                    }
                }
            }
        }

        Ok(Self { fonts })
    }
}
