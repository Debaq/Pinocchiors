use super::*;
use cad_occt::Frame;

/// Todas las piezas del catálogo se arman (y sus agujeros y soportes) con cada variante
#[test]
fn every_item_builds() {
    for it in catalog() {
        let variants: Vec<String> = if it.variants.is_empty() { vec![String::new()] } else { it.variants.iter().map(|v| v.id.clone()).collect() };
        for v in variants {
            let mut p = LibraryPart::new(it.id, &v);
            let s = build(Frame::WORLD, &p).unwrap_or_else(|e| panic!("{} {v}: {e}", it.id));
            assert!(s.is_valid(), "{} {v}: forma inválida", it.id);
            let vol = s.mass().map(|m| m.volume).unwrap_or(0.0);
            assert!(vol > 0.0, "{} {v}: sin volumen", it.id);
            if it.holes {
                p.role = LibraryRole::Holes;
                let h = build(Frame::WORLD, &p).unwrap_or_else(|e| panic!("agujeros de {} {v}: {e}", it.id));
                assert!(h.mass().map(|m| m.volume).unwrap_or(0.0) > 0.0, "agujeros de {} {v}: vacíos", it.id);
            }
            if it.mounts {
                p.role = LibraryRole::Mounts;
                build(Frame::WORLD, &p).unwrap_or_else(|e| panic!("soportes de {} {v}: {e}", it.id));
            }
        }
    }
}

/// Medidas clave de la norma: la cara del NEMA 17 y su eje
#[test]
fn nema17_matches_the_standard() {
    let s = build(Frame::WORLD, &LibraryPart::new("nema17", "48")).unwrap();
    let (min, max) = s.bounds().unwrap();
    assert!((max[0] - min[0] - 42.3).abs() < 0.01, "cara {}", max[0] - min[0]);
    assert!((max[2] - 48.0).abs() < 0.01, "cuerpo hacia +Z");
    assert!((min[2] + 24.0).abs() < 0.01, "eje de 24 mm hacia −Z");
}

/// El imán queda embutido a ras: de −alto a 0
#[test]
fn magnet_is_flush() {
    let s = build(Frame::WORLD, &LibraryPart::new("magnet_disc", "10x3")).unwrap();
    let (min, max) = s.bounds().unwrap();
    assert!(max[2].abs() < 1e-6 && (min[2] + 3.0).abs() < 1e-6);
    assert!((max[0] - min[0] - 10.0).abs() < 1e-6);
}

#[test]
fn names() {
    assert_eq!(describe(&LibraryPart::new("nema17", "48")), "NEMA 17 · 48 mm");
    let mut b = LibraryPart::new("bolt_socket", "M3");
    b.length = Some(12.0);
    assert_eq!(describe(&b), "Tornillo Allen · M3 · 12 mm");
    b.role = LibraryRole::Holes;
    assert_eq!(describe(&b), "Agujeros para Tornillo Allen · M3 · 12 mm");
}

/// El perfil sale de su sección exacta y del largo pedido
#[test]
fn profile_section_and_length() {
    let mut p = LibraryPart::new("vslot_2040", "");
    p.length = Some(250.0);
    let s = build(Frame::WORLD, &p).unwrap();
    let (min, max) = s.bounds().unwrap();
    assert!((max[0] - min[0] - 20.0).abs() < 1e-6 && (max[1] - min[1] - 40.0).abs() < 1e-6);
    assert!((max[2] - 250.0).abs() < 1e-6 && min[2].abs() < 1e-6);
    // Ranuras y agujeros: bastante menos que la sección llena
    let area = s.mass().unwrap().volume / 250.0;
    assert!(area > 250.0 && area < 600.0, "sección {area}");
    assert_eq!(describe(&p), "Perfil V-slot 2040 · 250 mm");
}

/// Los agujeros de montaje de la placa salen donde dice su plano (centrada en el origen)
#[test]
fn board_holes_match_the_drawing() {
    let mut p = LibraryPart::new("rpi4", "");
    p.role = LibraryRole::Mounts;
    p.lift = 5.0;
    let s = build(Frame::WORLD, &p).unwrap();
    let (min, max) = s.bounds().unwrap();
    // Agujeros a 3,5 de los bordes y 58 × 49 entre sí: postes de Ø(2,7·1,1·2) alrededor
    let r = 2.7 * 1.1;
    assert!((min[0] - (3.5 - 42.5 - r)).abs() < 1e-3, "{min:?}");
    assert!((max[0] - (61.5 - 42.5 + r)).abs() < 1e-3, "{max:?}");
    assert!((max[1] - (52.5 - 28.0 + r)).abs() < 1e-3);
    assert!((max[2] - 5.0).abs() < 1e-6, "postes hasta la placa");
    // La placa queda encima de los postes
    let mut b = LibraryPart::new("rpi4", "");
    b.lift = 5.0;
    let (bmin, _) = build(Frame::WORLD, &b).unwrap().bounds().unwrap();
    assert!((bmin[2] - 5.0).abs() < 1e-6);
}

/// Las piezas de rol pieza son compradas; agujeros y soportes, no
#[test]
fn purchased_roles() {
    let mut p = LibraryPart::new("arduino_uno", "");
    assert!(is_purchased(&p));
    p.role = LibraryRole::Holes;
    assert!(!is_purchased(&p));
}

/// Vuelca el catálogo a JSON (para revisar textos y traducciones): `CATALOG_JSON=ruta cargo test -- --ignored dump_catalog`
#[test]
#[ignore]
fn dump_catalog() {
    let path = std::env::var("CATALOG_JSON").unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&catalog()).unwrap()).unwrap();
}
