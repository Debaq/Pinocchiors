//! Engranes: geometría de la evolvente contra las fórmulas.

use approx::assert_relative_eq;
use cad_model::gears::*;
use std::f64::consts::{PI, TAU};

fn spur(z: u32) -> GearSpec {
    GearSpec { teeth: z, ..Default::default() }
}

fn polar(p: [f64; 2]) -> (f64, f64) {
    (p[0].hypot(p[1]), p[1].atan2(p[0]).rem_euclid(TAU))
}

fn area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

#[test]
fn circulos_estandar() {
    let c = spur(20).circles().unwrap();
    assert_relative_eq!(c.pitch, 10.0);
    assert_relative_eq!(c.base, 10.0 * 20f64.to_radians().cos(), epsilon = 1e-12);
    assert_relative_eq!(c.tip, 11.0);
    assert_relative_eq!(c.root, 8.75);
    assert_relative_eq!(c.thickness, PI / 2.0, epsilon = 1e-12);
    let c = GearSpec { shift: 0.5, teeth: 20, ..Default::default() }.circles().unwrap();
    assert_relative_eq!(c.tip, 11.5);
    assert_relative_eq!(c.root, 9.25);
    assert_relative_eq!(c.thickness, PI / 2.0 + 2.0 * 0.5 * 20f64.to_radians().tan(), epsilon = 1e-12);
}

#[test]
fn paso_angular_y_simetria() {
    let g = spur(24).profile().unwrap();
    let tau = TAU / 24.0;
    let n = g.points.len() / 24;
    for i in 0..n {
        let (a, b) = (polar(g.points[i]), polar(g.points[i + n]));
        assert_relative_eq!(a.0, b.0, epsilon = 1e-9);
        assert_relative_eq!((b.1 - a.1).rem_euclid(TAU), tau, epsilon = 1e-9);
    }
    // Simetría del diente: espejo respecto al centro del diente (τ/2)
    let mut per = g.points[..n].to_vec();
    per.push(g.points[n]);
    for &p in &per {
        let (r, a) = polar(p);
        let q = [r * (tau - a).cos(), r * (tau - a).sin()];
        let near = per.iter().map(|&o| (o[0] - q[0]).hypot(o[1] - q[1])).fold(f64::MAX, f64::min);
        assert!(near < 0.06, "sin simétrico cerca de {p:?}: {near}");
    }
}

#[test]
fn espesor_en_el_primitivo() {
    for (z, x) in [(20u32, 0.0), (30, 0.3), (45, -0.2)] {
        let s = GearSpec { teeth: z, shift: x, ..Default::default() };
        let c = s.circles().unwrap();
        let g = s.profile().unwrap();
        // Cruce del flanco con el círculo primitivo, en el primer diente
        let n = g.points.len() / z as usize;
        let per = &g.points[..n];
        let mut cuts = vec![];
        for w in per.windows(2) {
            let (a, b) = (polar(w[0]), polar(w[1]));
            if (a.0 - c.pitch) * (b.0 - c.pitch) < 0.0 {
                let t = (c.pitch - a.0) / (b.0 - a.0);
                cuts.push(a.1 + (b.1 - a.1) * t);
            }
        }
        assert_eq!(cuts.len(), 2, "z={z}");
        // Entre ambos cruces está el diente: espesor de arco = Δθ·r
        let thick = (cuts[1] - cuts[0]) * c.pitch;
        assert_relative_eq!(thick, c.thickness, epsilon = 2e-3);
    }
}

#[test]
fn juego_adelgaza_el_diente() {
    let a = spur(24).profile().unwrap();
    let b = GearSpec { backlash: 0.1, ..spur(24) }.profile().unwrap();
    assert!(area(&b.points) < area(&a.points));
    assert_relative_eq!(b.circles.thickness, a.circles.thickness - 0.05, epsilon = 1e-12);
}

#[test]
fn flanco_es_evolvente_de_la_base() {
    let s = spur(30);
    let g = s.profile().unwrap();
    let rb = g.circles.base;
    // Puntos del flanco por encima de involute_start: θ = θ0 + inv(acos(rb/r)) en ±
    let n = g.points.len() / 30;
    let flank: Vec<(f64, f64)> = g.points[..n].iter().map(|&p| polar(p)).filter(|p| p.0 > g.involute_start + 1e-9 && p.0 < g.tip_radius - 1e-9).take_while(|p| p.1 < PI / 30.0).collect();
    assert!(flank.len() > 10);
    let inv = |r: f64| { let a = (rb / r).acos(); a.tan() - a };
    // Misma constante: θ − inv(α_r) no cambia a lo largo del flanco
        for (r, th) in &flank {
        assert_relative_eq!(*th - inv(*r) - (flank[0].1 - inv(flank[0].0)), 0.0, epsilon = 1e-9);
    }
}

