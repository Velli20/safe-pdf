//! Geometry-specific reconstruction for Coons and tensor-product patches.

use pdf_graphics::{color::Color, point::Point};

use crate::{
    error::PdfShadingError,
    model::{MeshPatch, ShadingType},
    patch_mesh::PatchMeshError,
};

const COONS_CONTROL_POINT_COUNT: usize = 12;
const TENSOR_CONTROL_POINT_COUNT: usize = 16;

/// Geometry layout selected by a patch mesh's `/ShadingType`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PatchKind {
    Coons,
    Tensor,
}

impl PatchKind {
    /// Every PDF patch assigns a decoded color to each of its four corners.
    pub(crate) const CORNER_COLOR_COUNT: usize = 4;

    /// Converts a shading type into its corresponding patch layout.
    pub(crate) fn from_shading_type(shading_type: ShadingType) -> Result<Self, PdfShadingError> {
        match shading_type {
            ShadingType::CoonsPatchMesh => Ok(Self::Coons),
            ShadingType::TensorProductPatchMesh => Ok(Self::Tensor),
            _ => Err(PatchMeshError::UnsupportedShadingType { shading_type }.into()),
        }
    }

    /// Returns the number of control points in one complete patch.
    pub(crate) const fn control_point_count(self) -> usize {
        match self {
            Self::Coons => COONS_CONTROL_POINT_COUNT,
            Self::Tensor => TENSOR_CONTROL_POINT_COUNT,
        }
    }

