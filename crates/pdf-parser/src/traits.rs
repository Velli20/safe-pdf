use pdf_object::{
    ObjectVariant, cross_reference_table::CrossReferenceTable, dictionary::Dictionary,
    trailer::Trailer, version::Version,
};

pub trait ArrayParser {
    type ErrorType;

    fn parse_array(&mut self) -> Result<Vec<ObjectVariant>, Self::ErrorType>;
}

pub trait StreamParser {
    type ErrorType;

    fn parse_stream(&mut self, dictionary: &Dictionary) -> Result<Vec<u8>, Self::ErrorType>;
}

pub trait BooleanParser {
    type ErrorType;

    fn parse_boolean(&mut self) -> Result<bool, Self::ErrorType>;
}

pub trait CommentParser {
    type ErrorType;

    fn parse_comment(&mut self) -> Result<String, Self::ErrorType>;
}

pub trait CrossReferenceTableParser {
    type ErrorType;

    fn parse_cross_reference_table(&mut self) -> Result<CrossReferenceTable, Self::ErrorType>;
}

pub trait DictionaryParser {
    type ErrorType;

    fn parse_dictionary(&mut self) -> Result<Dictionary, Self::ErrorType>;
}

pub trait HeaderParser {
    type ErrorType;

    fn parse_header(&mut self) -> Result<Version, Self::ErrorType>;
}

pub trait HexStringParser {
    type ErrorType;

    fn parse_hex_string(&mut self) -> Result<String, Self::ErrorType>;
}

pub trait IndirectObjectParser {
    type ErrorType;

    fn parse_indirect_object(&mut self) -> Result<ObjectVariant, Self::ErrorType>;
}

pub trait LiteralStringParser {
    type ErrorType;

    fn parse_literal_string(&mut self) -> Result<String, Self::ErrorType>;
}

pub trait NameParser {
    type ErrorType;

    fn parse_name(&mut self) -> Result<String, Self::ErrorType>;
}

pub trait NullObjectParser {
    type ErrorType;

    fn parse_null_object(&mut self) -> Result<(), Self::ErrorType>;
}

pub trait NumberParser {
    type ErrorType;

    fn parse_number(&mut self) -> Result<ObjectVariant, Self::ErrorType>;
}
pub trait TrailerParser {
    type ErrorType;

    fn parse_trailer(&mut self) -> Result<Trailer, Self::ErrorType>;
}