#[test]
fn socavado_con_pocos_dientes() {
    let g = spur(8).profile().unwrap();
    assert!(g.warnings.iter().any(|w| matches!(w, GearWarning::Undercut { .. })), "{:?}", g.warnings);
    assert!(g.involute_start >= g.circles.base);
    let g = spur(40).profile().unwrap();
    assert!(!g.warnings.iter().any(|w| matches!(w, GearWarning::Undercut { .. })), "{:?}", g.warnings);
    // La corrección de perfil quita el socavado
    let g = GearSpec { shift: 0.6, ..spur(12) }.profile().unwrap();
    assert!(!g.warnings.iter().any(|w| matches!(w, GearWarning::Undercut { .. })), "{:?}", g.warnings);
    assert_relative_eq!(spur(20).min_teeth_no_undercut(), 2.0 * (1.25 - 0.25 * (1.0 - 20f64.to_radians().sin())) / 20f64.to_radians().sin().powi(2));
}

#[test]
fn pie_sin_socavado_es_tangente() {
    // Sin socavado el trocoide enlaza con la evolvente sin saltos
    let g = spur(40).profile().unwrap();
    let n = g.points.len() / 40;
    let per = &g.points[..n];
    let max_step = per.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).fold(0.0, f64::max);
    assert!(max_step < 0.5, "salto de {max_step}");
}

#[test]
fn diente_en_punta_se_recorta() {
    let g = GearSpec { shift: 0.9, ..spur(12) }.profile().unwrap();
    assert!(g.warnings.iter().any(|w| matches!(w, GearWarning::Pointed)), "{:?}", g.warnings);
    assert!(g.tip_radius < g.circles.tip);
}

#[test]
fn par_estandar() {
    let p = pair(&spur(18), &spur(40)).unwrap();
    assert_relative_eq!(p.center_distance, 29.0, epsilon = 1e-9);
    assert_relative_eq!(p.working_angle, 20f64.to_radians(), epsilon = 1e-9);
    assert_relative_eq!(p.ratio, 40.0 / 18.0, epsilon = 1e-12);
    assert!(p.contact_ratio > 1.2, "{}", p.contact_ratio);
    assert!(!p.interference);
    // Distancia mayor con corrección positiva en los dos
    let q = pair(&GearSpec { shift: 0.3, ..spur(18) }, &GearSpec { shift: 0.3, ..spur(40) }).unwrap();
    assert!(q.center_distance > p.center_distance);
    assert!(q.working_angle > p.working_angle);
}

#[test]
fn par_con_interferencia_y_desacople() {
    assert!(pair(&spur(10), &spur(100)).unwrap().interference);
    assert!(pair(&spur(12), &spur(40)).unwrap().interference);
    assert!(matches!(pair(&spur(20), &GearSpec { module: 2.0, ..spur(20) }), Err(GearError::Mismatch(_))));
    assert!(matches!(pair(&spur(20), &GearSpec { pressure_angle: 25.0, ..spur(20) }), Err(GearError::Mismatch(_))));
}

#[test]
fn par_interno() {
    let ring = GearSpec { kind: GearKind::Internal, ..spur(60) };
    let p = pair(&spur(20), &ring).unwrap();
    assert_relative_eq!(p.center_distance, 20.0, epsilon = 1e-9);
    assert!(p.contact_ratio > 1.2, "{}", p.contact_ratio);
    assert!(!p.interference);
    assert!(pair(&ring, &spur(20)).is_err());
    let c = ring.circles().unwrap();
    assert_relative_eq!(c.tip, 30.0 - 1.0);
    assert_relative_eq!(c.root, 30.0 + 1.25);
    // El interno tiene espesor πm/2 en el primitivo
    let g = ring.profile().unwrap();
    assert!(g.points.iter().all(|p| { let r = p[0].hypot(p[1]); r > c.tip - 1e-6 && r < c.root + 1e-6 }));
}

