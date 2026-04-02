use std::borrow::Cow;

use num_traits::FromPrimitive;

use crate::cross_reference_table::CrossReferenceTable;
use crate::dictionary::Dictionary;
use crate::error::ObjectError;
use crate::indirect_object::IndirectObject;
use crate::object_resolver::ObjectResolver;
use crate::stream::StreamObject;
use crate::trailer::Trailer;

/// Represents any PDF object as described in the PDF specification.
///
/// This enum is the central value type used across the crate to model
/// dictionaries, arrays, numbers, strings, streams, and other PDF constructs.
#[derive(Debug, PartialEq, Clone)]
pub enum ObjectVariant {
    /// A PDF dictionary object.
    Dictionary(Box<Dictionary>),
    /// A PDF array of objects.
    Array(Vec<ObjectVariant>),
    /// A literal string (enclosed in parentheses in PDF syntax), stored as raw bytes.
    LiteralString(Vec<u8>),
    /// A name object (prefixed with a slash in PDF syntax), stored as raw bytes.
    Name(Vec<u8>),
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
    /// The trailer dictionary object.
    Trailer(Trailer),
    /// The cross-reference table object.
    CrossReferenceTable(CrossReferenceTable),
    /// End-of-file marker.
    EndOfFile,
    /// An indirect object with its object number and generation.
    IndirectObject(Box<IndirectObject>),
    /// An indirect reference pointing to an object number.
    Reference(usize),
    /// A stream object, which may have associated dictionary and data.
    Stream(StreamObject),
}

impl ObjectVariant {
    /// Resolves an `ObjectVariant` into a `Dictionary`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `Dictionary`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `Dictionary` or `Err` if the object is not a dictionary or if a reference cannot be
    /// resolved.
    pub fn try_dictionary<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a Dictionary, ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Dictionary(dict) => Ok(dict.as_ref()),
            ObjectVariant::Stream(stream) => Ok(stream.dictionary.as_ref()),
            _ => Err(ObjectError::TypeMismatch("Dictionary", object.name())),
        }
    }

    /// Resolves an `ObjectVariant` into a `StreamObject`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `StreamObject`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `StreamObject` or `Err` if the object is not a stream or if a reference cannot be
    /// resolved.
    pub fn try_stream<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a StreamObject, ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Stream(s) => Ok(s),
            _ => Err(ObjectError::TypeMismatch("Stream", object.name())),
        }
    }

    /// Resolves an `ObjectVariant` into an `Array`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into an `Array`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `Array` or `Err` if the object is not an array or if a reference cannot be
    /// resolved.
    pub fn try_array<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [ObjectVariant], ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Array(arr) => Ok(arr.as_slice()),
            _ => Err(ObjectError::TypeMismatch("Array", object.name())),
        }
    }

    /// Resolves an `ObjectVariant` into a `String`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `String`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `String` or `Err` if the object is not a string or if a reference cannot be
    /// resolved.
    pub fn try_str<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Cow<'a, str>, ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::HexString(s) => {
                let s = String::from_utf8_lossy(s);
                Ok(s)
            }
            ObjectVariant::LiteralString(s) | ObjectVariant::Name(s) => {
                Ok(String::from_utf8_lossy(s))
            }
            _ => Err(ObjectError::TypeMismatch("String", object.name())),
        }
    }

    /// Resolves an `ObjectVariant` into a `Vec<T>` of numeric values.
    ///
    /// This function attempts to convert an array object into a dynamically-sized
    /// vector where each element is parsed as a numeric value.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each element to. Must implement `FromPrimitive`,
    ///   `Copy`, and `Default`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `Vec<T>` or `Err` if:
    /// - The object is not an array.
    /// - Any element cannot be converted to type `T`.
    pub fn try_vec_of<T>(&self, objects: &dyn ObjectResolver) -> Result<Vec<T>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        let values = self.try_array(objects)?;

        let mut result: Vec<T> = Vec::new();
        for v in values.iter() {
            result.push(v.try_number(objects)?);
        }

        Ok(result)
    }

    /// Resolves an `ObjectVariant` into a numeric type `T`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a numeric type `T`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `T` or `Err` if the object is not a number or if a reference cannot be
    /// resolved.
    pub fn try_number<T>(&self, objects: &dyn ObjectResolver) -> Result<T, ObjectError>
    where
        T: FromPrimitive,
    {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Integer(value) => {
                T::from_i64(*value).ok_or(ObjectError::NumberConversionError)
            }
            ObjectVariant::Real(value) => {
                T::from_f64(*value).ok_or(ObjectError::NumberConversionError)
            }
            _ => Err(ObjectError::TypeMismatch("Number", object.name())),
        }
    }

    /// Returns `true` if this value is a Name object.
    pub fn is_name(&self) -> bool {
        matches!(self, ObjectVariant::Name(_))
    }

    /// Returns `true` if this value is an `Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, ObjectVariant::Array(_))
    }

    /// Resolves an `ObjectVariant` into a fixed-size array of numeric values.
    ///
    /// This function attempts to convert an array object into a Rust array of type `[T; N]`,
    /// where each element is parsed as a numeric value.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each element to. Must implement `FromPrimitive`,
    ///   `Copy`, and `Default`.
    /// - `N`: The expected length of the array (compile-time constant).
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `[T; N]` or `Err` if:
    /// - The object is not an array.
    /// - The array length does not match `N`.
    /// - Any element cannot be converted to type `T`.
    pub fn try_array_of<T, const N: usize>(
        &self,
        objects: &dyn ObjectResolver,
    ) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        let values = self.try_array(objects)?;

        if values.len() != N {
            return Err(ObjectError::InvalidArrayLength {
                expected: N,
                found: values.len(),
            });
        }

        let mut result = [T::default(); N];
        result
            .iter_mut()
            .zip(values.iter())
            .try_for_each(|(out, v)| {
                *out = v.try_number(objects)?;
                Ok::<(), ObjectError>(())
            })?;

        Ok(result)
    }

    /// Resolves an `ObjectVariant` into raw bytes.
    ///
    /// This function attempts to extract the underlying byte representation from
    /// string-like objects (`HexString`, `LiteralString`, or `Name`).
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `&[u8]` or `Err` if the object is not a string-like type or if a reference
    /// cannot be resolved.
    pub fn try_bytes<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::HexString(s) => Ok(s),
            ObjectVariant::Name(s) | ObjectVariant::LiteralString(s) => Ok(s),
            _ => Err(ObjectError::TypeMismatch("HexString", object.name())),
        }
    }

    /// Resolves an `ObjectVariant` into a boolean value.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `bool`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `bool` or `Err` if the object is not a boolean or if a reference cannot be
    /// resolved.
    pub fn try_boolean(&self, objects: &dyn ObjectResolver) -> Result<bool, ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Boolean(value) => Ok(*value),
            _ => Err(ObjectError::TypeMismatch("Boolean", object.name())),
        }
    }

    /// Returns the object number if this is a `Reference`.
    pub fn try_object_number(&self) -> Result<usize, ObjectError> {
        match self {
            ObjectVariant::Reference(value) => Ok(*value),
            ObjectVariant::Dictionary(value) => Ok(value.object_number),
            ObjectVariant::Stream(value) => Ok(value.object_number),
            _ => Err(ObjectError::TypeMismatch(
                "Reference or Object with number",
                self.name(),
            )),
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
            ObjectVariant::Trailer(_) => "Trailer",
            ObjectVariant::CrossReferenceTable(_) => "CrossReferenceTable",
            ObjectVariant::EndOfFile => "EndOfFile",
            ObjectVariant::Reference(_) => "Reference",
        }
    }
}
