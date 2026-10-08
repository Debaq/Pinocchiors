//! Roscas del diseño y su coordinación ([`ThreadLink`]).
//!
//! Cada rosca calculada (agujero roscado, operación Rosca, tornillo, tuerca)
//! deja un [`ThreadAxis`]: su medida, su hélice y su boca. Dos roscas calzan
//! cuando comparten la hélice: el mismo eje, paso y mano, y el filete del macho
//! pasando por el hueco de la hembra. Todas las hélices se describen igual (el
//! filete del macho, o el del macho que talla la hembra): pasa por
//! `origin + r·x` y avanza un paso por vuelta hacia `dir`.

use serde::{Deserialize, Serialize};

use crate::feature::{FeatureId, ThreadLink, ThreadSpec};
use crate::geom::{P3, add, cross, dot, norm, normalize, scale, sub};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadAxis {
    pub feature: FeatureId,
    /// Cuál de las roscas de la operación (un agujero tiene una por centro)
    pub index: u32,
    pub spec: ThreadSpec,
    /// Hembra (agujero, tuerca) o macho (eje, tornillo)
    pub internal: bool,
    /// Con el filete de verdad o solo como dato (cosmética)
    pub modeled: bool,
    /// Hélice: el filete pasa por `origin + r·x` y avanza hacia `dir`
    pub origin: P3,
    pub dir: P3,
    pub x: P3,
    /// Boca: por donde entra la otra pieza (sobre el eje) y hacia afuera
    pub mouth: P3,
    pub out: P3,
    /// Largo de la rosca desde la boca hacia adentro (contra `out`)
    pub length: f64,
    /// Agujero ciego: un tornillo no puede pasar del fondo
    #[serde(default)]
    pub blind: bool,
    /// Marco en que quedó un tornillo o una tuerca (origen, Z, X)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placed: Option<[P3; 3]>,
    /// La rosca que sigue
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadLink>,
}

impl ThreadAxis {
    /// Boca y salida desde las que entra una pieza (la del otro extremo con `flip`).
    pub fn entry(&self, flip: bool) -> (P3, P3) {
        if flip { (sub(self.mouth, scale(self.out, self.length)), scale(self.out, -1.0)) } else { (self.mouth, self.out) }
    }

    /// Distancia del punto `p` al eje de la rosca.
    pub fn distance_to_axis(&self, p: P3) -> f64 {
        let d = normalize(self.dir);
        let v = sub(p, self.origin);
        norm(sub(v, scale(d, dot(v, d))))
    }

    /// ¿Otra rosca de eje `origin`/`dir` comparte este eje (para alinear el
    /// filete)? Paralela y a menos de un cuarto del diámetro.
    pub fn coaxial(&self, origin: P3, dir: P3) -> bool {
        dot(normalize(dir), normalize(self.dir)).abs() > 0.999 && self.distance_to_axis(origin) < 0.25 * self.spec.nominal.max(0.4)
    }

    /// X para una hélice que arranca en `origin` hacia `dir` (sobre este eje)
    /// y sigue a esta: la misma curva, corrida y girada. Con el eje al revés la
    /// mano no cambia y la fórmula es la misma (ver la prueba).
    pub fn phase_x(&self, origin: P3, dir: P3) -> P3 {
        let z = normalize(self.dir);
        let x0 = normalize(sub(self.x, scale(z, dot(self.x, z))));
        let y0 = cross(z, x0);
        let s = dot(sub(origin, self.origin), z);
        let turn = 2.0 * std::f64::consts::PI * s / self.spec.pitch;
        let a = if self.spec.left { -turn } else { turn };
        let x = add(scale(x0, a.cos()), scale(y0, a.sin()));
        // Ya es perpendicular a `dir` (paralelo a `z`); se limpia el error
        let d = normalize(dir);
        normalize(sub(x, scale(d, dot(x, d))))
    }
}

/// X por defecto de una hélice alrededor de `dir`.
pub fn default_x(dir: P3) -> P3 {
    let d = normalize(dir);
    let a = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    normalize(sub(a, scale(d, dot(a, d))))
}

/// Medida métrica de una rosca ("M6", "M8×1", "M6 izq.").
pub fn spec_name(s: &ThreadSpec) -> String {
    let d = if (s.nominal - s.nominal.round()).abs() < 1e-9 { format!("{}", s.nominal.round()) } else { format!("{}", s.nominal) };
    let coarse = crate::standard::size_by_nominal(s.nominal).is_some_and(|m| (m.pitch - s.pitch).abs() < 1e-9);
    let mut name = if coarse { format!("M{d}") } else { format!("M{d}×{}", s.pitch) };
    if s.left {
        name.push_str(" izq.");
    }
    name
}
