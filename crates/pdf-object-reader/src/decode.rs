//! Extensible typed decoding with one object-bound context per invocation.

use crate::context::ObjectContext;
use crate::error::{ObjectReadError, ReadResult};
use crate::object_kind::ObjectKind;
use crate::object_variant::ObjectVariant;
use crate::pdf_array::PdfArray;
use crate::pdf_name::PdfName;
use crate::pdf_object::PdfObject;
use crate::pdf_string::PdfString;
use crate::reader::ObjectAccess;
use crate::resolved_object::ResolvedObject;
use crate::string_kind::StringKind;
use crate::{dictionary::Dictionary, stream::StreamObject};

/// Decodes a resolved PDF value using one object-bound context.
///
/// Implement this trait on client-owned types. The context exposes the current
/// value, typed child reads, content-stream IDs, and cycle-protected access.
/// Implementations should use context reads for recursion rather than starting
/// fresh sessions.
///
/// There are no ownership or thread-safety bounds on eager outputs.
/// Shared reading additionally requires `Self: Send + Sync + 'static`.
///
/// Implementations must not retain strong pointers to other cached values as
/// graph edges: store `ObjectHandle<T>` for indirect edges to avoid ownership
/// cycles. Do not demand a pending handle's value while constructing a cycle.
///
/// Shared reading rejects a decoder whose output contains a non-thread-safe Rc:
///
/// ```compile_fail,E0277
/// use std::rc::Rc;
/// use pdf_object_reader::{
///     FromPdfObject, ObjectAccess, ObjectContext, object_id::ObjectId, ObjectReader,
///     ObjectSource, ReadResult,
/// };
///
/// struct Local(Rc<u8>);
///
/// impl FromPdfObject for Local {
///     fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>
///     {
///         todo!("client decoder")
///     }
/// }
///
/// fn invalid_shared_read<S: ObjectSource>(reader: &ObjectReader<S>, id: ObjectId) {
///     let _ = reader.read_shared_indirect::<Local>(id);
/// }
/// ```
pub trait FromPdfObject: Sized {
    /// Converts the context's object, retaining its session for all nested reads.
    ///
    /// Return domain failures using `ObjectReadError::Decode`; shape and child
    /// failures propagate through `ReadResult`.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>;
}

macro_rules! value_decoder {
    ($type:ty, $variant:ident, $kind:ident) => {
        impl FromPdfObject for $type {
            fn from_pdf_object(
                context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
            ) -> ReadResult<Self> {
                match context.object().value() {
                    ObjectVariant::$variant(value) => Ok(value.clone()),
                    _ => Err(ObjectReadError::TypeMismatch {
                        expected: ObjectKind::$kind,
                        actual: context.object().kind(),
                    }),
                }
            }
        }
    };
}

value_decoder!(Dictionary, Dictionary, Dictionary);
value_decoder!(StreamObject, Stream, Stream);

impl FromPdfObject for () {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::Null => Ok(()),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Null,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for f64 {
    #[allow(
        clippy::as_conversions,
        reason = "PDF integer-to-real conversion intentionally permits rounding"
    )]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::Real(value) => Ok(*value),
            ObjectVariant::Integer(value) => Ok(*value as f64),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Real,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for PdfObject {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(context.object().object().clone())
    }
}

impl FromPdfObject for ResolvedObject {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(context.object().clone())
    }
}

/// Decodes PDF null as absence and delegates other values to the inner decoder.
///
/// An absent dictionary key is handled by the dictionary context, not this trait.
impl<T: FromPdfObject> FromPdfObject for Option<T> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if context.object().kind() == ObjectKind::Null {
            Ok(None)
        } else {
            T::from_pdf_object(context).map(Some)
        }
    }
}

/// Decodes array elements in source order within the current traversal.
impl<T: FromPdfObject> FromPdfObject for Vec<T> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context.array()?.read_all()
    }
}

impl FromPdfObject for PdfName {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::Name(value) => Ok(PdfName::from_bytes(value)),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Name,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for PdfString {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::LiteralString(value) => Ok(PdfString::new(value, StringKind::Literal)),
            ObjectVariant::HexString(value) => Ok(PdfString::new(value, StringKind::Hexadecimal)),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::String,
                actual: context.object().kind(),
            }),
        }
    }
}

impl FromPdfObject for PdfArray {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::Array(value) => Ok(PdfArray::new(value.clone())),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Array,
                actual: context.object().kind(),
            }),
        }
    }
}

macro_rules! number_decoder {
    ($($type:ty),+ $(,)?) => {
        $(
            impl FromPdfObject for $type {
                /// Decodes a resolved number with the model's checked numeric conversion.
                fn from_pdf_object(
                    context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
                ) -> ReadResult<Self> {
                    // The context has already resolved references and owns their scope.
                    Ok(context.object().value().try_number(context.source())?)
                }
            }
        )+
    };
}

number_decoder!(f32, u16, u32, usize);

/// Reads exactly N elements, validating length before decoding any child.
impl<T: FromPdfObject, const N: usize> FromPdfObject for [T; N] {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.array()?;
        if context.array().len() != N {
            return Err(crate::object_error::ObjectError::InvalidArrayLength {
                expected: N,
                found: context.array().len(),
            }
            .into());
        }
        context
            .read_all::<T>()?
            .try_into()
            .map_err(|values: Vec<T>| {
                crate::object_error::ObjectError::InvalidArrayLength {
                    expected: N,
                    found: values.len(),
                }
                .into()
            })
    }
}

/// Reads the bytes of a name or string, accepting producer substitutions between them.
///
/// Use `PdfName` or `PdfString` when the PDF object kind must be enforced.
impl FromPdfObject for std::sync::Arc<[u8]> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self::from(
            context.object().value().try_bytes(context.source())?,
        ))
    }
}
