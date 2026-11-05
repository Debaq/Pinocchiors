use cad_model::sketch_text::{TextAlign, TextError, add_text};
use cad_model::{Geometry, Sketch, find_regions};

const FONT: &[u8] = include_bytes!("../../../../apps/web/public/fonts/LiberationSans-Regular.ttf");

fn add(s: &mut Sketch, t: &str, align: TextAlign) -> cad_model::sketch_text::TextAdded {
    add_text(s, FONT, "Liberation Sans", t, 10.0, [100.0, 50.0], false, false, align).unwrap()
}

fn bbox(s: &Sketch, ids: &[u32]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for e in s.entities.iter().filter(|e| ids.contains(&e.id)) {
        let pts: Vec<u32> = match &e.geometry {
            Geometry::Line { start, end } => vec![*start, *end],
            Geometry::Spline { points, .. } => points.clone(),
            _ => vec![],
        };
        for p in pts {
            let p = s.points.iter().find(|q| q.id == p).unwrap();
            b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
        }
    }
    b
}

#[test]
fn o_son_dos_splines_cerradas() {
    let mut s = Sketch::default();
    let r = add(&mut s, "o", TextAlign::Left);
    assert_eq!(r.entities.len(), 2);
    for e in &s.entities {
        assert!(matches!(&e.geometry, Geometry::Spline { closed: true, .. }));
    }
    assert_eq!(find_regions(&s).unwrap().len(), 2);
}

#[test]
fn hi_tiene_contornos_cerrados_y_regiones() {
    let mut s = Sketch::default();
    let r = add(&mut s, "Hi", TextAlign::Left);
    // H: un contorno; i: punto y cuerpo
    let regs = find_regions(&s).unwrap();
    assert!(regs.len() >= 3, "regiones: {}", regs.len());
    // Cada punto de línea usado por dos tramos como mínimo (contornos cerrados)
    let mut uso = std::collections::HashMap::new();
    for e in &s.entities {
        if let Geometry::Line { start, end } = e.geometry {
            *uso.entry(start).or_insert(0) += 1;
            *uso.entry(end).or_insert(0) += 1;
        }
    }
    assert!(uso.values().all(|&n| n == 2));
    // Altura de la H = altura de mayúscula (~0.73 em)
    let b = bbox(&s, &r.entities);
    let alto = b[3] - 50.0;
    assert!((alto - 7.3).abs() < 0.4, "alto {alto}");
    assert!((b[1] - 50.0).abs() < 0.2);
    assert!(b[0] > 100.0);
}

#[test]
fn alineacion_desplaza() {
    let (mut a, mut b, mut c) = (Sketch::default(), Sketch::default(), Sketch::default());
    let ra = add(&mut a, "Hola", TextAlign::Left);
    let rb = add(&mut b, "Hola", TextAlign::Center);
    let rc = add(&mut c, "Hola", TextAlign::Right);
    let (ba, bb, bc) = (bbox(&a, &ra.entities), bbox(&b, &rb.entities), bbox(&c, &rc.entities));
    let w = bc[2] - bc[0];
    assert!(w > 10.0);
    // el centro queda cerca del ancla y el borde derecho cerca del ancla
    assert!(((bb[0] + bb[2]) / 2.0 - 100.0).abs() < 1.5);
    assert!((bc[2] - 100.0).abs() < 1.5);
    assert!((ba[0] - 100.0).abs() < 1.5);
}

#[test]
fn registro_del_texto() {
    let mut s = Sketch::default();
    let r = add_text(&mut s, FONT, "Liberation Sans", " Hi ", 10.0, [1.0, 2.0], true, false, TextAlign::Center).unwrap();
    assert_eq!(s.texts.len(), 1);
    let t = &s.texts[0];
    assert_eq!((t.id, t.anchor), (r.id, r.anchor));
    assert_eq!(t.text, "Hi");
    assert_eq!(t.size, 10.0);
    assert_eq!(t.font, "Liberation Sans");
    assert!(t.style.bold && !t.style.italic);
    assert_eq!(t.style.align, "center");
    assert_eq!(t.entities, r.entities);
    assert!(t.block.is_none());
    let a = s.points.iter().find(|p| p.id == t.anchor).unwrap();
    assert_eq!((a.x, a.y), (1.0, 2.0));
    assert!(t.points.iter().all(|p| s.points.iter().any(|q| q.id == *p)));
    assert!(![t.id].contains(&t.anchor));
    assert!(!s.entities.iter().any(|e| e.id == t.id) && !s.points.iter().any(|p| p.id == t.id));
    // izquierda: estilo por defecto sin alineación
    let r2 = add(&mut s, "A", TextAlign::Left);
    let t2 = s.texts.iter().find(|t| t.id == r2.id).unwrap();
    assert_eq!(t2.style.align, "");
}

#[test]
fn errores() {
    let mut s = Sketch::default();
    let f = |s: &mut Sketch, font: &[u8], t: &str, size: f64| add_text(s, font, "x", t, size, [0.0, 0.0], false, false, TextAlign::Left);
    assert!(matches!(f(&mut s, FONT, "  ", 5.0), Err(TextError::Empty)));
    assert!(matches!(f(&mut s, FONT, "a", 0.0), Err(TextError::BadSize)));
    assert!(matches!(f(&mut s, FONT, "a", f64::NAN), Err(TextError::BadSize)));
    assert!(matches!(f(&mut s, FONT, "a\nb", 5.0), Err(TextError::MultiLine)));
    assert!(matches!(f(&mut s, b"no es una fuente", "a", 5.0), Err(TextError::BadFont(_))));
    assert!(s.texts.is_empty() && s.points.is_empty());
}
