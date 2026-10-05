//! Vectores y planos mínimos (arreglos simples para que serialicen directo).

use serde::{Deserialize, Serialize};

pub type P2 = [f64; 2];
pub use cad_occt::P3;

pub fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

pub fn normalize(a: P3) -> P3 {
    let n = norm(a);
    if n < 1e-15 { a } else { scale(a, 1.0 / n) }
}

pub fn dist2(a: P2, b: P2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Plano con sistema de coordenadas: los sketches viven en (x, y) local.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub origin: P3,
    pub normal: P3,
    pub x_dir: P3,
}

impl Plane {
    /// Planta (mira desde arriba).
    pub const XY: Plane = Plane { origin: [0.0; 3], normal: [0.0, 0.0, 1.0], x_dir: [1.0, 0.0, 0.0] };
    /// Frente: x → X, y → Z (normal −Y, hacia quien mira de frente).
    pub const XZ: Plane = Plane { origin: [0.0; 3], normal: [0.0, -1.0, 0.0], x_dir: [1.0, 0.0, 0.0] };
    /// Lateral derecho: x → Y, y → Z (normal +X).
    pub const YZ: Plane = Plane { origin: [0.0; 3], normal: [1.0, 0.0, 0.0], x_dir: [0.0, 1.0, 0.0] };

    /// Plano por un punto con esa normal; el eje x sale de proyectar X del
    /// mundo (o Y si la normal es casi X), así es estable al mover el plano.
    pub fn from_normal(origin: P3, normal: P3) -> Plane {
        let n = normalize(normal);
        let world = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let x = normalize(sub(world, scale(n, dot(world, n))));
        Plane { origin, normal: n, x_dir: x }
    }

    pub fn y_dir(&self) -> P3 {
        cross(self.normal, self.x_dir)
    }

    pub fn to_world(&self, p: P2) -> P3 {
        add(self.origin, add(scale(self.x_dir, p[0]), scale(self.y_dir(), p[1])))
    }

    pub fn to_local(&self, p: P3) -> P2 {
        let d = sub(p, self.origin);
        [dot(d, self.x_dir), dot(d, self.y_dir())]
    }

    pub fn offset(&self, distance: f64) -> Plane {
        Plane { origin: add(self.origin, scale(self.normal, distance)), ..*self }
    }

    pub fn flipped(&self) -> Plane {
        Plane { normal: scale(self.normal, -1.0), x_dir: self.x_dir, origin: self.origin }
    }
}