    /// Returns the control points inherited from the preceding patch.
    ///
    /// Flag zero starts an independent patch. Flags one through three select a
    /// boundary of the preceding patch, preserving the point order required for
    /// the new patch's first edge.
    pub(crate) fn initial_control_points(
        self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<Vec<Point>, PdfShadingError> {
        if flag == 0 {
            return Ok(Vec::with_capacity(self.control_point_count()));
        }

        match self {
            Self::Coons => coons_shared_points(flag, previous),
            Self::Tensor => tensor_shared_points(flag, previous),
        }
    }

    /// Returns the two corner colors inherited with a shared edge.
    pub(crate) fn initial_corner_colors(
        self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<Vec<Color>, PdfShadingError> {
        if flag == 0 {
            return Ok(Vec::with_capacity(Self::CORNER_COLOR_COUNT));
        }

        let colors = self.previous_corner_colors(flag, previous)?;
        Ok(Vec::from(shared_colors(flag, *colors, self.name())?))
    }

    /// Converts decoded vectors into the fixed-size public patch model.
    pub(crate) fn build_patch(
        self,
        control_points: Vec<Point>,
        corner_colors: Vec<Color>,
    ) -> Result<MeshPatch, PdfShadingError> {
        match self {
            Self::Coons => Ok(MeshPatch::Coons {
                control_points: try_array(control_points, "12 Coons control points")?,
                corner_colors: try_array(corner_colors, "4 Coons corner colors")?,
            }),
            Self::Tensor => Ok(MeshPatch::Tensor {
                control_points: try_array(control_points, "16 tensor control points")?,
                corner_colors: try_array(corner_colors, "4 tensor corner colors")?,
            }),
        }
    }

    /// Finds corner colors on a compatible preceding patch.
    fn previous_corner_colors(
        self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<&[Color; Self::CORNER_COLOR_COUNT], PdfShadingError> {
        match (self, previous) {
            (Self::Coons, Some(MeshPatch::Coons { corner_colors, .. }))
            | (Self::Tensor, Some(MeshPatch::Tensor { corner_colors, .. })) => Ok(corner_colors),
            _ => Err(PatchMeshError::ContinuationWithoutPreviousPatch {
                kind: self.name(),
                flag,
            }
            .into()),
        }
    }

    /// Returns the human-readable patch name used in parser errors.
    const fn name(self) -> &'static str {
        match self {
            Self::Coons => "Coons",
            Self::Tensor => "Tensor",
        }
    }
}

/// Selects a shared Coons boundary from its twelve perimeter control points.
fn coons_shared_points(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Point>, PdfShadingError> {
    let Some(MeshPatch::Coons { control_points, .. }) = previous else {
        return Err(PatchMeshError::ContinuationWithoutPreviousPatch {
            kind: "Coons",
            flag,
        }
        .into());
    };
    let &[p0, _p1, _p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = control_points;
    let shared = match flag {
        1 => [p3, p4, p5, p6],
        2 => [p6, p7, p8, p9],
        3 => [p9, p10, p11, p0],
        _ => {
            return Err(PatchMeshError::InvalidContinuationFlag {
                kind: "Coons",
                flag,
            }
            .into());
        }
    };
    Ok(Vec::from(shared))
}

/// Selects a shared tensor boundary from its 4x4 control-point grid.
fn tensor_shared_points(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Point>, PdfShadingError> {
    let Some(MeshPatch::Tensor { control_points, .. }) = previous else {
        return Err(PatchMeshError::ContinuationWithoutPreviousPatch {
            kind: "Tensor",
            flag,
        }
        .into());
    };
    let &[
        p0,
        _p1,
        _p2,
        p3,
        p4,
        _p5,
        _p6,
        p7,
        p8,
        _p9,
        _p10,
        p11,
        p12,
        p13,
        p14,
        p15,
    ] = control_points;
    let shared = match flag {
        1 => [p3, p7, p11, p15],
        2 => [p15, p14, p13, p12],
        3 => [p12, p8, p4, p0],
        _ => {
            return Err(PatchMeshError::InvalidContinuationFlag {
                kind: "Tensor",
                flag,
            }
            .into());
        }
    };
    Ok(Vec::from(shared))
}

/// Maps a shared edge to the corresponding pair of corner colors.
fn shared_colors(
    flag: u8,
    [c0, c1, c2, c3]: [Color; PatchKind::CORNER_COLOR_COUNT],
    kind: &'static str,
) -> Result<[Color; 2], PdfShadingError> {
    match flag {
        1 => Ok([c1, c2]),
        2 => Ok([c2, c3]),
        3 => Ok([c3, c0]),
        _ => Err(PatchMeshError::InvalidContinuationFlag { kind, flag }.into()),
    }
}

/// Converts decoded values into the fixed array required by the public model.
fn try_array<T, const N: usize>(
    values: Vec<T>,
    expected: &'static str,
) -> Result<[T; N], PdfShadingError> {
    values
        .try_into()
        .map_err(|_| PdfShadingError::from(PatchMeshError::IncompletePatch { expected }))
}

#[cfg(test)]
mod tests {
    use pdf_graphics::{color::Color, point::Point};

    use super::PatchKind;
    use crate::model::MeshPatch;

    #[test]
    fn coons_continuations_reuse_the_selected_boundary() {
        let points = coons_points();
        let patch = MeshPatch::Coons {
            control_points: points,
            corner_colors: corner_colors(),
        };

        for (flag, expected) in [
            (1, [points[3], points[4], points[5], points[6]]),
            (2, [points[6], points[7], points[8], points[9]]),
            (3, [points[9], points[10], points[11], points[0]]),
        ] {
            let actual = PatchKind::Coons
                .initial_control_points(flag, Some(&patch))
                .expect("valid Coons continuation should be reconstructed");
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn tensor_continuations_reuse_the_selected_boundary() {
        let points = tensor_points();
        let patch = MeshPatch::Tensor {
            control_points: points,
            corner_colors: corner_colors(),
        };

        for (flag, expected) in [
            (1, [points[3], points[7], points[11], points[15]]),
            (2, [points[15], points[14], points[13], points[12]]),
            (3, [points[12], points[8], points[4], points[0]]),
        ] {
            let actual = PatchKind::Tensor
                .initial_control_points(flag, Some(&patch))
                .expect("valid tensor continuation should be reconstructed");
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn continuations_reuse_the_shared_edge_colors() {
        let colors = corner_colors();
        let patch = MeshPatch::Coons {
            control_points: coons_points(),
            corner_colors: colors,
        };

        for (flag, expected) in [
            (1, [colors[1], colors[2]]),
            (2, [colors[2], colors[3]]),
            (3, [colors[3], colors[0]]),
        ] {
            let actual = PatchKind::Coons
                .initial_corner_colors(flag, Some(&patch))
                .expect("valid continuation colors should be reconstructed");
            assert_eq!(actual, expected);
        }
    }

    fn coons_points() -> [Point; 12] {
        [
            point(0.0),
            point(1.0),
            point(2.0),
            point(3.0),
            point(4.0),
            point(5.0),
            point(6.0),
            point(7.0),
            point(8.0),
            point(9.0),
            point(10.0),
            point(11.0),
        ]
    }

    fn tensor_points() -> [Point; 16] {
        [
            point(0.0),
            point(1.0),
            point(2.0),
            point(3.0),
            point(4.0),
            point(5.0),
            point(6.0),
            point(7.0),
            point(8.0),
            point(9.0),
            point(10.0),
            point(11.0),
            point(12.0),
            point(13.0),
            point(14.0),
            point(15.0),
        ]
    }

    fn corner_colors() -> [Color; PatchKind::CORNER_COLOR_COUNT] {
        [
            Color::from_rgb(0.0, 0.0, 0.0),
            Color::from_rgb(0.25, 0.25, 0.25),
            Color::from_rgb(0.5, 0.5, 0.5),
            Color::from_rgb(0.75, 0.75, 0.75),
        ]
    }

    fn point(value: f32) -> Point {
        Point::new(value, value)
    }
}
