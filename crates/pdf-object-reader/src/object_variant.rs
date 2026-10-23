//! PDF object variants and checked accessors.

use crate::error::ObjectReadError;
use crate::object_kind::ObjectKind;
use crate::pdf_array::PdfArray;
use crate::pdf_string::PdfString;
use crate::string_kind::StringKind;
use crate::{
    cross_reference_table::CrossReferenceTable, dictionary::Dictionary, object_error::ObjectError,
    object_id::ObjectId, object_resolver::ObjectResolver, stream::StreamObject, trailer::Trailer,
};
use num_traits::FromPrimitive;
/// Represents any PDF object as described in the PDF specification.
///
/// This enum is the central value type used across the crate to model
/// dictionaries, arrays, numbers, strings, streams, and other PDF constructs.
#[derive(Debug, PartialEq, Clone)]
pub enum ObjectVariant {
    /// A PDF dictionary object.
    Dictionary(Dictionary),
    /// A PDF array of objects.
    Array(PdfArray),
    /// A literal string (enclosed in parentheses in PDF syntax), stored as raw bytes.
    String(PdfString),
    /// A name object (prefixed with a slash in PDF syntax), stored as raw bytes.
    /// An integer number.
    Integer(i64),
    /// A real (floating point) number.
    Real(f64),
    /// A boolean value.
    Boolean(bool),
    /// The null object.
    Null,
    /// A hexadecimal string represented as raw bytes.
    /// The trailer dictionary object.
    Trailer(Trailer),
    /// The cross-reference table object.
    CrossReferenceTable(CrossReferenceTable),
    /// End-of-file marker.
    EndOfFile,
    /// An indirect reference pointing to an object number.
    Reference(ObjectId),
    /// A stream object, which may have associated dictionary and data.
    Stream(StreamObject),
}

