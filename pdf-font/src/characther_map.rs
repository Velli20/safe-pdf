use pdf_object::{stream::StreamObject, traits::FromStreamObject};

use crate::error::FontError;

pub struct CharacterMap {}

impl FromStreamObject for CharacterMap {
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_stream_object(stream: &StreamObject) -> Result<Self::ResultType, Self::ErrorType> {
        let map = String::from_utf8_lossy(&stream.data);
        println!("map {:?}", map);
        Ok(Self {})
    }
}
