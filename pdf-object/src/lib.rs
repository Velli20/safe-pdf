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
pub mod traits;
pub mod version;

use std::rc::Rc;

use array::Array;
use boolean::Boolean;
use comment::Comment;
use cross_reference_table::CrossReferenceTable;
use dictionary::Dictionary;
use error::ObjectError;
use hex_string::HexString;
use indirect_object::IndirectObject;
use literal_string::LiteralString;
use name::Name;
use null::NullObject;
use number::Number;
use stream::StreamObject;
use trailer::Trailer;

use num_traits::FromPrimitive;

#[derive(Debug, PartialEq, Clone)]
pub enum ObjectVariant {
    IndirectObject(Rc<IndirectObject>),
    Reference(i32),
    Stream(Rc<StreamObject>),
}

impl ObjectVariant {
    pub fn object_number(&self) -> i32 {
        match self {
            ObjectVariant::IndirectObject(o) => o.object_number,
            ObjectVariant::Reference(o) => *o,
            ObjectVariant::Stream(o) => o.object_number,
        }
    }

    /// Returns a string representation of the `ObjectVariant`'s type.
    /// This is useful for creating descriptive error messages.
    pub const fn name(&self) -> &'static str {
        match self {
            ObjectVariant::IndirectObject(_) => "IndirectObjectValue", // Represents a fully resolved IndirectObject containing its Value
            ObjectVariant::Reference(_) => "Reference",
            ObjectVariant::Stream(_) => "StreamValue", // Represents a fully resolved StreamObject
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    IndirectObject(ObjectVariant),
    Dictionary(Rc<Dictionary>),
    Array(Array),
    LiteralString(LiteralString),
    Name(Name),
    Number(Number),
    Boolean(Boolean),
    Null(NullObject),
    Stream(StreamObject),
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
    pub fn as_array(&self) -> Option<&Array> {
        if let Value::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&ObjectVariant> {
        if let Value::IndirectObject(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Value::LiteralString(value) = self {
            Some(&value.0)
        } else if let Value::HexString(value) = self {
            Some(&value.0)
        } else if let Value::Name(value) = self {
            Some(&value.0)
        } else {
            None
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        if let Value::Boolean(value) = self {
            Some(value.0)
        } else {
            None
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
        if let Value::Number(value) = self {
            if let Some(i) = value.integer {
                return T::from_i64(i).ok_or(ObjectError::NumberConversionError);
            } else if let Some(f) = value.real {
                return T::from_f64(f).ok_or(ObjectError::NumberConversionError);
            }
        }
        Err(ObjectError::TypeMismatch("Number", self.name()))
    }

    /// Returns a string representation of the `Value` variant's type.
    /// This is useful for creating descriptive error messages.
    pub const fn name(&self) -> &'static str {
        match self {
            Value::IndirectObject(_) => "IndirectObject",
            Value::Dictionary(_) => "Dictionary",
            Value::Array(_) => "Array",
            Value::LiteralString(_) => "LiteralString",
            Value::Name(_) => "Name",
            Value::Number(_) => "Number",
            Value::Boolean(_) => "Boolean",
            Value::Null(_) => "Null",
            Value::Stream(_) => "Stream",
            Value::HexString(_) => "HexString",
            Value::Comment(_) => "Comment",
            Value::Trailer(_) => "Trailer",
            Value::CrossReferenceTable(_) => "CrossReferenceTable",
            Value::EndOfFile => "EndOfFile",
        }
    }
}