#[test]
fn helicoidal_usa_medidas_transversales() {
    let h = GearSpec { helix_angle: 20.0, ..spur(30) };
    let c = h.circles().unwrap();
    assert_relative_eq!(c.module_t, 1.0 / 20f64.to_radians().cos(), epsilon = 1e-12);
    assert_relative_eq!(c.pitch, 15.0 / 20f64.to_radians().cos(), epsilon = 1e-9);
    assert!(c.alpha_t > 20f64.to_radians());
    assert!(h.profile().is_ok());
    let p = pair(&h, &GearSpec { helix_angle: -20.0, ..spur(60) }).unwrap();
    assert_relative_eq!(p.center_distance, 45.0 / 20f64.to_radians().cos(), epsilon = 1e-9);
}

#[test]
fn cremallera() {
    let r = GearSpec { kind: GearKind::Rack, teeth: 3, ..Default::default() };
    let o = r.rack_outline(3, 4.0).unwrap();
    // Largo = 3 pasos; área = 3 pasos × cuerpo + dientes y huecos
    let xs: Vec<f64> = o.iter().map(|p| p[0]).collect();
    let largo = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    assert_relative_eq!(largo, 3.0 * PI, epsilon = 1e-9);
    let ys: Vec<f64> = o.iter().map(|p| p[1]).collect();
    assert_relative_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 1.0);
    assert_relative_eq!(ys.iter().cloned().fold(f64::MAX, f64::min), -1.25 - 4.0);
    assert!(r.profile().is_err());
}

#[test]
fn entradas_invalidas() {
    assert!(GearSpec { module: 0.0, ..Default::default() }.circles().is_err());
    assert!(GearSpec { teeth: 3, ..Default::default() }.circles().is_err());
    assert!(GearSpec { pressure_angle: 60.0, ..Default::default() }.circles().is_err());
    assert!(GearSpec { helix_angle: 70.0, ..Default::default() }.circles().is_err());
}

#[test]
fn curvas_cierran_el_contorno() {
    let g = spur(16).profile().unwrap();
    assert!(g.curves.len() >= 16 * 4);
}

#[test]
fn curvas_encadenadas() {
    // Cada curva empieza donde termina la anterior (también al dar la vuelta)
    use cad_occt::Curve;
    let ends = |c: &Curve| match c {
        Curve::Arc(a, _, b) => (*a, *b),
        Curve::SplineEnds { points, .. } => (points[0], *points.last().unwrap()),
        _ => unreachable!(),
    };
    for z in [8u32, 17, 24, 60] {
        let g = spur(z).profile().unwrap();
        let n = g.curves.len();
        for i in 0..n {
            let (_, e) = ends(&g.curves[i]);
            let (s, _) = ends(&g.curves[(i + 1) % n]);
            let d = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2)).sqrt();
            assert!(d < 1e-6, "z={z} curva {i}: hueco de {d}");
        }
    }
}

#[test]
fn conico_perfil_plano_y_conos() {
    let s = GearSpec { kind: GearKind::Bevel, teeth: 24, module: 2.0, cone_angle: 30.0, ..Default::default() };
    // Primitivo 24 mm de radio, vértice a r/tan δ, generatriz r/sen δ
    assert_relative_eq!(s.circles().unwrap().pitch, 24.0);
    assert_relative_eq!(s.apex_height(), 24.0 / 30f64.to_radians().tan(), epsilon = 1e-12);
    assert_relative_eq!(s.cone_distance(), 48.0, epsilon = 1e-12);
    let p = s.profile().unwrap();
    // El plano pasa por el punto primitivo: el perfil cruza el radio 24 en el flanco
    let rs: Vec<f64> = p.points.iter().map(|q| q[0].hypot(q[1])).collect();
    let (lo, hi) = (rs.iter().cloned().fold(f64::MAX, f64::min), rs.iter().cloned().fold(0.0, f64::max));
    assert!(lo < 24.0 && hi > 24.0, "{lo} {hi}");
    // Paso angular exacto y simetría de 24 dientes
    let n = p.points.len() / 24;
    let (a, b) = (polar(p.points[3]), polar(p.points[3 + n]));
    assert_relative_eq!(a.0, b.0, epsilon = 1e-9);
    assert_relative_eq!((b.1 - a.1).rem_euclid(TAU), TAU / 24.0, epsilon = 1e-9);
    assert!(GearSpec { helix_angle: 10.0, ..s }.profile().is_err());
    assert!(GearSpec { cone_angle: 2.0, ..s }.profile().is_err());
}
