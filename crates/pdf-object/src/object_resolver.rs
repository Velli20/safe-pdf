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

pub struct UnimplementedResolver;

impl ObjectResolver for UnimplementedResolver {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        Ok(obj)
    }
}
