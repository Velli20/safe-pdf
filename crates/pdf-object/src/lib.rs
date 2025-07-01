pub mod cross_reference_table;
pub mod dictionary;
pub mod error;
pub mod indirect_object;
pub mod object_collection;
pub mod stream;
pub mod trailer;
pub mod traits;
pub mod version;

use std::{borrow::Cow, rc::Rc};

use cross_reference_table::CrossReferenceTable;
use dictionary::Dictionary;
use error::ObjectError;
use indirect_object::IndirectObject;
use stream::StreamObject;
use trailer::Trailer;

use num_traits::FromPrimitive;

#[derive(Debug, PartialEq, Clone)]
pub enum ObjectVariant {
    Dictionary(Rc<Dictionary>),
    Array(Vec<ObjectVariant>),
    LiteralString(String),
    Name(String),
    Integer(i64),
    Real(f64),
    Boolean(bool),
    Null,
    HexString(Vec<u8>),
    Comment(String),
    Trailer(Trailer),
    CrossReferenceTable(CrossReferenceTable),
    EndOfFile,
    IndirectObject(Rc<IndirectObject>),
    Reference(i32),
    Stream(Rc<StreamObject>),
}

impl ObjectVariant {
    pub fn as_object_number(&self) -> Option<i32> {
        match self {
            ObjectVariant::IndirectObject(o) => Some(o.object_number),
            ObjectVariant::Reference(o) => Some(*o),
            _ => None,
        }
    }

    pub fn to_object_number(&self) -> Option<i32> {
        match self {
            ObjectVariant::IndirectObject(o) => Some(o.object_number),
            ObjectVariant::Reference(o) => Some(*o),
            ObjectVariant::Stream(o) => Some(o.object_number),
            _ => None,
        }
    }

    pub fn as_dictionary(&self) -> Option<&Rc<Dictionary>> {
        match self {
            ObjectVariant::Dictionary(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[ObjectVariant]> {
        match self {
            ObjectVariant::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<i32> {
        match self {
            ObjectVariant::Reference(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_str<'a>(&'a self) -> Option<Cow<'a, str>> {
        match self {
            ObjectVariant::HexString(s) => {
                let s = String::from_utf8_lossy(s);
                Some(s)
            }
            ObjectVariant::LiteralString(s) | ObjectVariant::Name(s) => Some(Cow::Borrowed(&s)),
            _ => None,
        }
    }

    pub fn as_bytes<'a>(&'a self) -> Option<&'a [u8]> {
        match self {
            ObjectVariant::HexString(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            ObjectVariant::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Attempts to convert this `Value` into a numeric type `T`.
    ///
    /// This function checks if the `Value` is a `Value::Number`.
    /// If it is, it attempts to convert the inner integer or float
    /// value into the requested type `T` using the `FromPrimitive` trait.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The target numeric type. Must implement `num_traits::FromPrimitive`.
    ///
    /// # Returns
    ///
    /// Value converted to `T` or an error if the conversion from the internal value to `T` fails.
    pub fn as_number<T>(&self) -> Result<T, ObjectError>
    where
        T: FromPrimitive,
    {
        if let ObjectVariant::Integer(value) = self {
            T::from_i64(*value).ok_or(ObjectError::NumberConversionError)
        } else if let ObjectVariant::Real(value) = self {
            T::from_f64(*value).ok_or(ObjectError::NumberConversionError)
        } else {
            Err(ObjectError::TypeMismatch("Number", self.name()))
        }
    }

    /// Returns a string representation of the `Value` variant's type.
    /// This is useful for creating descriptive error messages.
    pub const fn name(&self) -> &'static str {
        match self {
            ObjectVariant::IndirectObject(_) => "IndirectObject",
            ObjectVariant::Dictionary(_) => "Dictionary",
            ObjectVariant::Array(_) => "Array",
            ObjectVariant::LiteralString(_) => "LiteralString",
            ObjectVariant::Name(_) => "Name",
            ObjectVariant::Integer(_) => "Integer",
            ObjectVariant::Real(_) => "Real",
            ObjectVariant::Boolean(_) => "Boolean",
            ObjectVariant::Null => "Null",
            ObjectVariant::Stream(_) => "Stream",
            ObjectVariant::HexString(_) => "HexString",
            ObjectVariant::Comment(_) => "Comment",
            ObjectVariant::Trailer(_) => "Trailer",
            ObjectVariant::CrossReferenceTable(_) => "CrossReferenceTable",
            ObjectVariant::EndOfFile => "EndOfFile",
            ObjectVariant::Reference(_) => "Reference",
        }
    }
}
