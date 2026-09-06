use num_traits::ToPrimitive;
use pdf_object_reader::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};
use pdf_postscript::{operator::Operator, value::Value};

use crate::{
    error::FunctionReadError,
    function::{Function, FunctionImpl, get_pair},
    function_interpolation_error::FunctionInterpolationError,
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
    /// Builds the PostScript input stack, clamping each value to its domain entry.
    fn build_postscript_stack(
        inputs: &[f32],
        domain: &[f32],
        input_count: usize,
    ) -> Result<Vec<Value>, FunctionInterpolationError> {
        let mut stack = Vec::with_capacity(input_count);

        for i in 0..input_count {
            let (start, end) = get_pair(domain, i)
                .ok_or(FunctionInterpolationError::FunctionDataIndexOutOfBounds)?;

            let val = inputs
                .get(i)
                .copied()
                .ok_or(FunctionInterpolationError::InsufficientInputs {
                    expected: input_count,
                    got: inputs.len(),
                })?
                .clamp(start, end);

            stack.push(Value::Real(f64::from(val)));
        }

        Ok(stack)
    }

    /// Extracts and clamps outputs from the PostScript result stack.
    fn extract_postscript_outputs(
        result_stack: &[Value],
        range: &[f32],
        output_count: usize,
    ) -> Result<Vec<f32>, FunctionInterpolationError> {
        let mut outputs = Vec::with_capacity(output_count);

        for i in 0..output_count {
            let val = result_stack
                .get(i)
                .ok_or(FunctionInterpolationError::PostScriptResultStackUnderflow)?;
            let val = match val {
                Value::Integer(value) => f64::from(*value),
                Value::Real(value) => *value,
                Value::Bool(_) => {
                    return Err(FunctionInterpolationError::NonNumericPostScriptOutput);
                }
            };

            let (min, max) = get_pair(range, i)
                .ok_or(FunctionInterpolationError::FunctionDataIndexOutOfBounds)?;

            let v_f32 = val
                .to_f32()
                .ok_or(FunctionInterpolationError::NonFiniteNumericValue)?;

            outputs.push(v_f32.clamp(min, max));
        }

        Ok(outputs)
    }
}

impl FunctionImpl for PostScriptCalculatorFunction {
    /// Interpolates using PostScript calculator (Type 4 function).
    ///
    /// All input values are taken from `inputs` in order, clamped to their respective
    /// domain entries, and pushed onto the PostScript stack before execution.
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let input_count = self.domain.len() / 2;
        let output_count = self.range.len() / 2;

        if inputs.len() < input_count {
            return Err(FunctionInterpolationError::InsufficientInputs {
                expected: input_count,
                got: inputs.len(),
            });
        }

        // Build the input stack, clamping each input to its domain
        let stack = Self::build_postscript_stack(inputs, &self.domain, input_count)?;
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
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let stream = object.try_stream(objects)?;
        let domain = stream
            .dictionary
            .required_vec_of::<f32>(b"Domain", objects)?;

        let range = stream
            .dictionary
            .required_vec_of::<f32>(b"Range", objects)?;

        // Parse PostScript code: add spaces around braces for tokenization
        let code_str = String::from_utf8_lossy(stream.raw_data());
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
