//! Interfaz dieléctrica lisa: Fresnel exacto (no polarizado), refracción de Snell
//! y absorción de Beer–Lambert para el volumen.

use glam::Vec3;

/// Reflectancia de Fresnel de un dieléctrico. `cos_i` es el coseno del ángulo de
/// incidencia (en [0, 1]); `n1` el índice de donde viene la luz y `n2` el de destino.
/// Devuelve 1 en reflexión total interna.
pub fn fresnel_dielectric(cos_i: f32, n1: f32, n2: f32) -> f32 {
    let cos_i = cos_i.clamp(0.0, 1.0);
    let eta = n1 / n2;
    let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
    if sin2_t >= 1.0 {
        return 1.0;
    }
    let cos_t = (1.0 - sin2_t).sqrt();
    let rs = (n1 * cos_i - n2 * cos_t) / (n1 * cos_i + n2 * cos_t);
    let rp = (n1 * cos_t - n2 * cos_i) / (n1 * cos_t + n2 * cos_i);
    0.5 * (rs * rs + rp * rp)
}

/// Dirección refractada. `d` es la dirección del rayo (unitaria), `n` la normal unitaria
/// del lado desde donde llega el rayo (`d · n < 0`), `eta = n1 / n2`.
/// `None` si hay reflexión total interna.
pub fn refract(d: Vec3, n: Vec3, eta: f32) -> Option<Vec3> {
    let cos_i = (-d).dot(n).clamp(0.0, 1.0);
    let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
    if sin2_t >= 1.0 {
        return None;
    }
    let cos_t = (1.0 - sin2_t).sqrt();
    Some((d * eta + n * (eta * cos_i - cos_t)).normalize())
}

pub fn reflect(d: Vec3, n: Vec3) -> Vec3 {
    d - n * (2.0 * d.dot(n))
}

/// Coeficiente de extinción por unidad de longitud para que, tras recorrer `distance`,
/// la luz quede multiplicada por `color` (Beer–Lambert).
pub fn absorption_sigma(color: Vec3, distance: f32) -> Vec3 {
    let c = color.clamp(Vec3::splat(1e-3), Vec3::ONE);
    Vec3::new(-c.x.ln(), -c.y.ln(), -c.z.ln()) / distance.max(1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresnel_incidencia_normal() {
        let f = fresnel_dielectric(1.0, 1.0, 1.5);
        assert!((f - 0.04).abs() < 1e-5, "{f}");
        // Simétrico al salir
        assert!((fresnel_dielectric(1.0, 1.5, 1.0) - 0.04).abs() < 1e-5);
    }

    #[test]
    fn fresnel_rasante_tiende_a_uno() {
        assert!(fresnel_dielectric(0.001, 1.0, 1.5) > 0.95);
    }

    #[test]
    fn reflexion_total_interna_pasado_el_angulo_critico() {
        let crit = (1.0f32 / 1.5).asin();
        assert_eq!(fresnel_dielectric((crit + 0.05).cos(), 1.5, 1.0), 1.0);
        assert!(fresnel_dielectric((crit - 0.05).cos(), 1.5, 1.0) < 1.0);
        let d = Vec3::new((crit + 0.05).sin(), -(crit + 0.05).cos(), 0.0);
        assert!(refract(d, Vec3::Y, 1.5).is_none());
    }

    #[test]
    fn ley_de_snell() {
        for deg in [10.0f32, 30.0, 55.0, 80.0] {
            let a = deg.to_radians();
            let d = Vec3::new(a.sin(), -a.cos(), 0.0);
            let t = refract(d, Vec3::Y, 1.0 / 1.5).unwrap();
            let sin_t = t.x.abs();
            assert!((a.sin() - 1.5 * sin_t).abs() < 1e-5, "{deg}°");
            assert!(t.y < 0.0);
        }
    }

    #[test]
    fn beer_lambert_deja_el_color_a_la_distancia() {
        let c = Vec3::new(0.5, 0.25, 1.0);
        let s = absorption_sigma(c, 2.0);
        let t = Vec3::new((-s.x * 2.0).exp(), (-s.y * 2.0).exp(), (-s.z * 2.0).exp());
        assert!((t - c).abs().max_element() < 1e-5);
    }
}
