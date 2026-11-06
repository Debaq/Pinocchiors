//! Pieza de calibración de exposición para resina (SLA5 de `PLAN_SLA.md`).
//!
//! Una base de 46 × 24 × 1,5 mm con cuatro hileras, cada una de izquierda a derecha de
//! más fino a más grueso:
//! - **pines** (lo fino tiene que quedar de pie: con poca exposición se caen),
//! - **agujeros** en un bloque (con mucha exposición se cierran),
//! - **ranuras** que parten un bloque (con mucha exposición se pegan),
//! - **paredes** finas (con poca exposición no salen).
//!
//! La misma pieza se lamina con varias exposiciones (un archivo por cada una); la buena es
//! la más baja con la que salen los pines y paredes finas sin que se cierren agujeros y
//! ranuras. Se imprime directo sobre la placa, sin soportes.

use serde::Serialize;

use crate::sla_supports::{frustum, Tri};

/// Una hilera: qué mide y los tamaños, de izquierda a derecha (mm).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationRow {
    pub name: &'static str,
    pub sizes: Vec<f32>,
}

pub const PIN_DIAMETERS: [f32; 8] = [0.3, 0.4, 0.5, 0.6, 0.8, 1.0, 1.5, 2.0];
pub const HOLE_DIAMETERS: [f32; 8] = [0.4, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0, 2.5];
pub const SLOT_WIDTHS: [f32; 8] = [0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.75, 1.0];
pub const WALL_THICKNESS: [f32; 8] = [0.1, 0.2, 0.3, 0.4, 0.6, 0.8, 1.0, 1.5];

/// Tamaño de la pieza (mm).
pub const SIZE: [f32; 3] = [46.0, 24.0, 6.5];

/// Las hileras, de adelante (y chica) hacia atrás.
pub fn rows() -> Vec<CalibrationRow> {
    vec![
        CalibrationRow { name: "Pines", sizes: PIN_DIAMETERS.to_vec() },
        CalibrationRow { name: "Agujeros", sizes: HOLE_DIAMETERS.to_vec() },
        CalibrationRow { name: "Ranuras", sizes: SLOT_WIDTHS.to_vec() },
        CalibrationRow { name: "Paredes", sizes: WALL_THICKNESS.to_vec() },
    ]
}

/// La pieza, con su esquina en el origen y apoyada en z = 0.
pub fn calibration_piece() -> Vec<Tri> {
    const BASE: f32 = 1.5;
    const H: f32 = 5.0;
    let top = BASE + H;
    let x = |i: usize| 4.5 + i as f32 * 5.3;
    let mut t = Vec::new();
    cuboid(&mut t, [0.0, 0.0, 0.0], [SIZE[0], SIZE[1], BASE], false);

    // Pines (y = 3)
    for (i, d) in PIN_DIAMETERS.iter().enumerate() {
        frustum(&mut t, [x(i), 3.0, BASE - 0.1], d / 2.0, [x(i), 3.0, top], d / 2.0, 16);
    }
    // Bloque con agujeros (y 6–10): cilindros dados vuelta restan
    cuboid(&mut t, [2.0, 6.0, BASE - 0.1], [44.0, 10.0, top], false);
    for (i, d) in HOLE_DIAMETERS.iter().enumerate() {
        let mut c = Vec::new();
        frustum(&mut c, [x(i), 8.0, BASE + 0.5], d / 2.0, [x(i), 8.0, top], d / 2.0, 24);
        t.extend(c.into_iter().map(|t| [t[0], t[2], t[1]]));
    }
    // Bloque con ranuras (y 12–16): cajas dadas vuelta que lo cruzan en y
    cuboid(&mut t, [2.0, 12.0, BASE - 0.1], [44.0, 16.0, top], false);
    for (i, w) in SLOT_WIDTHS.iter().enumerate() {
        cuboid(&mut t, [x(i) - w / 2.0, 11.5, BASE + 0.5], [x(i) + w / 2.0, 16.5, top], true);
    }
    // Paredes (y 18–22)
    for (i, w) in WALL_THICKNESS.iter().enumerate() {
        cuboid(&mut t, [x(i) - w / 2.0, 18.0, BASE - 0.1], [x(i) + w / 2.0, 22.0, top], false);
    }
    t
}

/// Caja entre `a` y `b`; `inverted` la da vuelta (resta al laminar).
fn cuboid(out: &mut Vec<Tri>, a: [f32; 3], b: [f32; 3], inverted: bool) {
    let p = |i: usize| [if i & 1 == 0 { a[0] } else { b[0] }, if i & 2 == 0 { a[1] } else { b[1] }, if i & 4 == 0 { a[2] } else { b[2] }];
    for q in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            let t = [p(tri[0]), p(tri[1]), p(tri[2])];
            out.push(if inverted { [t[0], t[2], t[1]] } else { t });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::{slice, LD_002H};
    use crate::sla_check::check;

    #[test]
    fn la_pieza_se_lamina_con_sus_detalles() {
        let mut t = calibration_piece();
        for v in t.iter_mut().flatten() {
            v[0] += 18.0;
            v[1] += 50.0;
        }
        let layers = slice(&t, &LD_002H, 0.05).unwrap();
        assert_eq!(layers.len(), 130);
        let r = check(&layers, &LD_002H, 0.05);
        assert!(r.islands.is_empty(), "{:?}", r.islands);
        // A media altura de los detalles: los 8 pines, el bloque partido en 9 por las ranuras
        // (la de 0,1 mm es de 2 px y queda abierta), 8 paredes y los agujeros.
        let mid = &layers[80];
        let (_, ph) = LD_002H.pixel_size();
        let row_at = |y: f32| ((y + 50.0) / ph) as u32;
        let count = |y: f32| mid.spans.iter().filter(|s| s.y == row_at(y)).count();
        assert_eq!(count(3.0), 8, "pines");
        assert_eq!(count(14.0), 9, "ranuras");
        assert_eq!(count(20.0), 8, "paredes");
        // Fila por el centro de los agujeros: el bloque con 8 agujeros = 9 tramos
        assert_eq!(count(8.0), 9, "agujeros");
    }
}
