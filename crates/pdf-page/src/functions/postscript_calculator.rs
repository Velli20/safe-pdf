use num_traits::ToPrimitive;
use pdf_object::{ObjectVariant, object_collection::ObjectCollection};
use pdf_postscript::operator::Operator;

use crate::functions::{
    Function, FunctionImpl, FunctionInterpolationError, FunctionReadError, get_pair,
};

#[derive(Debug, Clone)]
pub struct PostScriptCalculatorFunction {
    /// Parsed PostScript operators.
    operators: Vec<Operator>,
    /// Input domain as pairs `[min0, max0, min1, max1, ...]`.
    domain: Vec<f32>,
    /// Output range as pairs `[min0, max0, min1, max1, ...]`.
    range: Vec<f32>,
}

impl PostScriptCalculatorFunction {
    /// Builds the input stack for PostScript evaluation.
    fn build_postscript_stack(
        x: f32,
        domain: &[f32],
        input_count: usize,
    ) -> Result<Vec<f64>, FunctionInterpolationError> {
        let mut stack = Vec::with_capacity(input_count);

        for i in 0..input_count {
            let (start, end) =
                get_pair(domain, i).ok_or(FunctionInterpolationError::EncodeIndexError)?;

            // Placeholder behavior for multi-input functions: reuse the same input value
            let val = x.clamp(start, end);

            stack.push(f64::from(val));
        }

        Ok(stack)
    }

    /// Extracts and clamps outputs from the PostScript result stack.
    fn extract_postscript_outputs(
        result_stack: &[f64],
        range: &[f32],
        output_count: usize,
    ) -> Result<Vec<f32>, FunctionInterpolationError> {
        let mut outputs = Vec::with_capacity(output_count);

        for i in 0..output_count {
            let val = *result_stack
                .get(i)
                .ok_or(FunctionInterpolationError::InsufficientResultStack)?;

            let (min, max) =
                get_pair(range, i).ok_or(FunctionInterpolationError::EncodeIndexError)?;

            let v_f32 = val.to_f32().ok_or(FunctionInterpolationError::InputIsNaN)?;

            outputs.push(v_f32.clamp(min, max));
        }

        Ok(outputs)
    }
}

impl FunctionImpl for PostScriptCalculatorFunction {
    /// Interpolates using PostScript calculator (Type 4 function).
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        let input_count = self.domain.len() / 2;
        let output_count = self.range.len() / 2;

        // Build the input stack, clamping each input to its domain
        let stack = Self::build_postscript_stack(x, &self.domain, input_count)?;
        // Execute PostScript operators
        let result_stack = pdf_postscript::calculator::execute(&stack, &self.operators)?;

        // Extract and clamp outputs
        Self::extract_postscript_outputs(&result_stack, &self.range, output_count)
    }

    fn domain(&self) -> Option<[f32; 2]> {
        let first = *self.domain.first()?;
        let second = *self.domain.get(1)?;
        Some([first, second])
    }

    /// Parses a Type 4 (PostScript Calculator) function.
    fn parse(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Function, FunctionReadError> {
        let stream = objects.resolve_stream(object)?;
        let domain = stream.dictionary.get_or_err("Domain")?.as_vec_of::<f32>()?;

        let range = stream.dictionary.get_or_err("Range")?.as_vec_of::<f32>()?;

        // Parse PostScript code: add spaces around braces for tokenization
        let stream_data = stream.data()?;

        let code_str = String::from_utf8_lossy(&stream_data);
        let code_with_spaces = code_str.replace('{', " { ").replace('}', " } ");
        let tokens: Vec<&str> = code_with_spaces.split_whitespace().collect();
        let operators = pdf_postscript::parser::parse_tokens(&tokens)?;

        Ok(Function::PostScriptCalculator(
            PostScriptCalculatorFunction {
                operators,
                domain,
                range,
            },
        ))
    }
}
