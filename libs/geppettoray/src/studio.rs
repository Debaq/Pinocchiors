//! Fondo de estudio (ciclorama): un piso que sube en curva hasta un muro, sin costuras, bajo el
//! modelo y detrás de él. Es lo que usan los fotógrafos para fotos de producto: el piso y el fondo
//! son una sola superficie que recibe luz y sombras de verdad.

use crate::scene::{RayMaterial, Triangle};
use glam::{Vec3, Vec4};

#[derive(Debug, Clone, Copy)]
pub struct Studio {
    /// Color (lineal) del ciclorama
    pub color: Vec3,
    /// Punto del piso bajo el centro del modelo
    pub floor_center: Vec3,
    /// Hacia dónde está el muro, horizontal y unitario (detrás del modelo, visto desde la cámara)
    pub toward_wall: Vec3,
    /// Tamaño del modelo (su diagonal): todo el set se mide con él
    pub size: f32,
    /// 0 = mate; 1 = piso brillante que refleja al modelo
    pub gloss: f32,
}

impl Studio {
    /// Material del ciclorama
    pub fn material(&self) -> RayMaterial {
        RayMaterial {
            base_color: self.color.extend(1.0),
            metallic: 0.0,
            roughness: 1.0 - 0.8 * self.gloss.clamp(0.0, 1.0),
            lambert: self.gloss <= 0.0,
            // Una sola cara: desde abajo (cámara bajo el piso) el rayo lo atraviesa
            double_sided: false,
            casts_shadow: false,
            ..Default::default()
        }
    }

    /// Triángulos del ciclorama con el material `material`. Las caras miran hacia el interior.
    pub fn triangles(&self, material: u32) -> Vec<Triangle> {
        let up = Vec3::Y;
        let back = {
            let h = Vec3::new(self.toward_wall.x, 0.0, self.toward_wall.z);
            if h.length() > 1e-4 { h.normalize() } else { -Vec3::Z }
        };
        let side = up.cross(back).normalize();
        let s = self.size.max(1e-3);
        // Medidas: piso hacia la cámara y hacia el muro, ancho, radio de la curva y alto del muro
        let (front, behind, half_w, radius, height) = (7.0 * s, 2.5 * s, 9.0 * s, 0.9 * s, 7.0 * s);
        let o = self.floor_center;
        // Perfil en el plano (hacia atrás, arriba): del frente del piso, por la curva, hasta lo alto del muro
        let mut profile: Vec<(f32, f32, Vec3)> = vec![(-front, 0.0, up)];
        let steps = 16;
        for i in 0..=steps {
            let th = std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
            // Centro de la curva en (behind, radius); la normal apunta hacia ese centro
            let (sn, cs) = th.sin_cos();
            profile.push((behind + radius * sn, radius * (1.0 - cs), (up * cs - back * sn).normalize()));
        }
        profile.push((behind + radius, height, -back));
        let at = |b: f32, u: f32, x: f32| o + back * b + up * u + side * x;
        let mut out = Vec::new();
        for w in profile.windows(2) {
            let ((b0, u0, n0), (b1, u1, n1)) = (w[0], w[1]);
            let (a, b, c, d) = (at(b0, u0, -half_w), at(b0, u0, half_w), at(b1, u1, half_w), at(b1, u1, -half_w));
            // Orienta cada triángulo para que su cara frontal mire al interior (la normal del perfil)
            for (p, n) in [([a, b, c], [n0, n0, n1]), ([a, c, d], [n0, n1, n1])] {
                let want = n[0] + n[1] + n[2];
                let g = (p[1] - p[0]).cross(p[2] - p[0]);
                let tri = if g.dot(want) >= 0.0 { p } else { [p[0], p[2], p[1]] };
                let nn = if g.dot(want) >= 0.0 { n } else { [n[0], n[2], n[1]] };
                let mut t = Triangle::new(tri, Some(nn), None, material);
                t.color = [Vec4::ONE; 3];
                out.push(t);
            }
        }
        out
    }
}
