pub mod array;
pub mod boolean;
pub mod comment;
pub mod cross_reference_table;
pub mod dictionary;
pub mod error;
pub mod hex_string;
pub mod indirect_object;
pub mod literal_string;
pub mod name;
pub mod null;
pub mod number;
pub mod object_collection;
pub mod stream;
pub mod trailer;
pub mod version;

use std::rc::Rc;

use array::Array;
use boolean::Boolean;
use comment::Comment;
use cross_reference_table::CrossReferenceTable;
use dictionary::Dictionary;
use hex_string::HexString;
use indirect_object::IndirectObjectOrReference;
use literal_string::LiteralString;
use name::Name;
use null::NullObject;
use number::Number;
use stream::Stream;
use trailer::Trailer;

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    IndirectObject(Rc<IndirectObjectOrReference>),
    Dictionary(Rc<Dictionary>),
    Array(Array),
    LiteralString(LiteralString),
    Name(Name),
    Number(Number),
    Boolean(Boolean),
    Null(NullObject),
    Stream(Stream),
    HexString(HexString),
    Comment(Comment),
    Trailer(Trailer),
    CrossReferenceTable(CrossReferenceTable),
    EndOfFile,
}

impl Value {
    pub fn as_dictionary(&self) -> Option<&Rc<Dictionary>> {
        if let Value::Dictionary(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&IndirectObjectOrReference> {
        if let Value::IndirectObject(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
