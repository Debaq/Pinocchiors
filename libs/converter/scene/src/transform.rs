use glam::{Mat4, Quat, Vec3};

/// Transform de un nodo — TRS o matrix.
#[derive(Debug, Clone, Copy)]
pub enum Transform {
    /// Translation, Rotation, Scale por separado.
    Trs {
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
    },
    /// Matrix 4x4 directa.
    Matrix(Mat4),
}

impl Transform {
    pub fn identity() -> Self {
        Self::Trs {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn to_matrix(self) -> Mat4 {
        match self {
            Self::Trs {
                translation,
                rotation,
                scale,
            } => Mat4::from_scale_rotation_translation(scale, rotation, translation),
            Self::Matrix(m) => m,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
