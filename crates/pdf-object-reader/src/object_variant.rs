//! PDF object variants and checked accessors.

use crate::error::ObjectReadError;
use crate::object_kind::ObjectKind;
use crate::pdf_array::PdfArray;
use crate::pdf_string::PdfString;
use crate::string_kind::StringKind;
use crate::{
    cross_reference_table::CrossReferenceTable, dictionary::Dictionary, object_error::ObjectError,
    object_id::ObjectId, stream::StreamObject, trailer::Trailer,
};
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
    /// Creates an owned PDF Name from borrowed bytes.
    ///
    /// Parsed names should be moved directly into [`ObjectVariant::String`].
    /// This constructor is intended for hand-built objects whose source is a
    /// borrowed byte literal or slice.
    pub fn name_from_bytes(name: &[u8]) -> Self {
        PdfString::from(name, StringKind::Name)
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

    /// Returns `true` if this value is an `Array`.
    pub fn is_array(&self) -> bool {
        matches!(self, ObjectVariant::Array(_))
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
