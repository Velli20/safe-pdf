//! Geometry-specific reconstruction for Coons and tensor-product patches.

use pdf_graphics::{color::Color, point::Point};

use crate::{
    error::PdfShadingError,
    model::{MeshPatch, ShadingType},
    patch_mesh::PatchMeshError,
};

const COONS_CONTROL_POINT_COUNT: usize = 12;
const TENSOR_CONTROL_POINT_COUNT: usize = 16;

/// Geometry layout of a Type 6 or Type 7 patch mesh.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PatchKind {
    Coons,
    Tensor,
}

/// The boundary side of the preceding patch that a continuation patch starts from.
///
/// A patch boundary runs through four sides; the first side of a continuation
/// patch is always the second, third, or fourth side of its predecessor.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SharedEdge {
    /// Flag 1: the side from corner 1 to corner 2.
    Second,
    /// Flag 2: the side from corner 2 to corner 3.
    Third,
    /// Flag 3: the side from corner 3 back to corner 0.
    Fourth,
}

impl PatchKind {
    /// Every PDF patch assigns a decoded color to each of its four corners.
    pub(crate) const CORNER_COLOR_COUNT: usize = 4;

    /// Returns the `/ShadingType` that selects this layout.
    pub(crate) const fn shading_type(self) -> ShadingType {
        match self {
            Self::Coons => ShadingType::CoonsPatchMesh,
            Self::Tensor => ShadingType::TensorProductPatchMesh,
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
    /// An independent patch inherits nothing. A continuation inherits the four
    /// points of the shared edge, ordered as the new patch's first side.
    pub(crate) fn initial_control_points(
        self,
        edge: Option<SharedEdge>,
        previous: Option<&MeshPatch>,
    ) -> Result<Vec<Point>, PdfShadingError> {
        let Some(edge) = edge else {
            return Ok(Vec::with_capacity(self.control_point_count()));
        };
        let shared = match (self, previous) {
            (Self::Coons, Some(MeshPatch::Coons { control_points, .. })) => {
                edge.coons_points(control_points)
            }
            (Self::Tensor, Some(MeshPatch::Tensor { control_points, .. })) => {
                edge.tensor_points(control_points)
            }
            _ => {
                return Err(PatchMeshError::ContinuationWithoutPreviousPatch {
                    kind: self.name(),
                    flag: edge.flag(),
                }
                .into());
            }
        };
        Ok(Vec::from(shared))
    }

    /// Returns the two corner colors inherited with a shared edge.
    pub(crate) fn initial_corner_colors(
        self,
        edge: Option<SharedEdge>,
        previous: Option<&MeshPatch>,
    ) -> Result<Vec<Color>, PdfShadingError> {
        let Some(edge) = edge else {
            return Ok(Vec::with_capacity(Self::CORNER_COLOR_COUNT));
        };
        let colors = match (self, previous) {
            (Self::Coons, Some(MeshPatch::Coons { corner_colors, .. }))
            | (Self::Tensor, Some(MeshPatch::Tensor { corner_colors, .. })) => corner_colors,
            _ => {
                return Err(PatchMeshError::ContinuationWithoutPreviousPatch {
                    kind: self.name(),
                    flag: edge.flag(),
                }
                .into());
            }
        };
        Ok(Vec::from(edge.colors(*colors)))
    }

    /// Converts decoded vectors into the fixed-size public patch model.
    ///
    /// Tensor points arrive in stream order, which walks the boundary
    /// before the four interior points; they are reordered into the
    /// row-major 4x4 net the patch model stores.
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
            Self::Tensor => {
                let [
                    p00,
                    p01,
                    p02,
                    p03,
                    p13,
                    p23,
                    p33,
                    p32,
                    p31,
                    p30,
                    p20,
                    p10,
                    p11,
                    p12,
                    p22,
                    p21,
                ] = try_array(control_points, "16 tensor control points")?;
                Ok(MeshPatch::Tensor {
                    control_points: [
                        p00, p01, p02, p03, p10, p11, p12, p13, p20, p21, p22, p23, p30, p31, p32,
                        p33,
                    ],
                    corner_colors: try_array(corner_colors, "4 tensor corner colors")?,
                })
            }
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

impl SharedEdge {
    /// Decodes a patch flag; flag 0 starts an independent patch.
    ///
    /// Only the two low bits of a flag carry meaning.
    pub(crate) const fn from_flag(flag: u8) -> Option<Self> {
        match flag & 0b11 {
            0 => None,
            1 => Some(Self::Second),
            2 => Some(Self::Third),
            _ => Some(Self::Fourth),
        }
    }

    /// Returns the flag that selects this edge.
    const fn flag(self) -> u8 {
        match self {
            Self::Second => 1,
            Self::Third => 2,
            Self::Fourth => 3,
        }
    }

    /// Selects this edge from the twelve perimeter points of a Coons patch.
    fn coons_points(self, points: &[Point; COONS_CONTROL_POINT_COUNT]) -> [Point; 4] {
        let &[p0, _p1, _p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = points;
        match self {
            Self::Second => [p3, p4, p5, p6],
            Self::Third => [p6, p7, p8, p9],
            Self::Fourth => [p9, p10, p11, p0],
        }
    }

    /// Selects this edge from the row-major 4x4 net of a tensor patch.
    fn tensor_points(self, points: &[Point; TENSOR_CONTROL_POINT_COUNT]) -> [Point; 4] {
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
        ] = points;
        match self {
            Self::Second => [p3, p7, p11, p15],
            Self::Third => [p15, p14, p13, p12],
            Self::Fourth => [p12, p8, p4, p0],
        }
    }

    /// Selects the two corner colors at the ends of this edge.
    fn colors(self, [c0, c1, c2, c3]: [Color; PatchKind::CORNER_COLOR_COUNT]) -> [Color; 2] {
        match self {
            Self::Second => [c1, c2],
            Self::Third => [c2, c3],
            Self::Fourth => [c3, c0],
        }
    }
}

/// Converts decoded values into the fixed array required by the public model.
fn try_array<T, const N: usize>(
    values: Vec<T>,
    expected: &'static str,
) -> Result<[T; N], PdfShadingError> {
    Ok(values
        .try_into()
        .map_err(|_| PatchMeshError::IncompletePatch { expected })?)
}

#[cfg(test)]
mod tests {
    use pdf_graphics::{color::Color, point::Point};

    use super::{PatchKind, SharedEdge};
    use crate::model::MeshPatch;

    #[test]
    fn coons_continuations_reuse_the_selected_boundary() {
        let points = coons_points();
        let patch = MeshPatch::Coons {
            control_points: points,
            corner_colors: corner_colors(),
        };

        for (edge, expected) in [
            (
                SharedEdge::Second,
                [points[3], points[4], points[5], points[6]],
            ),
            (
                SharedEdge::Third,
                [points[6], points[7], points[8], points[9]],
            ),
            (
                SharedEdge::Fourth,
                [points[9], points[10], points[11], points[0]],
            ),
        ] {
            let actual = PatchKind::Coons
                .initial_control_points(Some(edge), Some(&patch))
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

        for (edge, expected) in [
            (
                SharedEdge::Second,
                [points[3], points[7], points[11], points[15]],
            ),
            (
                SharedEdge::Third,
                [points[15], points[14], points[13], points[12]],
            ),
            (
                SharedEdge::Fourth,
                [points[12], points[8], points[4], points[0]],
            ),
        ] {
            let actual = PatchKind::Tensor
                .initial_control_points(Some(edge), Some(&patch))
                .expect("valid tensor continuation should be reconstructed");
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn tensor_stream_order_is_reordered_into_a_row_major_net() {
        let points = tensor_points();
        let patch = PatchKind::Tensor
            .build_patch(Vec::from(points), Vec::from(corner_colors()))
            .expect("complete tensor patch should build");
        let control_points = match patch {
            MeshPatch::Tensor { control_points, .. } => Some(control_points),
            MeshPatch::Coons { .. } => None,
        };

        let expected = [
            points[0], points[1], points[2], points[3], points[11], points[12], points[13],
            points[4], points[10], points[15], points[14], points[5], points[9], points[8],
            points[7], points[6],
        ];
        assert_eq!(control_points, Some(expected));
    }

    #[test]
    fn continuations_reuse_the_shared_edge_colors() {
        let colors = corner_colors();
        let patch = MeshPatch::Coons {
            control_points: coons_points(),
            corner_colors: colors,
        };

        for (edge, expected) in [
            (SharedEdge::Second, [colors[1], colors[2]]),
            (SharedEdge::Third, [colors[2], colors[3]]),
            (SharedEdge::Fourth, [colors[3], colors[0]]),
        ] {
            let actual = PatchKind::Coons
                .initial_corner_colors(Some(edge), Some(&patch))
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
