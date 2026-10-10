use num_traits::ToPrimitive;
use pdf_graphics::interval::Interval;
use pdf_object_reader::{object_resolver::ObjectResolver, object_variant::ObjectVariant};
use pdf_postscript::{operator::Operator, value::Value};

use crate::{
    error::FunctionReadError,
    function::{Function, FunctionImpl},
    function_interpolation_error::FunctionInterpolationError,
};

#[derive(Debug, Clone)]
pub struct PostScriptCalculatorFunction {
    /// Parsed PostScript operators.
    operators: Vec<Operator>,
    /// Input domain, one interval per input.
    domain: Vec<Interval>,
    /// Output range, one interval per output.
    range: Vec<Interval>,
}

impl PostScriptCalculatorFunction {
    /// Builds the PostScript input stack, clamping each value to its domain entry.
    fn build_postscript_stack(
        inputs: &[f32],
        domain: &[Interval],
    ) -> Result<Vec<Value>, FunctionInterpolationError> {
        domain
            .iter()
            .enumerate()
            .map(|(i, interval)| {
                let val = inputs.get(i).copied().ok_or(
                    FunctionInterpolationError::InsufficientInputs {
                        expected: domain.len(),
                        got: inputs.len(),
                    },
                )?;
                Ok(Value::Real(f64::from(interval.clamp(val))))
            })
            .collect()
    }

    /// Extracts and clamps outputs from the PostScript result stack.
    fn extract_postscript_outputs(
        result_stack: &[Value],
        range: &[Interval],
    ) -> Result<Vec<f32>, FunctionInterpolationError> {
        let mut outputs = Vec::with_capacity(range.len());

        for (i, interval) in range.iter().enumerate() {
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

            let v_f32 = val
                .to_f32()
                .ok_or(FunctionInterpolationError::NonFiniteNumericValue)?;

            outputs.push(interval.clamp(v_f32));
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
        let input_count = self.domain.len();

        if inputs.len() < input_count {
            return Err(FunctionInterpolationError::InsufficientInputs {
                expected: input_count,
                got: inputs.len(),
            });
        }

        // Build the input stack, clamping each input to its domain
        let stack = Self::build_postscript_stack(inputs, &self.domain)?;
        // Execute PostScript operators
        let result_stack = pdf_postscript::calculator::execute(&stack, &self.operators)?;

        // Extract and clamp outputs
        Self::extract_postscript_outputs(&result_stack, &self.range)
    }

    fn domain(&self) -> Option<Interval> {
        self.domain.first().copied()
    }

    /// Parses a Type 4 (PostScript Calculator) function.
    fn parse(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let stream = object.try_stream(objects)?;
        let domain = stream.dictionary.required_intervals(b"Domain", objects)?;

        let range = stream.dictionary.required_intervals(b"Range", objects)?;

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
