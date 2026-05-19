use crate::error::PdfCanvasError;

/// Dash pattern used for stroking paths.
#[derive(Clone, Debug, PartialEq)]
pub struct DashPattern {
    /// Alternating dash and gap lengths.
    pub intervals: Vec<f32>,
    /// Distance into the dash pattern where stroking starts.
    pub phase: f32,
}

impl DashPattern {
    /// Builds a validated dash pattern from PDF dash operands.
    ///
    /// An empty dash array represents a solid stroke and returns `Ok(None)`.
    pub fn new(intervals: &[f32], phase: f32) -> Result<Option<Self>, PdfCanvasError> {
        if intervals.is_empty() {
            return Ok(None);
        }

        let mut has_positive_interval = false;
        for interval in intervals {
            if !interval.is_finite() || *interval < 0.0 {
                return Err(PdfCanvasError::InvalidDashPattern(
                    "dash intervals must be finite and non-negative".into(),
                ));
            }
            if *interval > 0.0 {
                has_positive_interval = true;
            }
        }

        if !has_positive_interval {
            return Err(PdfCanvasError::InvalidDashPattern(
                "dash pattern must contain at least one positive interval".into(),
            ));
        }

        if !phase.is_finite() || phase < 0.0 {
            return Err(PdfCanvasError::InvalidDashPattern(
                "dash phase must be finite and non-negative".into(),
            ));
        }

        let normalized_intervals = normalize_intervals(intervals);

        Ok(Some(Self {
            intervals: normalized_intervals,
            phase,
        }))
    }

    /// Returns a pattern scaled into the same coordinate space as the stroked path.
    pub fn scaled(&self, scale: f32) -> Self {
        Self {
            intervals: self
                .intervals
                .iter()
                .map(|interval| interval * scale)
                .collect(),
            phase: self.phase * scale,
        }
    }
}

fn normalize_intervals(intervals: &[f32]) -> Vec<f32> {
    let mut normalized = intervals.to_vec();
    if intervals.len() % 2 == 1 {
        normalized.extend(intervals.iter().copied());
    }
    normalized
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_dash_array_is_solid() {
        let pattern = DashPattern::new(&[], 4.0).expect("empty dash array should be valid");

        assert_eq!(pattern, None);
    }

    #[test]
    fn odd_dash_array_is_repeated() {
        let pattern = DashPattern::new(&[3.0, 1.0, 2.0], 1.0)
            .expect("dash pattern should be valid")
            .expect("dash pattern should be present");

        assert_eq!(pattern.intervals, vec![3.0, 1.0, 2.0, 3.0, 1.0, 2.0]);
        assert_eq!(pattern.phase, 1.0);
    }

    #[test]
    fn all_zero_dash_array_is_invalid() {
        let error =
            DashPattern::new(&[0.0, 0.0], 0.0).expect_err("all-zero dash pattern should fail");

        assert!(matches!(error, PdfCanvasError::InvalidDashPattern(_)));
    }

    #[test]
    fn negative_dash_values_are_invalid() {
        let error =
            DashPattern::new(&[2.0, -1.0], 0.0).expect_err("negative dash interval should fail");

        assert!(matches!(error, PdfCanvasError::InvalidDashPattern(_)));
    }
}

/// Stroke-specific rendering metadata passed to canvas backends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeStyle {
    /// Optional dash pattern. `None` means a solid stroke.
    pub dash_pattern: Option<DashPattern>,
}

impl StrokeStyle {
    /// Returns a stroke style scaled into the same coordinate space as the stroked path.
    pub fn scaled(&self, scale: f32) -> Self {
        Self {
            dash_pattern: self
                .dash_pattern
                .as_ref()
                .map(|dash_pattern| dash_pattern.scaled(scale)),
        }
    }
}
