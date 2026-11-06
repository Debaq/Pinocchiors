//! Perfiles de resina por impresora (SLA5 de `PLAN_SLA.md`) y revisión de parámetros.
//!
//! Son **puntos de partida**: la exposición depende de la resina, del lote, de la
//! temperatura y del desgaste de la pantalla. Siempre calibrar con una pieza de prueba.
//! Los de «Estándar» salen de los archivos de referencia de SLA0 (Lychee).

use serde::Serialize;

use crate::sla::{ResinPrinter, ResinSettings};

/// Un perfil de resina para una impresora.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResinProfile {
    pub id: &'static str,
    /// `id` de la impresora.
    pub printer: &'static str,
    pub name: &'static str,
    pub settings: ResinSettings,
}

/// Los perfiles de todas las impresoras.
pub fn profiles() -> Vec<ResinProfile> {
    let base = ResinSettings::default();
    // LD-002H (2K mono): el archivo de referencia (3,5 s / 20 s × 8, subida 7 mm a 70 mm/min).
    let ld = ResinSettings { ..base.clone() };
    // Halot-Sky (4K mono): el .cxdlp de referencia (3 s / 30 s × 6, subida 7 mm a 1 mm/s).
    let halot = ResinSettings {
        exposure: 3.0,
        bottom_exposure: 30.0,
        bottom_layers: 6,
        lift_speed: 60.0,
        bottom_lift_speed: 60.0,
        retract_speed: 60.0,
        ..base
    };
    let mut out = Vec::new();
    for (printer, std) in [("ld-002h", ld), ("halot-sky", halot)] {
        let variant = |id, name, f: &dyn Fn(&mut ResinSettings)| {
            let mut s = std.clone();
            f(&mut s);
            ResinProfile { id, printer, name, settings: s }
        };
        out.push(variant("standard", "Estándar 0,05 mm", &|_| {}));
        out.push(variant("standard-fine", "Estándar 0,03 mm (detalle)", &|s| {
            s.layer_height = 0.03;
            s.exposure *= 0.75;
        }));
        out.push(variant("water-washable", "Lavable con agua", &|s| {
            s.exposure *= 1.15;
            s.density = 1.1;
        }));
        out.push(variant("abs-like", "Tipo ABS", &|s| {
            s.exposure *= 1.3;
            s.bottom_exposure *= 1.2;
            s.density = 1.15;
        }));
        out.push(variant("tough", "Tenaz / flexible", &|s| {
            s.exposure *= 1.6;
            s.bottom_exposure *= 1.3;
            s.lift_speed *= 0.7;
            s.density = 1.15;
        }));
    }
    out
}

/// Perfiles de una impresora.
pub fn profiles_for(printer: &ResinPrinter) -> Vec<ResinProfile> {
    profiles().into_iter().filter(|p| p.printer == printer.id).collect()
}

/// Avisos sobre los parámetros (vacío si todo está en rangos razonables).
pub fn validate(s: &ResinSettings, printer: &ResinPrinter) -> Vec<String> {
    let mut w = Vec::new();
    let (pw, _) = printer.pixel_size();
    if !(0.01..=0.2).contains(&s.layer_height) {
        w.push(format!("alto de capa {:.3} mm fuera de 0,01–0,2 mm", s.layer_height));
    }
    if !(0.5..=30.0).contains(&s.exposure) {
        w.push(format!("exposición {:.1} s fuera de 0,5–30 s", s.exposure));
    }
    if !(5.0..=120.0).contains(&s.bottom_exposure) {
        w.push(format!("exposición de base {:.0} s fuera de 5–120 s", s.bottom_exposure));
    }
    if s.bottom_exposure < s.exposure {
        w.push("la exposición de base es menor que la normal: la pieza puede no pegarse a la placa".into());
    }
    if s.bottom_layers == 0 {
        w.push("sin capas de base la pieza puede despegarse de la placa".into());
    } else if s.bottom_layers > 20 {
        w.push(format!("{} capas de base es mucho: la base sale gruesa y difícil de sacar", s.bottom_layers));
    }
    if s.lift < 2.0 || s.bottom_lift < 2.0 {
        w.push("subida menor a 2 mm: el film puede no despegarse".into());
    }
    if s.lift_speed <= 0.0 || s.bottom_lift_speed <= 0.0 || s.retract_speed <= 0.0 {
        w.push("las velocidades tienen que ser mayores que cero".into());
    }
    if s.lift_speed > 300.0 || s.retract_speed > 400.0 {
        w.push("velocidades muy altas: riesgo de que la pieza se suelte".into());
    }
    if s.layer_height < pw / 3.0 {
        w.push("capa mucho más fina que el píxel: más tiempo sin ganar detalle".into());
    }
    if s.xy_offset.abs() > 0.3 {
        w.push("compensación XY de más de 0,3 mm: las piezas cambian mucho de medida".into());
    }
    if !(0.0..=0.5).contains(&s.elephant_foot) {
        w.push("pata de elefante fuera de 0–0,5 mm".into());
    }
    if s.elephant_foot > 0.0 && s.bottom_layers == 0 {
        w.push("la pata de elefante solo se aplica a las capas de base y no hay capas de base".into());
    }
    if !(0.0..=0.5).contains(&s.hole_offset) {
        w.push("compensación de agujeros fuera de 0–0,5 mm".into());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::{HALOT_SKY, LD_002H};

    #[test]
    fn cada_impresora_tiene_perfiles_validos() {
        for pr in [&LD_002H, &HALOT_SKY] {
            let ps = profiles_for(pr);
            assert_eq!(ps.len(), 5);
            for p in ps {
                assert!(validate(&p.settings, pr).is_empty(), "{} {}: {:?}", pr.id, p.id, validate(&p.settings, pr));
            }
        }
    }

    #[test]
    fn avisos() {
        let s = ResinSettings { bottom_exposure: 1.0, bottom_layers: 0, ..Default::default() };
        let w = validate(&s, &LD_002H);
        assert!(w.len() >= 3, "{w:?}");
    }

    #[test]
    fn ajustes_en_json() {
        let s: ResinSettings = serde_json::from_str(r#"{"exposure": 2.2}"#).unwrap();
        assert_eq!(s.exposure, 2.2);
        assert_eq!(s.bottom_layers, ResinSettings::default().bottom_layers);
    }
}
