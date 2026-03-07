use crate::{error::ObjectError, object_variant::ObjectVariant};

pub trait ObjectResolver {
    /// Resolves an object reference to its underlying object.
    ///
    /// If the provided `obj` is a [`ObjectVariant::Reference`], this method attempts to
    /// look up the referenced object by following chains of
    /// references (e.g., Ref -> Ref -> Object)
    ///
    /// If `obj` is not a reference, it is returned as-is.
    ///
    /// # Parameters
    ///
    /// - `obj`: The object to resolve.
    ///
    /// # Returns
    ///
    /// The resolved object or an error if a reference in the chain cannot
    /// be found in the collection.
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError>;
}

/// A resolver that returns every object unchanged without following references.
///
/// Use this only in contexts where the input is known to contain no
/// [`ObjectVariant::Reference`] values (e.g., early parsing phases before the
/// cross-reference table is available). Feeding a `Reference` into a
/// `try_*` helper through this resolver will return the reference itself,
/// not the resolved object, producing a wrong-type error rather than
/// resolving correctly.
pub struct PassthroughResolver;

impl ObjectResolver for PassthroughResolver {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        Ok(obj)
    }
}

/// Deprecated alias for [`PassthroughResolver`]. Use `PassthroughResolver` instead.
#[deprecated(note = "Use PassthroughResolver instead")]
pub type UnimplementedResolver = PassthroughResolver;
