use crate::{
    color::Color, point::Point, rect::Rect, transform::Transform, BoundsAccumulator,
};

const DEFAULT_SUBDIVISION: usize = 8;
const MAX_SUBDIVISION: f32 = 32.0;

/// A borrowed patch mesh view that keeps `pdf-graphics` independent from parser crates.
#[derive(Debug, Clone, Copy)]
pub enum MeshPatchRef<'a> {
    Coons {
        control_points: &'a [Point; 12],
        corner_colors: &'a [Color; 4],
    },
    Tensor {
        control_points: &'a [Point; 16],
        corner_colors: &'a [Color; 4],
    },
}

/// A sampled mesh patch vertex with interpolated position and color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    pub point: Point,
    pub color: Color,
}

impl<'a> MeshPatchRef<'a> {
    pub(crate) fn control_points(&self) -> &'a [Point] {
        match self {
            Self::Coons { control_points, .. } => control_points.as_slice(),
            Self::Tensor { control_points, .. } => control_points.as_slice(),
        }
    }

    pub(crate) fn evaluate(self, u: f32, v: f32) -> MeshVertex {
        match self {
            Self::Coons {
                control_points,
                corner_colors,
            } => evaluate_coons_patch_vertex(control_points, corner_colors, u, v),
            Self::Tensor {
                control_points,
                corner_colors,
            } => evaluate_tensor_patch_vertex(control_points, corner_colors, u, v),
        }
    }
}

/// Evaluates a Coons patch at the `(u, v)` parametric coordinate.
pub fn evaluate_coons_patch_vertex(
    control_points: &[Point; 12],
    corner_colors: &[Color; 4],
    u: f32,
    v: f32,
) -> MeshVertex {
    let top = evaluate_cubic_bezier(
        [
            control_points[0],
            control_points[1],
            control_points[2],
            control_points[3],
        ],
        u,
    );
    let right = evaluate_cubic_bezier(
        [
            control_points[3],
            control_points[4],
            control_points[5],
            control_points[6],
        ],
        v,
    );
    let bottom = evaluate_cubic_bezier(
        [
            control_points[9],
            control_points[8],
            control_points[7],
            control_points[6],
        ],
        u,
    );
    let left = evaluate_cubic_bezier(
        [
            control_points[0],
            control_points[11],
            control_points[10],
            control_points[9],
        ],
        v,
    );

    let bilinear = bilinear_point(
        control_points[0],
        control_points[3],
        control_points[6],
        control_points[9],
        u,
        v,
    );

    MeshVertex {
        point: Point::new(
            (1.0 - v) * top.x + v * bottom.x + (1.0 - u) * left.x + u * right.x - bilinear.x,
            (1.0 - v) * top.y + v * bottom.y + (1.0 - u) * left.y + u * right.y - bilinear.y,
        ),
        color: bilinear_color(corner_colors, u, v),
    }
}

/// Evaluates a tensor-product patch at the `(u, v)` parametric coordinate.
pub fn evaluate_tensor_patch_vertex(
    control_points: &[Point; 16],
    corner_colors: &[Color; 4],
    u: f32,
    v: f32,
) -> MeshVertex {
    let u_basis = bernstein_basis(u);
    let v_basis = bernstein_basis(v);
    let mut x = 0.0;
    let mut y = 0.0;

    for (row, v_weight) in control_points.chunks_exact(4).zip(v_basis) {
        for (&point, u_weight) in row.iter().zip(u_basis) {
            let weight = u_weight * v_weight;
            x += point.x * weight;
            y += point.y * weight;
        }
    }

    MeshVertex {
        point: Point::new(x, y),
        color: bilinear_color(corner_colors, u, v),
    }
}

/// Computes device-space bounds for a sequence of patches after applying `transform`.
pub fn patch_mesh_bounds<'a, I>(patches: I, transform: &Transform) -> Option<Rect>
where
    I: IntoIterator<Item = MeshPatchRef<'a>>,
{
    let mut bounds = BoundsAccumulator::new();

    for patch in patches {
        for point in patch.control_points() {
            bounds.include(transformed_point(*point, transform));
        }
    }

    bounds.finish()
}

/// Chooses a tessellation subdivision count for a single patch in device space.
pub fn patch_subdivision(patch: MeshPatchRef<'_>, transform: &Transform) -> usize {
    let Some(bounds) = patch_mesh_bounds(std::iter::once(patch), transform) else {
        return DEFAULT_SUBDIVISION;
    };

    let normalized = bounds.normalized();
    normalized
        .width()
        .max(normalized.height())
        .clamp(DEFAULT_SUBDIVISION as f32, MAX_SUBDIVISION)
        .round() as usize
}

