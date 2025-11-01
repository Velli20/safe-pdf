use crate::rect::Rect;

/// An affine transformation matrix.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Transform {
    pub sx: f32,
    pub kx: f32,
    pub ky: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            sx: 1.0,
            kx: 0.0,
            ky: 0.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

impl Transform {
    /// Creates an identity transform.
    pub fn identity() -> Self {
        Transform::default()
    }

    /// We are using column-major-column-vector matrix notation, therefore it's ky-kx, not kx-ky.
    pub const fn from_row(sx: f32, ky: f32, kx: f32, sy: f32, tx: f32, ty: f32) -> Self {
        Transform {
            sx,
            ky,
            kx,
            sy,
            tx,
            ty,
        }
    }

    pub fn from_translate(tx: f32, ty: f32) -> Self {
        Transform::from_row(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    pub fn from_scale(sx: f32, sy: f32) -> Self {
        Transform::from_row(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// Applies a translation to this transform.
    ///
    /// The translation `(tx, ty)` is applied by pre-multiplying the current transform
    /// matrix with a translation matrix. This means that the translation is effectively
    /// applied *before* the existing transformation components.
    ///
    /// If the current transform is `M` and the translation operation is `Op_translate(tx, ty)`,
    /// the new transform becomes `Op_translate * M`.
    ///
    /// # Arguments
    ///
    /// * `tx`: The translation amount along the x-axis.
    /// * `ty`: The translation amount along the y-axis.
    ///
    /// # Returns
    ///
    /// A mutable reference to `self` for chaining.
    pub fn translate(&mut self, tx: f32, ty: f32) -> &mut Self {
        // Current matrix M:
        // [ self.sx  self.kx  self.tx ]
        // [ self.ky  self.sy  self.ty ]
        // [    0        0       1    ]
        //
        // Translation matrix Op_translate(dtx, dty):
        // [ 1  0  dtx ]
        // [ 0  1  dty ]
        // [ 0  0   1  ]
        //
        // New matrix M' = Op_translate * M
        // M'.tx = 1*self.tx + 0*self.ty + dtx*1 = self.tx + dtx
        // M'.ty = 0*self.tx + 1*self.ty + dty*1 = self.ty + dty
        // Other components (sx, kx, ky, sy) remain unchanged by this pre-multiplication.
        self.tx += tx;
        self.ty += ty;
        self
    }

    /// Applies a scaling to this transform.
    ///
    /// The scaling by factors `(sx, sy)` is applied by pre-multiplying the current
    /// transform matrix with a scaling matrix. This means that the scaling is effectively
    /// applied *before* the existing transformation components.
    ///
    /// If the current transform is `M` and the scaling operation is `Op_scale(sx, sy)`,
    /// the new transform becomes `Op_scale * M`.
    ///
    /// # Arguments
    ///
    /// * `sx`: The scaling factor along the x-axis.
    /// * `sy`: The scaling factor along the y-axis.
    ///
    /// # Returns
    ///
    /// A mutable reference to `self` for chaining.
    pub fn scale(&mut self, sx: f32, sy: f32) -> &mut Self {
        // M' = Op_scale * M
        // All components of the first row of M are scaled by sx.
        // All components of the second row of M are scaled by sy.
        self.sx *= sx;
        self.kx *= sx;
        self.tx *= sx;

        self.sy *= sy;
        self.ky *= sy;
        self.ty *= sy;
        self
    }

    /// Applies a rotation (in radians) to this transform by pre-multiplying
    /// with a rotation matrix R(theta).
    ///
    /// Using column-vector notation, the rotation matrix is:
    /// [ cosθ  -sinθ  0 ]
    /// [ sinθ   cosθ  0 ]
    /// [  0      0    1 ]
    ///
    /// This updates self as: self <- R(theta) * self
    ///
    /// # Arguments
    /// * `radians`: Rotation angle in radians (counter-clockwise).
    ///
    /// # Returns
    /// A mutable reference to `self` for chaining.
    pub fn rotate(&mut self, radians: f32) -> &mut Self {
        let c = radians.cos();
        let s = radians.sin();

        // Preserve current components
        let s_sx = self.sx;
        let s_kx = self.kx;
        let s_tx = self.tx;
        let s_ky = self.ky;
        let s_sy = self.sy;
        let s_ty = self.ty;

        // Pre-multiply by rotation: S_new = R * S
        self.sx = c * s_sx - s * s_ky;
        self.kx = c * s_kx - s * s_sy;
        self.tx = c * s_tx - s * s_ty;

        self.ky = s * s_sx + c * s_ky;
        self.sy = s * s_kx + c * s_sy;
        self.ty = s * s_tx + c * s_ty;

        self
    }

    /// Pre-multiplies this transform by another `Transform`.
    ///
    /// If the current transform is `M_self` and the `other` transform is `M_other`,
    /// this operation updates `M_self` to `M_other * M_self`.
    ///
    /// This means that the transformation represented by `M_other` is applied,
    /// and then the original transformation `M_self` is applied to that result.
    /// When transforming a point `P`, the new transformation acts as `(M_other * M_self) * P`.
    ///
    /// # Arguments
    ///
    /// * `other`: The `Transform` to pre-multiply with.
    ///
    /// # Returns
    ///
    /// A mutable reference to `self` for chaining.
    pub fn concat(&mut self, other: &Transform) -> &mut Self {
        // Let current transform be S (self) and the matrix to concatenate be O (other).
        // We want to compute S_new = O * S.
        // S = [ sx  kx  tx ]
        //     [ ky  sy  ty ]
        //     [ 0   0   1  ]
        //
        // O = [ o.sx  o.kx  o.tx ]
        //     [ o.ky  o.sy  o.ty ]
        //     [  0     0     1   ]
        //
        // S_new.sx = o.sx * s.sx + o.kx * s.ky
        // S_new.kx = o.sx * s.kx + o.kx * s.sy
        // S_new.tx = o.sx * s.tx + o.kx * s.ty + o.tx
        //
        // S_new.ky = o.ky * s.sx + o.sy * s.ky
        // S_new.sy = o.ky * s.kx + o.sy * s.sy
        // S_new.ty = o.ky * s.tx + o.sy * s.ty + o.ty

        let s_sx = self.sx;
        let s_kx = self.kx;
        let s_tx = self.tx;
        let s_ky = self.ky;
        let s_sy = self.sy;
        let s_ty = self.ty;

        self.sx = other.sx * s_sx + other.kx * s_ky;
        self.kx = other.sx * s_kx + other.kx * s_sy;
        self.tx = other.sx * s_tx + other.kx * s_ty + other.tx;

        self.ky = other.ky * s_sx + other.sy * s_ky;
        self.sy = other.ky * s_kx + other.sy * s_sy;
        self.ty = other.ky * s_tx + other.sy * s_ty + other.ty;

        self
    }

    /// Post-multiplies this transform by another `Transform` (self = self * other).
    ///
    /// This applies the existing transformation first, then the `other` transformation.
    /// Useful when updating a current transform by appending an operation in the
    /// coordinate space of the current transform (e.g., PDF text matrix updates).
    pub fn post_concat(&mut self, other: &Transform) -> &mut Self {
        // Let current transform be S (self) and the matrix to concatenate be O (other).
        // We want to compute S_new = S * O.
        // S = [ sx  kx  tx ]
        //     [ ky  sy  ty ]
        //     [ 0   0   1  ]
        //
        // O = [ o.sx  o.kx  o.tx ]
        //     [ o.ky  o.sy  o.ty ]
        //     [  0     0     1   ]
        //
        // S_new.sx = sx * o.sx + kx * o.ky
        // S_new.kx = sx * o.kx + kx * o.sy
        // S_new.tx = sx * o.tx + kx * o.ty + tx
        //
        // S_new.ky = ky * o.sx + sy * o.ky
        // S_new.sy = ky * o.kx + sy * o.sy
        // S_new.ty = ky * o.tx + sy * o.ty + ty

        let s_sx = self.sx;
        let s_kx = self.kx;
        let s_tx = self.tx;
        let s_ky = self.ky;
        let s_sy = self.sy;
        let s_ty = self.ty;

        self.sx = s_sx * other.sx + s_kx * other.ky;
        self.kx = s_sx * other.kx + s_kx * other.sy;
        self.tx = s_sx * other.tx + s_kx * other.ty + s_tx;

        self.ky = s_ky * other.sx + s_sy * other.ky;
        self.sy = s_ky * other.kx + s_sy * other.sy;
        self.ty = s_ky * other.tx + s_sy * other.ty + s_ty;

        self
    }

    /// Post-multiplies this transform by a translation (self = self * T(tx, ty)).
    ///
    /// Unlike `translate`, which pre-multiplies, this applies the translation in the
    /// local space of the current transform. For a current transform S, the new
    /// translation components become:
    ///   tx' = sx*tx + kx*ty + tx
    ///   ty' = ky*tx + sy*ty + ty
    pub fn post_translate(&mut self, tx: f32, ty: f32) -> &mut Self {
        let new_tx = self.sx * tx + self.kx * ty + self.tx;
        let new_ty = self.ky * tx + self.sy * ty + self.ty;
        self.tx = new_tx;
        self.ty = new_ty;
        self
    }

    /// Post-multiplies this transform by a rotation R(theta) (self = self * R(theta)).
    ///
    /// Using column-vector notation, the rotation matrix is:
    /// [ cosθ  -sinθ  0 ]
    /// [ sinθ   cosθ  0 ]
    /// [  0      0    1 ]
    ///
    /// This applies the existing transformation first, then the rotation in the
    /// local space of the current transform. The translation (tx, ty) remains unchanged.
    ///
    /// Updates are:
    ///   sx' =  sx*cosθ + kx*sinθ
    ///   kx' = -sx*sinθ + kx*cosθ
    ///   ky' =  ky*cosθ + sy*sinθ
    ///   sy' = -ky*sinθ + sy*cosθ
    pub fn post_rotate(&mut self, radians: f32) -> &mut Self {
        let c = radians.cos();
        let s = radians.sin();

        let s_sx = self.sx;
        let s_kx = self.kx;
        let s_ky = self.ky;
        let s_sy = self.sy;

        self.sx = s_sx * c + s_kx * s;
        self.kx = -s_sx * s + s_kx * c;

        self.ky = s_ky * c + s_sy * s;
        self.sy = -s_ky * s + s_sy * c;

        // tx and ty remain unchanged when post-multiplying by a pure rotation.
        self
    }

    /// Transforms a 2D point `(x, y)` using this transform.
    ///
    /// The transformation is applied as follows:
    /// `x_new = self.sx * x + self.kx * y + self.tx`
    /// `y_new = self.ky * x + self.sy * y + self.ty`
    ///
    /// # Arguments
    ///
    /// * `x`: The x-coordinate of the point to transform.
    /// * `y`: The y-coordinate of the point to transform.
    ///
    /// # Returns
    ///
    /// A tuple `(f32, f32)` representing the transformed point `(x_new, y_new)`.
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let new_x = self.sx * x + self.kx * y + self.tx;
        let new_y = self.ky * x + self.sy * y + self.ty;
        (new_x, new_y)
    }

    /// Transforms a rectangle using this transform.
    ///
    /// # Arguments
    ///
    /// * `rect`: The rectangle to transform.
    ///
    /// # Returns
    ///
    /// A new `Rect` representing the transformed rectangle.
    pub fn map_rect(&self, rect: &Rect) -> Rect {
        let (x0, y0) = self.transform_point(rect.left, rect.top);
        let (x1, y1) = self.transform_point(rect.right, rect.top);
        let (x2, y2) = self.transform_point(rect.left, rect.bottom);
        let (x3, y3) = self.transform_point(rect.right, rect.bottom);

        let min_x = x0.min(x1).min(x2).min(x3);
        let max_x = x0.max(x1).max(x2).max(x3);
        let min_y = y0.min(y1).min(y2).min(y3);
        let max_y = y0.max(y1).max(y2).max(y3);

        Rect {
            left: min_x,
            top: min_y,
            right: max_x,
            bottom: max_y,
        }
    }

    /// Returns the rotation angle (in degrees, counter-clockwise) encoded by the
    /// linear part of this affine transform.
    ///
    /// The transform is represented as the 3x3 matrix using column-vectors:
    /// [ sx  kx  tx ]
    /// [ ky  sy  ty ]
    /// [  0   0   1 ]
    ///
    /// The rotation is extracted from the orientation of the transformed X-axis,
    /// i.e., the first column vector `(sx, ky)`. This is robust against uniform
    /// or non-uniform scaling and shear. If the first column is degenerate
    /// (zero length), it falls back to using the second column `(kx, sy)` with
    /// `atan2(-kx, sy)`. If both columns are degenerate, it returns `0.0`.
    pub fn rotation_degrees(&self) -> f32 {
        let x_len = (self.sx * self.sx + self.ky * self.ky).sqrt();
        if x_len > 0.0 {
            self.ky.atan2(self.sx).to_degrees()
        } else {
            let y_len = (self.kx * self.kx + self.sy * self.sy).sqrt();
            if y_len > 0.0 {
                (-self.kx).atan2(self.sy).to_degrees()
            } else {
                // Degenerate linear transform: no meaningful rotation.
                0.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn rotation_degrees_identity_is_zero() {
        let t = Transform::identity();
        assert!(approx_eq(t.rotation_degrees(), 0.0, 1e-4));
    }

    #[test]
    fn rotation_degrees_ninety_ccw() {
        let mut t = Transform::identity();
        t.rotate(std::f32::consts::FRAC_PI_2);
        let angle = t.rotation_degrees();
        assert!(approx_eq(angle, 90.0, 1e-3));
    }

    #[test]
    fn rotation_degrees_with_scaling() {
        let mut t = Transform::identity();
        // Apply some scaling and rotation; extraction should still be ~45 deg.
        t.scale(2.0, 3.0);
        t.rotate(std::f32::consts::FRAC_PI_4);
        let angle = t.rotation_degrees();
        assert!(approx_eq(angle, 45.0, 1e-2));
    }
}
