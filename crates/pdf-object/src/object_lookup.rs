use num_traits::FromPrimitive;

use crate::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};

/// Provides typed lookup helpers for PDF object containers.
///
/// Implementations support dictionary key lookup and array index lookup while
/// delegating object conversion to [`ObjectVariant`] conversion methods.
///
/// # Type Parameters
///
/// - `K`: The lookup key type used by the container. Dictionaries use `&str`
///   keys and arrays use `usize` indexes.
pub trait ObjectLookupExt<K> {
    /// Returns an optional dictionary value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_dictionary`]. Missing entries and explicit PDF
    /// `null` values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(&Dictionary))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_dictionary<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a Dictionary>, ObjectError>;

    /// Returns a required dictionary value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_dictionary`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&Dictionary)` when the value exists and converts
    /// successfully, or `Err` if the value is missing, reference resolution
    /// fails, or conversion fails.
    fn required_dictionary<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a Dictionary, ObjectError>;

    /// Returns an optional stream value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_stream`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(&StreamObject))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_stream<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a StreamObject>, ObjectError>;

    /// Returns a required stream value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_stream`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&StreamObject)` when the value exists and converts
    /// successfully, or `Err` if the value is missing, reference resolution
    /// fails, or conversion fails.
    fn required_stream<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a StreamObject, ObjectError>;

    /// Returns an optional array value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_array`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(&[ObjectVariant]))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_array<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [ObjectVariant]>, ObjectError>;

    /// Returns a required array value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_array`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&[ObjectVariant])` when the value exists and converts
    /// successfully, or `Err` if the value is missing, reference resolution
    /// fails, or conversion fails.
    fn required_array<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [ObjectVariant], ObjectError>;

    /// Returns an optional UTF-8 string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_str`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(&str))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_str<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a str>, ObjectError>;

    /// Returns a required UTF-8 string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_str`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&str)` when the value exists and converts successfully, or
    /// `Err` if the value is missing, reference resolution fails, or
    /// conversion fails.
    fn required_str<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a str, ObjectError>;

    /// Returns an optional byte string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_bytes`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(&[u8]))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_bytes<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [u8]>, ObjectError>;

    /// Returns a required byte string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_bytes`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&[u8])` when the value exists and converts successfully, or
    /// `Err` if the value is missing, reference resolution fails, or conversion
    /// fails.
    fn required_bytes<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError>;

    /// Returns an optional owned byte string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_bytes_vec`]. Missing entries and explicit PDF
    /// `null` values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Vec<u8>))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_bytes_vec<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<Vec<u8>>, ObjectError>;

    /// Returns a required owned byte string value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_bytes_vec`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<u8>)` when the value exists and converts successfully,
    /// or `Err` if the value is missing, reference resolution fails, or
    /// conversion fails.
    fn required_bytes_vec<'a>(
        &'a self,
        key: K,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Vec<u8>, ObjectError>;

