/// A closed numeric interval `[min, max]`.
///
/// Models the per-dimension pairs of PDF `/Domain`, `/Range`, `/Encode` and `/Decode`
/// arrays. Values come straight from documents, so no ordering between `min` and `max`
/// is assumed and none of the methods panic on inverted or non-finite bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    /// The lower bound.
    pub min: f32,
    /// The upper bound.
    pub max: f32,
}

impl Interval {
    /// The unit interval `[0, 1]`.
    pub const UNIT: Self = Self { min: 0.0, max: 1.0 };

    /// Creates an interval from its bounds.
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    /// Groups a flat `[min0 max0 min1 max1 ...]` array into intervals.
    ///
    /// A trailing unpaired value is ignored.
    pub fn from_pairs(values: &[f32]) -> Vec<Self> {
        values
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(Self::from)
            .collect()
    }

    /// Returns `max - min`.
    pub fn span(&self) -> f32 {
        self.max - self.min
    }

    /// Returns `true` when both bounds are finite and `min < max`.
    pub fn is_increasing(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min < self.max
    }

    /// Clamps `x` into the interval.
    ///
    /// Unlike [`f32::clamp`], this never panics: an inverted interval yields `max`, and a
    /// NaN `x` yields `min`.
    pub fn clamp(&self, x: f32) -> f32 {
        x.max(self.min).min(self.max)
    }

    /// Clamps `x` into the interval and returns its position in `[0, 1]`.
    ///
    /// Returns `None` unless the interval [`is_increasing`](Self::is_increasing).
    pub fn normalize(&self, x: f32) -> Option<f32> {
        if !self.is_increasing() {
            return None;
        }
        Some((self.clamp(x) - self.min) / self.span())
    }

    /// Returns the point at fraction `t` from `min` towards `max`.
    pub fn lerp(&self, t: f32) -> f32 {
        self.min + t * self.span()
    }

    /// Maps `x` from this interval onto the same relative position in `to`.
    ///
    /// A zero-width source interval maps every `x` to `to.min`.
    pub fn remap(&self, x: f32, to: &Interval) -> f32 {
        let span = self.span();
        let t = if span.abs() < f32::EPSILON {
            0.0
        } else {
            (x - self.min) / span
        };
        to.lerp(t)
    }
}

impl From<[f32; 2]> for Interval {
    fn from([min, max]: [f32; 2]) -> Self {
        Self { min, max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_pairs_groups_values_and_drops_trailing_value() {
        let intervals = Interval::from_pairs(&[0.0, 1.0, -2.0, 4.0, 9.0]);

        assert_eq!(
            intervals,
            vec![Interval::new(0.0, 1.0), Interval::new(-2.0, 4.0)]
        );
        assert!(Interval::from_pairs(&[]).is_empty());
        assert!(Interval::from_pairs(&[3.0]).is_empty());
    }

    #[test]
    fn is_increasing_rejects_degenerate_inverted_and_nonfinite_bounds() {
        assert!(Interval::new(-1.0, 1.0).is_increasing());
        assert!(!Interval::new(2.0, 2.0).is_increasing());
        assert!(!Interval::new(1.0, 0.0).is_increasing());
        assert!(!Interval::new(0.0, f32::INFINITY).is_increasing());
        assert!(!Interval::new(f32::NAN, 1.0).is_increasing());
    }

    #[test]
    fn clamp_limits_values_to_bounds() {
        let interval = Interval::new(2.0, 6.0);

        assert_eq!(interval.clamp(-10.0), 2.0);
        assert_eq!(interval.clamp(4.0), 4.0);
        assert_eq!(interval.clamp(10.0), 6.0);
    }

    #[test]
    fn clamp_does_not_panic_on_inverted_bounds_or_nan() {
        // `f32::clamp` panics for both of these cases.
        assert_eq!(Interval::new(1.0, 0.0).clamp(0.5), 0.0);
        assert_eq!(Interval::UNIT.clamp(f32::NAN), 0.0);
        assert_eq!(Interval::new(f32::NAN, 1.0).clamp(0.5), 0.5);
    }

    #[test]
    fn normalize_maps_clamped_value_into_unit_interval() {
        let interval = Interval::new(2.0, 6.0);

        assert_eq!(interval.normalize(2.0), Some(0.0));
        assert_eq!(interval.normalize(3.0), Some(0.25));
        assert_eq!(interval.normalize(6.0), Some(1.0));
        assert_eq!(interval.normalize(-5.0), Some(0.0));
        assert_eq!(interval.normalize(100.0), Some(1.0));
    }

    #[test]
    fn normalize_rejects_intervals_that_are_not_increasing() {
        assert_eq!(Interval::new(3.0, 3.0).normalize(3.0), None);
        assert_eq!(Interval::new(1.0, 0.0).normalize(0.5), None);
        assert_eq!(Interval::new(0.0, f32::INFINITY).normalize(1.0), None);
    }

    #[test]
    fn lerp_interpolates_between_bounds_and_follows_inversion() {
        let interval = Interval::new(2.0, 6.0);

        assert_eq!(interval.lerp(0.0), 2.0);
        assert_eq!(interval.lerp(0.25), 3.0);
        assert_eq!(interval.lerp(1.0), 6.0);
        assert_eq!(Interval::new(1.0, 0.0).lerp(0.25), 0.75);
    }

    #[test]
    fn remap_preserves_relative_position_including_inverted_targets() {
        let source = Interval::new(0.0, 4.0);

        assert_eq!(source.remap(1.0, &Interval::new(10.0, 18.0)), 12.0);
        assert_eq!(source.remap(1.0, &Interval::new(1.0, 0.0)), 0.75);
        // Remap does not clamp: values outside the source extrapolate.
        assert_eq!(source.remap(8.0, &Interval::UNIT), 2.0);
    }

    #[test]
    fn remap_from_zero_width_interval_yields_target_min() {
        let point = Interval::new(5.0, 5.0);

        assert_eq!(point.remap(5.0, &Interval::new(3.0, 9.0)), 3.0);
        assert_eq!(point.remap(-100.0, &Interval::new(3.0, 9.0)), 3.0);
    }
}