/// Tessellates a patch into device-space triangles.
pub fn tessellate_patch(
    patch: MeshPatchRef<'_>,
    transform: &Transform,
    subdivision: usize,
) -> Vec<[MeshVertex; 3]> {
    let steps = subdivision.max(1);
    let rows = build_patch_rows(patch, transform, steps);
    let mut triangles = Vec::with_capacity(steps.saturating_mul(steps).saturating_mul(2));

    for row_pair in rows.windows(2) {
        let top_row = &row_pair[0];
        let bottom_row = &row_pair[1];

        for (top_pair, bottom_pair) in top_row.windows(2).zip(bottom_row.windows(2)) {
            let top_left = top_pair[0];
            let top_right = top_pair[1];
            let bottom_left = bottom_pair[0];
            let bottom_right = bottom_pair[1];

            triangles.push([top_left, top_right, bottom_left]);
            triangles.push([top_right, bottom_right, bottom_left]);
        }
    }

    triangles
}

pub(crate) fn transformed_point(point: Point, transform: &Transform) -> Point {
    let (x, y) = transform.transform_point(point.x, point.y);
    Point::new(x, y)
}

fn bernstein_basis(t: f32) -> [f32; 4] {
    let one_minus_t = 1.0 - t;
    [
        one_minus_t * one_minus_t * one_minus_t,
        3.0 * t * one_minus_t * one_minus_t,
        3.0 * t * t * one_minus_t,
        t * t * t,
    ]
}

fn evaluate_cubic_bezier(points: [Point; 4], t: f32) -> Point {
    let basis = bernstein_basis(t);
    let (x, y) = points
        .into_iter()
        .zip(basis)
        .fold((0.0, 0.0), |(x, y), (point, weight)| {
            (x + point.x * weight, y + point.y * weight)
        });

    Point::new(x, y)
}

fn bilinear_point(
    top_left: Point,
    top_right: Point,
    bottom_right: Point,
    bottom_left: Point,
    u: f32,
    v: f32,
) -> Point {
    let one_minus_u = 1.0 - u;
    let one_minus_v = 1.0 - v;

    Point::new(
        one_minus_u * one_minus_v * top_left.x
            + u * one_minus_v * top_right.x
            + u * v * bottom_right.x
            + one_minus_u * v * bottom_left.x,
        one_minus_u * one_minus_v * top_left.y
            + u * one_minus_v * top_right.y
            + u * v * bottom_right.y
            + one_minus_u * v * bottom_left.y,
    )
}

fn bilinear_color(corner_colors: &[Color; 4], u: f32, v: f32) -> Color {
    let one_minus_u = 1.0 - u;
    let one_minus_v = 1.0 - v;
    let [top_left, top_right, bottom_right, bottom_left] = *corner_colors;

    Color::from_rgba(
        one_minus_u * one_minus_v * top_left.r
            + u * one_minus_v * top_right.r
            + u * v * bottom_right.r
            + one_minus_u * v * bottom_left.r,
        one_minus_u * one_minus_v * top_left.g
            + u * one_minus_v * top_right.g
            + u * v * bottom_right.g
            + one_minus_u * v * bottom_left.g,
        one_minus_u * one_minus_v * top_left.b
            + u * one_minus_v * top_right.b
            + u * v * bottom_right.b
            + one_minus_u * v * bottom_left.b,
        one_minus_u * one_minus_v * top_left.a
            + u * one_minus_v * top_right.a
            + u * v * bottom_right.a
            + one_minus_u * v * bottom_left.a,
    )
}

fn build_patch_rows(
    patch: MeshPatchRef<'_>,
    transform: &Transform,
    steps: usize,
) -> Vec<Vec<MeshVertex>> {
    let mut rows = Vec::with_capacity(steps.saturating_add(1));

    for row in 0..=steps {
        let v = row as f32 / steps as f32;
        let mut vertices = Vec::with_capacity(steps.saturating_add(1));

        for column in 0..=steps {
            let u = column as f32 / steps as f32;
            let vertex = patch.evaluate(u, v);
            vertices.push(transform_mesh_vertex(vertex, transform));
        }

        rows.push(vertices);
    }

    rows
}

fn transform_mesh_vertex(vertex: MeshVertex, transform: &Transform) -> MeshVertex {
    MeshVertex {
        point: transformed_point(vertex.point, transform),
        color: vertex.color,
    }
}
