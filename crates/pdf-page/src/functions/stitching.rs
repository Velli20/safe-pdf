use std::cmp::Ordering;

use pdf_object::{ObjectVariant, object_collection::ObjectCollection};

use crate::functions::{
    Function, FunctionImpl, FunctionInterpolationError, FunctionReadError, get_pair,
    linear_interpolate,
};

#[derive(Debug, Clone)]
pub struct StitchingFunction {
    /// Sub-functions to be stitched together.
    functions: Vec<Function>,
    /// Boundary values dividing the domain into sub-domains.
    bounds: Vec<f32>,
    /// Encoding values mapping sub-domains to sub-function domains.
    encode: Vec<f32>,
    /// Input domain `[min, max]`.
    domain: [f32; 2],
}

impl StitchingFunction {
    /// Returns the sub-domain `[b0, b1]` for the given segment index.
    fn get_subdomain(&self, index: usize) -> Result<(f32, f32), FunctionInterpolationError> {
        let b0 = if index == 0 {
            self.domain[0]
        } else {
            let prev_idx = index
                .checked_sub(1)
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;
            *self
                .bounds
                .get(prev_idx)
                .ok_or(FunctionInterpolationError::EncodeIndexError)?
        };

        let b1 = if index >= self.bounds.len() {
            self.domain[1]
        } else {
            *self
                .bounds
                .get(index)
                .ok_or(FunctionInterpolationError::EncodeIndexError)?
        };

        Ok((b0, b1))
    }
}

impl FunctionImpl for StitchingFunction {
    /// Interpolates using stitching (Type 3 function).
    ///
    /// Selects the appropriate sub-function based on bounds and maps the input
    /// to the sub-function's domain using the encode array.
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        // Validate structural invariants
        let expected_bounds_len = self
            .functions
            .len()
            .checked_sub(1)
            .ok_or(FunctionInterpolationError::InvalidBoundsLength)?;
        if self.bounds.len() != expected_bounds_len {
            return Err(FunctionInterpolationError::InvalidBoundsLength);
        }

        let expected_encode_len = self
            .functions
            .len()
            .checked_mul(2)
            .ok_or(FunctionInterpolationError::InvalidEncodeLength)?;
        if self.encode.len() != expected_encode_len {
            return Err(FunctionInterpolationError::InvalidEncodeLength);
        }

        // Clamp input to domain
        let x_clamped = x.clamp(self.domain[0], self.domain[1]);

        // Find which sub-function to use via binary search on bounds
        let index = match self
            .bounds
            .binary_search_by(|b| b.partial_cmp(&x_clamped).unwrap_or(Ordering::Less))
        {
            Ok(pos) => pos.saturating_add(1), // Exact match: use next segment
            Err(pos) => pos,
        };

        // Determine the sub-domain [b0, b1] for this segment
        let (b0, b1) = self.get_subdomain(index)?;

        // Get encoding values [e0, e1] for mapping to sub-function domain
        let (e0, e1) =
            get_pair(&self.encode, index).ok_or(FunctionInterpolationError::EncodeIndexError)?;
        // Map input from [b0, b1] to [e0, e1]
        let x_mapped = linear_interpolate(x_clamped, b0, b1, e0, e1);

        // Evaluate the selected sub-function
        let func = self
            .functions
            .get(index)
            .ok_or(FunctionInterpolationError::EncodeIndexError)?;
        func.interpolate(x_mapped)
    }

    fn domain(&self) -> Option<[f32; 2]> {
        Some(self.domain)
    }

    fn parse(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Function, FunctionReadError> {
        let dictionary = objects.resolve_dictionary(object)?;

        let domain = dictionary.get_or_err("Domain")?.as_array_of::<f32, 2>()?;

        // Parse /Functions array (sub-functions to stitch together)
        let functions_arr = dictionary.get_or_err("Functions")?.try_array(objects)?;
        let functions = functions_arr
            .iter()
            .map(|obj| Function::parse(obj, objects))
            .collect::<Result<Vec<_>, _>>()?;

        // Parse /Bounds array (boundaries between sub-functions)
        let bounds = dictionary.get_or_err("Bounds")?.as_vec_of::<f32>()?;

        // Parse /Encode array (input mapping for each sub-function)
        let encode = dictionary.get_or_err("Encode")?.as_vec_of::<f32>()?;

        // Validate structural relationships
        let expected_bounds = functions
            .len()
            .checked_sub(1)
            .ok_or(FunctionReadError::InvalidBoundsLength)?;
        if bounds.len() != expected_bounds {
            return Err(FunctionReadError::InvalidBoundsLength);
        }

        let expected_encode = functions
            .len()
            .checked_mul(2)
            .ok_or(FunctionReadError::InvalidEncodeLength)?;
        if encode.len() != expected_encode {
            return Err(FunctionReadError::InvalidEncodeLength);
        }

        Ok(Function::Stitching(StitchingFunction {
            functions,
            bounds,
            encode,
            domain,
        }))
    }
}
