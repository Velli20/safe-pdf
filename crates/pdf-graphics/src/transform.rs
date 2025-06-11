/// An affine transformation matrix.
#[derive(Copy, Clone, PartialEq, Debug)]
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
}