    /// Returns an optional numeric value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_number`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert the object to.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(T))` when the value exists and converts successfully,
    /// `Ok(None)` when the value is missing or resolves to `null`, or `Err` if
    /// reference resolution or numeric conversion fails.
    fn optional_number<T>(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<T>, ObjectError>
    where
        T: FromPrimitive;

    /// Returns a required numeric value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_number`].
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert the object to.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(T)` when the value exists and converts successfully, or
    /// `Err` if the value is missing, reference resolution fails, or numeric
    /// conversion fails.
    fn required_number<T>(&self, key: K, objects: &dyn ObjectResolver) -> Result<T, ObjectError>
    where
        T: FromPrimitive;

    /// Returns an optional numeric vector value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_vec_of`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each array item to.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Vec<T>))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_vec_of<T>(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Vec<T>>, ObjectError>
    where
        T: FromPrimitive + Copy + Default;

    /// Returns a required numeric vector value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_vec_of`].
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each array item to.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<T>)` when the value exists and converts successfully,
    /// or `Err` if the value is missing, reference resolution fails, or
    /// conversion fails.
    fn required_vec_of<T>(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<T>, ObjectError>
    where
        T: FromPrimitive + Copy + Default;

    /// Returns an optional fixed-size numeric array value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_array_of`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each array item to.
    /// - `N`: The expected array length.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some([T; N]))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_array_of<T, const N: usize>(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<[T; N]>, ObjectError>
    where
        T: FromPrimitive + Copy + Default;

    /// Returns a required fixed-size numeric array value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_array_of`].
    ///
    /// # Type Parameters
    ///
    /// - `T`: The numeric type to convert each array item to.
    /// - `N`: The expected array length.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok([T; N])` when the value exists and converts successfully,
    /// or `Err` if the value is missing, reference resolution fails, or
    /// conversion fails.
    fn required_array_of<T, const N: usize>(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default;

    /// Returns an optional boolean value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_boolean`]. Missing entries and explicit PDF `null`
    /// values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(bool))` when the value exists and converts
    /// successfully, `Ok(None)` when the value is missing or resolves to
    /// `null`, or `Err` if reference resolution or conversion fails.
    fn optional_boolean(
        &self,
        key: K,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<bool>, ObjectError>;

    /// Returns a required boolean value from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_boolean`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    /// - `objects`: The object resolver used when the looked-up value is an
    ///   indirect reference.
    ///
    /// # Returns
    ///
    /// Returns `Ok(bool)` when the value exists and converts successfully, or
    /// `Err` if the value is missing, reference resolution fails, or conversion
    /// fails.
    fn required_boolean(&self, key: K, objects: &dyn ObjectResolver) -> Result<bool, ObjectError>;

    /// Returns an optional object number from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_object_number`]. Missing entries and explicit PDF
    /// `null` values are treated as absent.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(usize))` when the value exists and has an object
    /// number, `Ok(None)` when the value is missing or is `null`, or `Err` if
    /// conversion fails.
    fn optional_object_number(&self, key: K) -> Result<Option<usize>, ObjectError>;

    /// Returns a required object number from this container.
    ///
    /// This method looks up a value by key or index and converts it using
    /// [`ObjectVariant::try_object_number`].
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key or array index to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(usize)` when the value exists and has an object number, or
    /// `Err` if the value is missing or conversion fails.
    fn required_object_number(&self, key: K) -> Result<usize, ObjectError>;
}

impl ObjectLookupExt<usize> for [ObjectVariant] {
    fn optional_dictionary<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a Dictionary>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_dictionary(objects))
            .transpose()
    }

    fn required_dictionary<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a Dictionary, ObjectError> {
        required_slice_value(self, index)?.try_dictionary(objects)
    }

    fn optional_stream<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a StreamObject>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_stream(objects))
            .transpose()
    }

    fn required_stream<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a StreamObject, ObjectError> {
        required_slice_value(self, index)?.try_stream(objects)
    }

    fn optional_array<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [ObjectVariant]>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_array(objects))
            .transpose()
    }

    fn required_array<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [ObjectVariant], ObjectError> {
        required_slice_value(self, index)?.try_array(objects)
    }

    fn optional_str<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a str>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_str(objects))
            .transpose()
    }

    fn required_str<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a str, ObjectError> {
        required_slice_value(self, index)?.try_str(objects)
    }

    fn optional_bytes<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [u8]>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_bytes(objects))
            .transpose()
    }

    fn required_bytes<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError> {
        required_slice_value(self, index)?.try_bytes(objects)
    }

    fn optional_bytes_vec<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<Vec<u8>>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_bytes_vec(objects))
            .transpose()
    }

    fn required_bytes_vec<'a>(
        &'a self,
        index: usize,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Vec<u8>, ObjectError> {
        required_slice_value(self, index)?.try_bytes_vec(objects)
    }

    fn optional_number<T>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<T>, ObjectError>
    where
        T: FromPrimitive,
    {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_number(objects))
            .transpose()
    }

    fn required_number<T>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<T, ObjectError>
    where
        T: FromPrimitive,
    {
        required_slice_value(self, index)?.try_number(objects)
    }

    fn optional_vec_of<T>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Vec<T>>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_vec_of(objects))
            .transpose()
    }

    fn required_vec_of<T>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<T>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        required_slice_value(self, index)?.try_vec_of(objects)
    }

    fn optional_array_of<T, const N: usize>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<[T; N]>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_array_of(objects))
            .transpose()
    }

    fn required_array_of<T, const N: usize>(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        required_slice_value(self, index)?.try_array_of(objects)
    }

    fn optional_boolean(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<bool>, ObjectError> {
        optional_resolved_value(self.get(index), objects)?
            .map(|value| value.try_boolean(objects))
            .transpose()
    }

    fn required_boolean(
        &self,
        index: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<bool, ObjectError> {
        required_slice_value(self, index)?.try_boolean(objects)
    }

    fn optional_object_number(&self, index: usize) -> Result<Option<usize>, ObjectError> {
        optional_direct_value(self.get(index))?
            .map(ObjectVariant::try_object_number)
            .transpose()
    }

    fn required_object_number(&self, index: usize) -> Result<usize, ObjectError> {
        required_slice_value(self, index)?.try_object_number()
    }
}

impl ObjectLookupExt<&str> for Dictionary {
    fn optional_dictionary<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a Dictionary>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_dictionary(objects))
            .transpose()
    }

    fn required_dictionary<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a Dictionary, ObjectError> {
        self.get_or_err(key)?.try_dictionary(objects)
    }

    fn optional_stream<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a StreamObject>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_stream(objects))
            .transpose()
    }

    fn required_stream<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a StreamObject, ObjectError> {
        self.get_or_err(key)?.try_stream(objects)
    }

    fn optional_array<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [ObjectVariant]>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_array(objects))
            .transpose()
    }

    fn required_array<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [ObjectVariant], ObjectError> {
        self.get_or_err(key)?.try_array(objects)
    }

    fn optional_str<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a str>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_str(objects))
            .transpose()
    }

    fn required_str<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a str, ObjectError> {
        self.get_or_err(key)?.try_str(objects)
    }

    fn optional_bytes<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<&'a [u8]>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_bytes(objects))
            .transpose()
    }

    fn required_bytes<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<&'a [u8], ObjectError> {
        self.get_or_err(key)?.try_bytes(objects)
    }

    fn optional_bytes_vec<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Option<Vec<u8>>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_bytes_vec(objects))
            .transpose()
    }

    fn required_bytes_vec<'a>(
        &'a self,
        key: &str,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Vec<u8>, ObjectError> {
        self.get_or_err(key)?.try_bytes_vec(objects)
    }

    fn optional_number<T>(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<T>, ObjectError>
    where
        T: FromPrimitive,
    {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_number(objects))
            .transpose()
    }

    fn required_number<T>(&self, key: &str, objects: &dyn ObjectResolver) -> Result<T, ObjectError>
    where
        T: FromPrimitive,
    {
        self.get_or_err(key)?.try_number(objects)
    }

    fn optional_vec_of<T>(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Vec<T>>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_vec_of(objects))
            .transpose()
    }

    fn required_vec_of<T>(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<T>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        self.get_or_err(key)?.try_vec_of(objects)
    }

    fn optional_array_of<T, const N: usize>(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<[T; N]>, ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_array_of(objects))
            .transpose()
    }

    fn required_array_of<T, const N: usize>(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        self.get_or_err(key)?.try_array_of(objects)
    }

    fn optional_boolean(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<bool>, ObjectError> {
        optional_resolved_value(self.get(key), objects)?
            .map(|value| value.try_boolean(objects))
            .transpose()
    }

    fn required_boolean(
        &self,
        key: &str,
        objects: &dyn ObjectResolver,
    ) -> Result<bool, ObjectError> {
        self.get_or_err(key)?.try_boolean(objects)
    }

    fn optional_object_number(&self, key: &str) -> Result<Option<usize>, ObjectError> {
        optional_direct_value(self.get(key))?
            .map(ObjectVariant::try_object_number)
            .transpose()
    }

    fn required_object_number(&self, key: &str) -> Result<usize, ObjectError> {
        self.get_or_err(key)?.try_object_number()
    }
}

fn optional_resolved_value<'a>(
    value: Option<&'a ObjectVariant>,
    objects: &'a dyn ObjectResolver,
) -> Result<Option<&'a ObjectVariant>, ObjectError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = if let ObjectVariant::Reference(_) = value {
        objects.resolve_object(value)?
    } else {
        value
    };

    match value {
        ObjectVariant::Null => Ok(None),
        _ => Ok(Some(value)),
    }
}

fn optional_direct_value(
    value: Option<&ObjectVariant>,
) -> Result<Option<&ObjectVariant>, ObjectError> {
    match value {
        Some(ObjectVariant::Null) | None => Ok(None),
        Some(value) => Ok(Some(value)),
    }
}

fn required_slice_value(
    values: &[ObjectVariant],
    index: usize,
) -> Result<&ObjectVariant, ObjectError> {
    values
        .get(index)
        .ok_or_else(|| ObjectError::InvalidArrayLength {
            expected: index.saturating_add(1),
            found: values.len(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::object_resolver::PassthroughResolver;

    use super::*;

    struct NullResolver;

    impl ObjectResolver for NullResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match obj {
                ObjectVariant::Reference(_) => Ok(&NULL_OBJECT),
                _ => Ok(obj),
            }
        }
    }

    static NULL_OBJECT: ObjectVariant = ObjectVariant::Null;

    fn dictionary_with(key: &str, value: ObjectVariant) -> Dictionary {
        Dictionary::new(BTreeMap::from([(key.to_owned(), value)]))
    }

    #[test]
    fn slice_optional_number_converts_existing_value() {
        let values = [ObjectVariant::Integer(42)];

        let value = values
            .optional_number::<u16>(0, &PassthroughResolver)
            .expect("integer converts to u16");

        assert_eq!(value, Some(42));
    }

    #[test]
    fn slice_optional_number_returns_none_for_missing_or_null() {
        let values = [ObjectVariant::Null, ObjectVariant::Reference(1)];

        let missing = values
            .optional_number::<u16>(2, &PassthroughResolver)
            .expect("missing optional item is absent");
        let direct_null = values
            .optional_number::<u16>(0, &PassthroughResolver)
            .expect("null optional item is absent");
        let resolved_null = values
            .optional_number::<u16>(1, &NullResolver)
            .expect("resolved null optional item is absent");

        assert_eq!(missing, None);
        assert_eq!(direct_null, None);
        assert_eq!(resolved_null, None);
    }

    #[test]
    fn slice_required_number_reports_missing_index() {
        let values = [ObjectVariant::Integer(42)];

        let error = values
            .required_number::<u16>(2, &PassthroughResolver)
            .expect_err("missing required item fails");

        assert_eq!(
            error,
            ObjectError::InvalidArrayLength {
                expected: 3,
                found: 1
            }
        );
    }

    #[test]
    fn dictionary_optional_number_converts_existing_value() {
        let dictionary = dictionary_with("Count", ObjectVariant::Real(12.0));

        let value = dictionary
            .optional_number::<f32>("Count", &PassthroughResolver)
            .expect("real converts to f32");

        assert_eq!(value, Some(12.0));
    }

    #[test]
    fn dictionary_optional_number_returns_none_for_missing_or_null() {
        let dictionary = dictionary_with("Null", ObjectVariant::Null);

        let missing = dictionary
            .optional_number::<u16>("Missing", &PassthroughResolver)
            .expect("missing optional key is absent");
        let direct_null = dictionary
            .optional_number::<u16>("Null", &PassthroughResolver)
            .expect("null optional key is absent");

        assert_eq!(missing, None);
        assert_eq!(direct_null, None);
    }

    #[test]
    fn dictionary_required_number_reports_missing_key() {
        let dictionary = Dictionary::new(BTreeMap::new());

        let error = dictionary
            .required_number::<u16>("Count", &PassthroughResolver)
            .expect_err("missing required key fails");

        assert_eq!(
            error,
            ObjectError::MissingRequiredKey {
                key: "Count".to_owned()
            }
        );
    }

    #[test]
    fn dictionary_lookup_methods_convert_common_types() {
        let nested = Dictionary::new(BTreeMap::new());
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "Dictionary".to_owned(),
                ObjectVariant::Dictionary(Box::new(nested.clone())),
            ),
            (
                "Stream".to_owned(),
                ObjectVariant::Stream(StreamObject::new(
                    7,
                    0,
                    Box::new(Dictionary::new(BTreeMap::new())),
                    Vec::new(),
                )),
            ),
            (
                "Array".to_owned(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(2)]),
            ),
            ("String".to_owned(), ObjectVariant::Name(b"Name".to_vec())),
            (
                "Bytes".to_owned(),
                ObjectVariant::LiteralString(b"abc".to_vec()),
            ),
            ("Boolean".to_owned(), ObjectVariant::Boolean(true)),
            ("Reference".to_owned(), ObjectVariant::Reference(9)),
        ]));

        let stream = dictionary
            .required_stream("Stream", &PassthroughResolver)
            .expect("stream exists");

        assert_eq!(
            dictionary
                .required_dictionary("Dictionary", &PassthroughResolver)
                .expect("dictionary exists"),
            &nested
        );
        assert_eq!(stream.object_number, 7);
        assert_eq!(
            dictionary
                .required_array("Array", &PassthroughResolver)
                .expect("array exists")
                .len(),
            2
        );
        assert_eq!(
            dictionary
                .required_str("String", &PassthroughResolver)
                .expect("string exists"),
            "Name"
        );
        assert_eq!(
            dictionary
                .required_bytes("Bytes", &PassthroughResolver)
                .expect("bytes exist"),
            b"abc"
        );
        assert_eq!(
            dictionary
                .required_bytes_vec("Bytes", &PassthroughResolver)
                .expect("bytes exist"),
            b"abc".to_vec()
        );
        assert!(
            dictionary
                .required_boolean("Boolean", &PassthroughResolver)
                .expect("boolean exists")
        );
        assert_eq!(
            dictionary
                .required_object_number("Reference")
                .expect("reference object number exists"),
            9
        );
    }

    #[test]
    fn dictionary_lookup_methods_convert_numeric_arrays() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "Numbers".to_owned(),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(2),
                ObjectVariant::Integer(3),
            ]),
        )]));

        assert_eq!(
            dictionary
                .required_vec_of::<u8>("Numbers", &PassthroughResolver)
                .expect("numeric vector converts"),
            vec![1, 2, 3]
        );
        assert_eq!(
            dictionary
                .required_array_of::<u8, 3>("Numbers", &PassthroughResolver)
                .expect("numeric array converts"),
            [1, 2, 3]
        );
    }

    #[test]
    fn slice_lookup_methods_convert_common_types() {
        let nested = Dictionary::new(BTreeMap::new());
        let values = [
            ObjectVariant::Dictionary(Box::new(nested.clone())),
            ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(2)]),
            ObjectVariant::LiteralString(b"text".to_vec()),
            ObjectVariant::Boolean(false),
            ObjectVariant::Reference(3),
        ];

        assert_eq!(
            values
                .required_dictionary(0, &PassthroughResolver)
                .expect("dictionary exists"),
            &nested
        );
        assert_eq!(
            values
                .required_array(1, &PassthroughResolver)
                .expect("array exists")
                .len(),
            2
        );
        assert_eq!(
            values
                .required_str(2, &PassthroughResolver)
                .expect("string exists"),
            "text"
        );
        assert!(
            !values
                .required_boolean(3, &PassthroughResolver)
                .expect("boolean exists")
        );
        assert_eq!(
            values
                .required_object_number(4)
                .expect("reference object number exists"),
            3
        );
    }

    #[test]
    fn optional_dictionary_returns_none_for_resolved_null() {
        let values = [ObjectVariant::Reference(1)];

        let value = values
            .optional_dictionary(0, &NullResolver)
            .expect("resolved null is absent");

        assert!(value.is_none());
    }

    #[test]
    fn required_lookup_propagates_type_mismatch() {
        let dictionary = dictionary_with("Boolean", ObjectVariant::Boolean(true));

        let error = dictionary
            .required_str("Boolean", &PassthroughResolver)
            .expect_err("boolean is not a string");

        assert_eq!(error, ObjectError::TypeMismatch("String", "Boolean"));
    }

    #[test]
    fn required_str_propagates_invalid_utf8() {
        let dictionary = dictionary_with("Name", ObjectVariant::Name(vec![0xFF]));

        let error = dictionary
            .required_str("Name", &PassthroughResolver)
            .expect_err("invalid utf-8 should fail");

        assert!(matches!(
            error,
            ObjectError::InvalidUtf8String {
                object_type: "Name",
                ..
            }
        ));
    }

    #[test]
    fn optional_str_propagates_invalid_utf8() {
        let dictionary = dictionary_with("Name", ObjectVariant::LiteralString(vec![0xFF]));

        let error = dictionary
            .optional_str("Name", &PassthroughResolver)
            .expect_err("invalid utf-8 should fail");

        assert!(matches!(
            error,
            ObjectError::InvalidUtf8String {
                object_type: "LiteralString",
                ..
            }
        ));
    }
}
