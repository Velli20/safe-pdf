use std::{borrow::Cow, rc::Rc};

use num_traits::FromPrimitive;

use crate::cross_reference_table::CrossReferenceTable;
use crate::dictionary::Dictionary;
use crate::error::ObjectError;
use crate::indirect_object::IndirectObject;
use crate::stream::StreamObject;
use crate::trailer::Trailer;

/// Represents any PDF object as described in the PDF specification.
///
/// This enum is the central value type used across the crate to model
/// dictionaries, arrays, numbers, strings, streams, and other PDF constructs.
#[derive(Debug, PartialEq, Clone)]
pub enum ObjectVariant {
    /// A PDF dictionary object.
    Dictionary(Rc<Dictionary>),
    /// A PDF array of objects.
    Array(Vec<ObjectVariant>),
    /// A literal string (enclosed in parentheses in PDF syntax).
    LiteralString(String),
    /// A name object (prefixed with a slash in PDF syntax).
    Name(String),
    /// An integer number.
    Integer(i64),
    /// A real (floating point) number.
    Real(f64),
    /// A boolean value.
    Boolean(bool),
    /// The null object.
    Null,
    /// A hexadecimal string represented as raw bytes.
    HexString(Vec<u8>),
    /// A comment encountered in the PDF content.
    Comment(String),
    /// The trailer dictionary object.
    Trailer(Trailer),
    /// The cross-reference table object.
    CrossReferenceTable(CrossReferenceTable),
    /// End-of-file marker.
    EndOfFile,
    /// An indirect object with its object number and generation.
    IndirectObject(Rc<IndirectObject>),
    /// An indirect reference pointing to an object number.
    Reference(i32),
    /// A stream object, which may have associated dictionary and data.
    Stream(Rc<StreamObject>),
}

impl ObjectVariant {
    /// Returns the object number if this value represents an indirect object
    /// or a reference; otherwise returns `None`.
    pub fn as_object_number(&self) -> Option<i32> {
        match self {
            ObjectVariant::IndirectObject(o) => Some(o.object_number),
            ObjectVariant::Reference(o) => Some(*o),
            _ => None,
        }
    }

    /// Converts this value into its object number if applicable.
    ///
    /// Unlike [`as_object_number`], this also returns the object number for
    /// stream objects, since streams are always indirect in PDFs.
    pub fn to_object_number(&self) -> Option<i32> {
        match self {
            ObjectVariant::IndirectObject(o) => Some(o.object_number),
            ObjectVariant::Reference(o) => Some(*o),
            ObjectVariant::Stream(o) => Some(o.object_number),
            _ => None,
        }
    }

    /// Returns a borrowed reference to the inner dictionary if this is a
    /// `Dictionary` variant.
    pub fn as_dictionary(&self) -> Option<&Rc<Dictionary>> {
        match self {
            ObjectVariant::Dictionary(value) => Some(value),
            _ => None,
        }
    }

    /// Returns a slice view into the inner array if this is an `Array` variant.
    pub fn as_array(&self) -> Option<&[ObjectVariant]> {
        match self {
            ObjectVariant::Array(value) => Some(value),
            _ => None,
        }
    }

    /// Returns `true` if this value is an `Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, ObjectVariant::Array(_))
    }

    /// Attempts to convert an array into a fixed-size array of numeric values.
    ///
    /// Each element must be a number that can be converted into `T` via
    /// `FromPrimitive`. The input array length must match `N` exactly.
    ///
    /// Errors
    /// - `TypeMismatch` if this is not an array.
    /// - `InvalidArrayLength` if the array length does not equal `N`.
    /// - `NumberConversionError` if any element cannot be converted to `T`.
    pub fn as_array_of<T, const N: usize>(&self) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        let values = self
            .as_array()
            .ok_or_else(|| ObjectError::TypeMismatch("Array", self.name()))?;

        if values.len() != N {
            return Err(ObjectError::InvalidArrayLength {
                expected: N,
                found: values.len(),
            });
        }

        let mut result = [T::default(); N];
        for (i, v) in values.iter().enumerate() {
            result[i] = v.as_number()?;
        }

        Ok(result)
    }

    /// Attempts to convert an array into a `Vec<T>` of numeric values.
    ///
    /// Errors
    /// - `TypeMismatch` if this is not an array.
    /// - `NumberConversionError` if any element cannot be converted to `T`.
    pub fn as_vec_of<T>(&self) -> Result<Vec<T>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        let values = self
            .as_array()
            .ok_or_else(|| ObjectError::TypeMismatch("Array", self.name()))?;

        let mut result: Vec<T> = Vec::new();
        for v in values.iter() {
            result.push(v.as_number()?);
        }

        Ok(result)
    }

    /// Returns the object number if this is a `Reference`, otherwise `None`.
    pub fn as_reference(&self) -> Option<i32> {
        match self {
            ObjectVariant::Reference(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns a string view if this is a string-like type.
    ///
    /// For `HexString`, a lossy UTF-8 conversion is performed and returned.
    /// For `LiteralString` and `Name`, a borrowed string slice is returned.
    pub fn as_str(&self) -> Option<Cow<'_, str>> {
        match self {
            ObjectVariant::HexString(s) => {
                let s = String::from_utf8_lossy(s);
                Some(s)
            }
            ObjectVariant::LiteralString(s) | ObjectVariant::Name(s) => Some(Cow::Borrowed(s)),
            _ => None,
        }
    }

    /// Returns the raw bytes if this is a `HexString`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            ObjectVariant::HexString(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the boolean value if this is a `Boolean`, otherwise `None`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            ObjectVariant::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Attempts to convert this value into a numeric type `T`.
    ///
    /// Accepts `Integer` and `Real` variants and uses `FromPrimitive` to
    /// perform the conversion.
    ///
    /// Errors
    /// - `TypeMismatch` if the value is not a number.
    /// - `NumberConversionError` if conversion into `T` fails.
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

    /// Returns the variant name as a static string, useful in error messages.
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