impl ObjectVariant {
    /// Resolves an `ObjectVariant` into a `Dictionary`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a direct or stream object's dictionary.
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
            ObjectVariant::Dictionary(dict) => Ok(dict),
            ObjectVariant::Stream(stream) => Ok(&stream.dictionary),
            _ => Err(ObjectError::TypeMismatch("Dictionary", object.name())),
        }
    }

    /// Creates an owned PDF Name from borrowed bytes.
    ///
    /// Parsed names should be moved directly into [`ObjectVariant::String`].
    /// This constructor is intended for hand-built objects whose source is a
    /// borrowed byte literal or slice.
    pub fn name_from_bytes(name: &[u8]) -> Self {
        PdfString::from(name, StringKind::Name)
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

    /// Returns the runtime kind of this value.
    pub fn kind(&self) -> ObjectKind {
        match self {
            ObjectVariant::Null => ObjectKind::Null,
            ObjectVariant::Boolean(_) => ObjectKind::Boolean,
            ObjectVariant::Integer(_) => ObjectKind::Integer,
            ObjectVariant::Real(_) => ObjectKind::Real,
            ObjectVariant::String(value) if value.kind() == StringKind::Name => ObjectKind::Name,
            ObjectVariant::String(_) => ObjectKind::String,
            ObjectVariant::Array(_) => ObjectKind::Array,
            ObjectVariant::Dictionary(_) => ObjectKind::Dictionary,
            ObjectVariant::Stream(_) => ObjectKind::Stream,
            ObjectVariant::Reference(_) => ObjectKind::Reference,
            ObjectVariant::Trailer(_) => ObjectKind::Trailer,
            ObjectVariant::CrossReferenceTable(_) => ObjectKind::CrossReferenceTable,
            ObjectVariant::EndOfFile => ObjectKind::EndOfFile,
        }
    }

    /// Returns the dictionary of a direct dictionary or stream.
    ///
    /// No reference is followed, so no resolver is borrowed and the dictionary
    /// lives as long as this value.
    pub fn dictionary(&self) -> Result<&Dictionary, ObjectReadError> {
        match self {
            ObjectVariant::Dictionary(dictionary) => Ok(dictionary),
            ObjectVariant::Stream(stream) => Ok(&stream.dictionary),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Dictionary,
                actual: self.kind(),
            }),
        }
    }

    /// Returns `true` if this value is a Name object.
    pub fn is_name(&self) -> bool {
        matches!(self, ObjectVariant::String(value) if value.kind() == StringKind::Name)
    }

    /// Returns `true` if this value, or the object it references, spells `name`.
    ///
    /// Like [`Self::try_bytes`], strings are accepted alongside names because malformed
    /// PDFs sometimes encode one as the other. An unresolvable reference is not `name`.
    pub fn is_named(&self, name: &[u8], objects: &dyn ObjectResolver) -> bool {
        let object = match self {
            ObjectVariant::Reference(_) => objects.resolve_object(self),
            _ => Ok(self),
        };
        matches!(object, Ok(ObjectVariant::String(value)) if value.as_bytes() == name)
    }

    /// Returns `true` if this value is an `Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, ObjectVariant::Array(_))
    }

    /// Returns `true` if this value, or the object it references, is `Null`.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `bool` or `Err` if a reference cannot be resolved.
    pub fn is_null(&self, objects: &dyn ObjectResolver) -> Result<bool, ObjectError> {
        let object = if let ObjectVariant::Reference(_) = self {
            objects.resolve_object(self)?
        } else {
            self
        };

        Ok(matches!(object, ObjectVariant::Null))
    }

    /// Resolves an `ObjectVariant` into raw bytes.
    ///
    /// This function extracts the underlying byte representation from PDF Names
    /// and strings. Accepting both representations is intentional: malformed PDFs
    /// sometimes encode a Name as a string or a string as a Name.
    ///
    /// # Parameters
    ///
    /// - `objects`: A reference to the `ObjectResolver` used for resolving references.
    ///
    /// # Returns
    ///
    /// `&[u8]` or `Err` if the object is not byte-backed or if a reference cannot
    /// be resolved.
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
            ObjectVariant::String(value) => Ok(value.as_bytes()),
            ObjectVariant::Stream(stream) => Ok(stream.raw_data()),
            _ => Err(ObjectError::TypeMismatch("Bytes", object.name())),
        }
    }

    /// Resolves a byte-backed PDF object and returns an owned copy of its bytes.
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
            ObjectVariant::Reference(value) => Ok(value.number),
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
    pub fn name(&self) -> &'static str {
        match self {
            ObjectVariant::Dictionary(_) => "Dictionary",
            ObjectVariant::Array(_) => "Array",
            ObjectVariant::String(value) => {
                if value.kind() == StringKind::Name {
                    "Name"
                } else {
                    "String"
                }
            }
            ObjectVariant::Integer(_) => "Integer",
            ObjectVariant::Real(_) => "Real",
            ObjectVariant::Boolean(_) => "Boolean",
            ObjectVariant::Null => "Null",
            ObjectVariant::Stream(_) => "Stream",
            ObjectVariant::Trailer(_) => "Trailer",
            ObjectVariant::CrossReferenceTable(_) => "CrossReferenceTable",
            ObjectVariant::EndOfFile => "EndOfFile",
            ObjectVariant::Reference(_) => "Reference",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_resolver::PassthroughResolver;

    struct FixedResolver {
        object: ObjectVariant,
    }

    impl ObjectResolver for FixedResolver {
        fn resolve_object<'a>(
            &'a self,
            object: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match object {
                ObjectVariant::Reference(_) => Ok(&self.object),
                _ => Ok(object),
            }
        }
    }

    #[test]
    fn try_bytes_accepts_names_and_strings() {
        for (object, expected) in [
            (
                crate::pdf_string::PdfString::from(
                    vec![0xFF],
                    crate::string_kind::StringKind::Name,
                ),
                vec![0xFF],
            ),
            (
                crate::pdf_string::PdfString::from(
                    vec![0xFE],
                    crate::string_kind::StringKind::Literal,
                ),
                vec![0xFE],
            ),
            (
                crate::pdf_string::PdfString::from(
                    vec![0xFD],
                    crate::string_kind::StringKind::Hexadecimal,
                ),
                vec![0xFD],
            ),
        ] {
            assert_eq!(
                object
                    .try_bytes(&PassthroughResolver)
                    .expect("byte-backed PDF objects should return their raw bytes"),
                expected
            );
        }
    }

    #[test]
    fn try_bytes_accepts_resolved_byte_backed_object() {
        let reference = ObjectVariant::Reference(crate::object_id::ObjectId::new(1, 0));
        let resolver = FixedResolver {
            object: crate::pdf_string::PdfString::from(
                vec![0xFF],
                crate::string_kind::StringKind::Name,
            ),
        };

        assert_eq!(
            reference
                .try_bytes(&resolver)
                .expect("a reference to a byte-backed object should resolve"),
            [0xFF]
        );
    }

    #[test]
    fn try_number_returns_type_mismatch_for_non_number() {
        let object = ObjectVariant::Array(vec![].into());
        let err = object
            .try_number::<u16>(&PassthroughResolver)
            .expect_err("non-number object should not decode as number");

        assert_eq!(err, ObjectError::TypeMismatch("Number", "Array"));
    }

    #[test]
    fn try_bytes_returns_type_mismatch_for_non_bytes() {
        for (object, name) in [
            (ObjectVariant::Integer(7), "Integer"),
            (
                ObjectVariant::Array(crate::pdf_array::PdfArray::new(Vec::new())),
                "Array",
            ),
            (
                ObjectVariant::Dictionary(crate::dictionary::Dictionary::new(
                    std::collections::BTreeMap::<Vec<u8>, ObjectVariant>::new(),
                )),
                "Dictionary",
            ),
            (ObjectVariant::Boolean(true), "Boolean"),
        ] {
            let error = object
                .try_bytes(&PassthroughResolver)
                .expect_err("non-byte-backed object should not decode as bytes");

            assert_eq!(error, ObjectError::TypeMismatch("Bytes", name));
        }
    }
}
