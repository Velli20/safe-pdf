use num_traits::FromPrimitive;

use crate::cross_reference_table::CrossReferenceTable;
use crate::dictionary::Dictionary;
use crate::error::ObjectError;
use crate::indirect_object::IndirectObject;
use crate::object_id::PdfObjectId;
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
    /// into a direct, stream, or indirect object's dictionary.
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
            ObjectVariant::IndirectObject(indirect) => match indirect.object.as_ref() {
                Some(ObjectVariant::Dictionary(dict)) => Ok(dict.as_ref()),
                _ => Err(ObjectError::TypeMismatch("Dictionary", object.name())),
            },
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

    /// Resolves an `ObjectVariant` into UTF-8 text.
    ///
    /// Resolves an object into the raw bytes of a PDF string object.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// Returns an error if the object is not a string or a reference cannot be resolved.
    pub fn try_string_bytes<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::HexString(s) | ObjectVariant::LiteralString(s) => Ok(s),
            _ => Err(ObjectError::TypeMismatch("String", object.name())),
        }
    }

    /// Resolves an object into the raw bytes of a PDF Name object.
    pub fn try_name<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        match object {
            ObjectVariant::Name(name) => Ok(name),
            _ => Err(ObjectError::TypeMismatch("Name", object.name())),
        }
    }

    /// Creates an owned PDF Name from borrowed bytes.
    ///
    /// Parsed names should be moved directly into [`ObjectVariant::Name`].
    /// This constructor is intended for hand-built objects whose source is a
    /// borrowed byte literal or slice.
    pub fn name_from_bytes(name: &[u8]) -> Self {
        Self::Name(Vec::from(name))
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
    /// PDF string objects (`HexString` or `LiteralString`).
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `&[u8]` or `Err` if the object is not a PDF string or if a reference
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
            ObjectVariant::HexString(s) | ObjectVariant::LiteralString(s) => Ok(s),
            _ => Err(ObjectError::TypeMismatch("Bytes", object.name())),
        }
    }

    pub fn try_bytes_vec<'a>(
        &'a self,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Vec<u8>, ObjectError> {
        self.try_bytes(objects).map(|bytes| bytes.to_vec())
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
            ObjectVariant::Dictionary(value) => {
                value.object_number.ok_or(ObjectError::ObjectMissingNumber {
                    found_type: "Dictionary",
                })
            }
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

    /// Extracts a named object identifier when an object carries one.
    pub fn identifier(&self) -> Option<PdfObjectId> {
        match self {
            ObjectVariant::IndirectObject(indirect) => Some(PdfObjectId {
                number: indirect.object_number,
                generation: indirect.generation_number,
            }),
            ObjectVariant::Stream(stream) => Some(PdfObjectId {
                number: stream.object_number,
                generation: stream.generation_number,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_resolver::PassthroughResolver;

    #[test]
    fn try_dictionary_returns_dictionary_from_indirect_object() {
        let object = ObjectVariant::IndirectObject(Box::new(IndirectObject::new(
            1,
            0,
            Some(ObjectVariant::Dictionary(Box::new(Dictionary::new(
                std::collections::BTreeMap::<Vec<u8>, ObjectVariant>::new(),
            )))),
        )));

        let dictionary = object
            .try_dictionary(&PassthroughResolver)
            .expect("indirect dictionary object should decode as a dictionary");

        assert!(dictionary.dictionary.is_empty());
    }

    #[test]
    fn try_dictionary_rejects_indirect_object_without_dictionary() {
        for inner_object in [None, Some(ObjectVariant::Integer(7))] {
            let object =
                ObjectVariant::IndirectObject(Box::new(IndirectObject::new(1, 0, inner_object)));
            let err = object
                .try_dictionary(&PassthroughResolver)
                .expect_err("indirect object without a dictionary should fail");

            assert_eq!(
                err,
                ObjectError::TypeMismatch("Dictionary", "IndirectObject")
            );
        }
    }

    #[test]
    fn try_string_bytes_returns_type_mismatch_for_non_string() {
        let object = ObjectVariant::Integer(7);
        let err = object
            .try_string_bytes(&PassthroughResolver)
            .expect_err("non-string object should not decode as string");

        assert_eq!(err, ObjectError::TypeMismatch("String", "Integer"));
    }

    #[test]
    fn try_string_bytes_rejects_name_objects() {
        let object = ObjectVariant::Name(vec![0xFF]);
        let err = object
            .try_string_bytes(&PassthroughResolver)
            .expect_err("a PDF Name is not a PDF string");

        assert_eq!(err, ObjectError::TypeMismatch("String", "Name"));
    }

    #[test]
    fn try_string_bytes_preserves_non_utf8_bytes() {
        let object = ObjectVariant::LiteralString(vec![0xFF]);

        assert_eq!(
            object
                .try_string_bytes(&PassthroughResolver)
                .expect("PDF strings may contain arbitrary bytes"),
            [0xFF]
        );
    }

    #[test]
    fn try_name_rejects_string_objects() {
        let object = ObjectVariant::LiteralString(Vec::from(b"Name"));

        assert_eq!(
            object
                .try_name(&PassthroughResolver)
                .expect_err("a PDF string is not a PDF Name"),
            ObjectError::TypeMismatch("Name", "LiteralString")
        );
    }

    #[test]
    fn try_number_returns_type_mismatch_for_non_number() {
        let object = ObjectVariant::Array(vec![]);
        let err = object
            .try_number::<u16>(&PassthroughResolver)
            .expect_err("non-number object should not decode as number");

        assert_eq!(err, ObjectError::TypeMismatch("Number", "Array"));
    }

    #[test]
    fn try_stream_returns_type_mismatch_for_non_stream() {
        let null_object = ObjectVariant::Null;
        let null_err = null_object
            .try_stream(&PassthroughResolver)
            .expect_err("null object should not decode as a stream");
        assert_eq!(null_err, ObjectError::TypeMismatch("Stream", "Null"));

        let dict_object = ObjectVariant::Dictionary(Box::new(crate::dictionary::Dictionary::new(
            std::collections::BTreeMap::<Vec<u8>, ObjectVariant>::new(),
        )));
        let dict_err = dict_object
            .try_stream(&PassthroughResolver)
            .expect_err("dictionary object should not decode as a stream");
        assert_eq!(dict_err, ObjectError::TypeMismatch("Stream", "Dictionary"));
    }

    #[test]
    fn try_bytes_returns_type_mismatch_for_non_bytes() {
        let object = ObjectVariant::Dictionary(Box::new(crate::dictionary::Dictionary::new(
            std::collections::BTreeMap::<Vec<u8>, ObjectVariant>::new(),
        )));
        let err = object
            .try_bytes(&PassthroughResolver)
            .expect_err("dictionary object should not decode as bytes");

        assert_eq!(err, ObjectError::TypeMismatch("Bytes", "Dictionary"));
    }

    #[test]
    fn try_bytes_rejects_name_objects() {
        let object = ObjectVariant::name_from_bytes(b"Name");

        assert_eq!(
            object
                .try_bytes(&PassthroughResolver)
                .expect_err("a PDF Name is not a PDF string"),
            ObjectError::TypeMismatch("Bytes", "Name")
        );
    }
}
