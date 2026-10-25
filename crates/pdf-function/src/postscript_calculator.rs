use num_traits::ToPrimitive;
use pdf_graphics::interval::Interval;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use pdf_postscript::{operator::Operator, value::Value};

use crate::{
    error::FunctionReadError, function::FunctionImpl,
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
    /// Extracts and clamps outputs from the PostScript result stack.
    fn outputs(&self, result_stack: &[Value]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let mut outputs = Vec::with_capacity(self.range.len());

        for (i, interval) in self.range.iter().enumerate() {
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
        let inputs = inputs.get(..self.domain.len()).ok_or(
            FunctionInterpolationError::InsufficientInputs {
                expected: self.domain.len(),
                got: inputs.len(),
            },
        )?;

        // Build the input stack, clamping each input to its domain
        let stack: Vec<Value> = self
            .domain
            .iter()
            .zip(inputs)
            .map(|(interval, &x)| Value::Real(f64::from(interval.clamp(x))))
            .collect();
        // Execute PostScript operators
        let result_stack = pdf_postscript::calculator::execute(stack, &self.operators)?;

        // Extract and clamp outputs
        self.outputs(&result_stack)
    }

    fn domain(&self) -> Option<Interval> {
        self.domain.first().copied()
    }
}

/// Reads a Type 4 (PostScript Calculator) function stream.
impl FromPdfObject for PostScriptCalculatorFunction {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.stream()?;
        let mut dictionary = context.dictionary();
        let domain = dictionary.required_intervals(b"Domain")?;
        let range = dictionary.required_intervals(b"Range")?;

        let operators = pdf_postscript::parser::parse(context.stream().raw_data())
            .map_err(FunctionReadError::from)?;

        Ok(Self {
            operators,
            domain,
            range,
        })
    }
}
