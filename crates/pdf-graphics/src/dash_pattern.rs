use thiserror::Error;

/// Errors that can occur while validating a PDF dash pattern.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DashPatternError {
    #[error("dash intervals must be finite and non-negative")]
    InvalidInterval,
    #[error("dash pattern must contain at least one positive interval")]
    AllZeroIntervals,
    #[error("dash phase must be finite and non-negative")]
    InvalidPhase,
}

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
    pub fn new(intervals: &[f32], phase: f32) -> Result<Option<Self>, DashPatternError> {
        if intervals.is_empty() {
            return Ok(None);
        }

        let mut has_positive_interval = false;
        for interval in intervals {
            if !interval.is_finite() || *interval < 0.0 {
                return Err(DashPatternError::InvalidInterval);
            }
            if *interval > 0.0 {
                has_positive_interval = true;
            }
        }

        if !has_positive_interval {
            return Err(DashPatternError::AllZeroIntervals);
        }

        if !phase.is_finite() || phase < 0.0 {
            return Err(DashPatternError::InvalidPhase);
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

        assert_eq!(error, DashPatternError::AllZeroIntervals);
    }

    #[test]
    fn negative_dash_values_are_invalid() {
        let error =
            DashPattern::new(&[2.0, -1.0], 0.0).expect_err("negative dash interval should fail");

        assert_eq!(error, DashPatternError::InvalidInterval);
    }

    #[test]
    fn non_finite_phase_is_invalid() {
        let error = DashPattern::new(&[2.0, 1.0], f32::INFINITY)
            .expect_err("non-finite dash phase should fail");

        assert_eq!(error, DashPatternError::InvalidPhase);
    }

    #[test]
    fn scaled_pattern_scales_normalized_intervals_and_phase() {
        let pattern = DashPattern::new(&[3.0, 1.0, 2.0], 1.5)
            .expect("dash pattern should be valid")
            .expect("dash pattern should be present");

        let scaled = pattern.scaled(2.0);

        assert_eq!(scaled.intervals, vec![6.0, 2.0, 4.0, 6.0, 2.0, 4.0]);
        assert_eq!(scaled.phase, 3.0);
    }
}
