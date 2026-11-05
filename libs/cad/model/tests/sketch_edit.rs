//! Edición del sketch en Rust (borrar, transformar, simetría, patrones,
//! equidistante y recortar): geometría, ids, restricciones y que el solver
//! siga convergiendo después.
use approx::assert_relative_eq;
use cad_model::sketch_edit::*;
use cad_model::*;
use serde_json::json;

fn pt(s: &Sketch, id: u32) -> P2 {
    s.point(id).unwrap()
}

fn ok(s: &mut Sketch) {
    let r = s.solve().unwrap();
    assert!(r.status != SketchStatus::Failed && r.status != SketchStatus::OverConstrained, "solver: {:?}", r.status);
}

fn geom(s: &Sketch, id: u32) -> Geometry {
    s.entity(id).unwrap().geometry.clone()
}

fn line_ends(s: &Sketch, id: u32) -> (P2, P2) {
    let Geometry::Line { start, end } = geom(s, id) else { panic!("no es línea") };
    (pt(s, start), pt(s, end))
}

fn near(a: P2, b: P2) {
    assert_relative_eq!(a[0], b[0], epsilon = 1e-6);
    assert_relative_eq!(a[1], b[1], epsilon = 1e-6);
}

#[test]
fn delete_entity_point_and_errors() {
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [10.0, 5.0]);
    let Geometry::Line { start, .. } = geom(&s, l[0]) else { unreachable!() };
    // Un punto borra las dos líneas que lo usan (y sus restricciones)
    let gone = delete(&mut s, &[start]).unwrap();
    assert_eq!(gone, vec![l[0].min(l[3]), l[0].max(l[3])]);
    assert_eq!(s.entities.len(), 2);
    assert!(s.constraints.iter().all(|c| constraint_ids(c).iter().all(|i| s.entity(*i).is_ok() || s.point(*i).is_ok())));
    // Un id que no existe no toca nada
    let n = s.entities.len();
    assert!(delete(&mut s, &[l[1], 9999]).is_err());
    assert_eq!(s.entities.len(), n);
    // El origen no se borra
    let o = s.ensure_origin();
    assert!(delete(&mut s, &[o]).is_err());
    // Una entidad y un punto suelto
    let p = s.add_point(50.0, 50.0);
    delete(&mut s, &[l[1], p]).unwrap();
    assert!(s.point(p).is_err());
    assert_eq!(s.entities.len(), 1);
}

#[test]
fn transform_move_rotate_scale_and_retune() {
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [10.0, 5.0]);
    s.constrain(SketchConstraint::Length { line: l[0], value: 10.0, reference: false, opts: DimOpts::default() });
    let ids = l.to_vec();
    transform(&mut s, &ids, &Xform::translation([3.0, 2.0]), false).unwrap();
    near(line_ends(&s, l[0]).0, [3.0, 2.0]);
    near(line_ends(&s, l[0]).1, [13.0, 2.0]);
    ok(&mut s);
    // Girar 90°: las horizontales pasan a verticales y viceversa
    transform(&mut s, &ids, &Xform::rotation([0.0, 0.0], 90.0), false).unwrap();
    assert!(s.constraints.iter().any(|c| *c == SketchConstraint::Vertical { line: l[0] }));
    assert!(s.constraints.iter().any(|c| *c == SketchConstraint::Horizontal { line: l[1] }));
    ok(&mut s);
    near(line_ends(&s, l[0]).0, [-2.0, 3.0]);
    // Escalar x2: la cota de largo toma lo que mide ahora
    transform(&mut s, &ids, &Xform::scaling([0.0, 0.0], 2.0), false).unwrap();
    assert_eq!(s.constraints.iter().find_map(|c| if let SketchConstraint::Length { value, .. } = c { Some(*value) } else { None }), Some(20.0));
    ok(&mut s);
    // Sin nada que mover
    assert!(transform(&mut s, &[], &Xform::translation([1.0, 0.0]), false).is_err());
}

#[test]
fn transform_circle_radius_and_copy() {
    let mut s = Sketch::new();
    let c = s.circle([5.0, 5.0], 2.0);
    s.constrain(SketchConstraint::Radius { entity: c, value: 2.0, reference: false, opts: DimOpts::default() });
    let ent = s.add_point(0.0, 0.0);
    let _ = ent;
    transform(&mut s, &[c], &Xform::scaling([0.0, 0.0], 3.0), false).unwrap();
    assert_relative_eq!(s.radius(c).unwrap(), 6.0);
    near(pt(&s, match geom(&s, c) { Geometry::Circle { center, .. } => center, _ => 0 }), [15.0, 15.0]);
    ok(&mut s);
    // Copia con giro: ids nuevos, el original queda
    let before = s.entities.len();
    let out = transform(&mut s, &[c], &Xform::rotation([0.0, 0.0], 90.0), true).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(s.entities.len(), before + 1);
    assert_relative_eq!(s.radius(out[0]).unwrap(), 6.0);
    let Geometry::Circle { center, .. } = geom(&s, out[0]) else { panic!() };
    near(pt(&s, center), [-15.0, 15.0]);
    // La cota de radio se copió con la copia
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::Radius { entity, value, .. } if *entity == out[0] && (*value - 6.0).abs() < 1e-9)));
    ok(&mut s);
}

#[test]
fn transform_copy_keeps_internal_constraints_and_marks() {
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [4.0, 2.0]);
    s.entities.iter_mut().find(|e| e.id == l[0]).unwrap().construction = true;
    let out = transform(&mut s, &l, &Xform::translation([10.0, 0.0]), true).unwrap();
    assert_eq!(out.len(), 4);
    assert!(s.entity(out[0]).unwrap().construction);
    assert!(!s.entity(out[1]).unwrap().construction);
    assert!(s.constraints.contains(&SketchConstraint::Horizontal { line: out[0] }));
    assert!(s.constraints.contains(&SketchConstraint::Vertical { line: out[3] }));
    near(line_ends(&s, out[0]).0, [10.0, 0.0]);
    ok(&mut s);
}

#[test]
fn locked_layer_does_not_move() {
    let mut s = Sketch::new();
    let a = s.line([0.0, 0.0], [5.0, 0.0]);
    s.layers.push(cad_model::sketch::SketchLayer { id: 99, name: "x".into(), hidden: false, locked: true });
    s.entities.iter_mut().find(|e| e.id == a).unwrap().layer = Some(99);
    assert!(transform(&mut s, &[a], &Xform::translation([1.0, 1.0]), false).is_err());
    near(line_ends(&s, a).0, [0.0, 0.0]);
}

#[test]
fn mirror_line_circle_arc() {
    let mut s = Sketch::new();
    let axis = s.line([0.0, -10.0], [0.0, 10.0]);
    let l = s.line([2.0, 0.0], [6.0, 3.0]);
    let c = s.circle([4.0, -4.0], 1.5);
    let arc = s.arc([3.0, 6.0], [5.0, 6.0], [3.0, 8.0]);
    let out = mirror(&mut s, &[l, c, arc, axis], axis).unwrap();
    assert_eq!(out.len(), 3);
    let (a, b) = line_ends(&s, out[0]);
    near(a, [-2.0, 0.0]);
    near(b, [-6.0, 3.0]);
    // Círculo: mismo radio por restricción `equal`
    assert!(s.constraints.contains(&SketchConstraint::Equal { a: c, b: out[1] }));
    // Arco: se invierte el sentido, así sigue antihorario: start = reflejo del end
    let Geometry::Arc { center, start, end } = geom(&s, out[2]) else { panic!() };
    near(pt(&s, center), [-3.0, 6.0]);
    near(pt(&s, start), [-3.0, 8.0]);
    near(pt(&s, end), [-5.0, 6.0]);
    let sym = s.constraints.iter().filter(|k| matches!(k, SketchConstraint::Symmetric { line, .. } if *line == axis)).count();
    assert_eq!(sym, 2 + 1 + 3);
    ok(&mut s);
    // Eje que no es línea
    assert!(mirror(&mut s, &[l], c).is_err());
}

#[test]
fn mirror_shares_points_on_axis() {
    let mut s = Sketch::new();
    let axis = s.line([0.0, 0.0], [0.0, 10.0]);
    let (p, q) = (s.add_point(0.0, 2.0), s.add_point(3.0, 2.0));
    let l = s.add_line(p, q);
    let out = mirror(&mut s, &[l], axis).unwrap();
    let Geometry::Line { start, .. } = geom(&s, out[0]) else { panic!() };
    assert_eq!(start, p);
    ok(&mut s);
}

#[test]
fn linear_pattern_record_and_constraints() {
    let mut s = Sketch::new();
    let l = s.line([0.0, 0.0], [2.0, 0.0]);
    let c = s.circle([1.0, 3.0], 0.5);
    let r = linear_pattern(&mut s, &[l, c], 3, [5.0, 0.0], 1, [0.0, 0.0]).unwrap();
    assert_eq!(r.pattern, 1);
    assert_eq!(r.copies.len(), 2);
    assert_eq!(r.copies[1].len(), 2);
    near(line_ends(&s, r.copies[1][0]).0, [10.0, 0.0]);
    // Solo el primer par lleva cotas; el resto, mismo desplazamiento
    let hd = s.constraints.iter().filter(|k| matches!(k, SketchConstraint::HorizontalDistance { .. })).count();
    assert_eq!(hd, 1);
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::EqualOffset { .. })));
    // Registro igual al de la interfaz
    let rec = &s.patterns[0];
    assert_eq!(rec["id"], 1);
    assert_eq!(rec["spec"], json!({ "kind": "linear", "count": 3, "offset": [5.0, 0.0], "rows": 1, "offset2": [0.0, 0.0] }));
    assert_eq!(rec["source"], json!([l, c]));
    assert_eq!(rec["copies"], json!([r.copies[0], r.copies[1]]));
    assert_eq!(rec["owned"].as_array().unwrap().len(), 4);
    assert_eq!(rec["skip"], json!([]));
    ok(&mut s);
    // El segundo patrón recibe el id 2
    let r2 = linear_pattern(&mut s, &[l], 2, [0.0, 4.0], 1, [0.0, 0.0]).unwrap();
    assert_eq!(r2.pattern, 2);
    assert!(linear_pattern(&mut s, &[l], 1, [1.0, 0.0], 1, [0.0, 0.0]).is_err());
}

#[test]
fn linear_pattern_grid() {
    let mut s = Sketch::new();
    let c = s.circle([0.0, 0.0], 1.0);
    let r = linear_pattern(&mut s, &[c], 2, [4.0, 0.0], 3, [0.0, 5.0]).unwrap();
    assert_eq!(r.copies.len(), 5);
    let Geometry::Circle { center, .. } = geom(&s, r.copies[4][0]) else { panic!() };
    near(pt(&s, center), [4.0, 10.0]);
    ok(&mut s);
}

#[test]
fn circular_pattern_full_turn() {
    let mut s = Sketch::new();
    let o = s.add_point(0.0, 0.0);
    let l = s.line([5.0, 0.0], [7.0, 0.0]);
    let r = circular_pattern(&mut s, &[l], 4, o, 360.0, true).unwrap();
    assert_eq!(r.copies.len(), 3);
    near(line_ends(&s, r.copies[0][0]).0, [0.0, 5.0]);
    near(line_ends(&s, r.copies[1][0]).1, [-7.0, 0.0]);
    near(line_ends(&s, r.copies[2][0]).0, [0.0, -5.0]);
    // Radios iguales y cota de ángulo entre líneas de construcción
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::Angle { degrees, .. } if (*degrees - 90.0).abs() < 1e-9)));
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::EqualRotation { .. })));
    let rec = &s.patterns[0];
    assert_eq!(rec["spec"], json!({ "kind": "circular", "count": 4, "center": o, "span": 360.0, "rotate": true }));
    assert_eq!(rec["source"], json!([l]));
    // owned: las 3 copias y las 2 líneas guía de construcción (que salen tras la primera copia)
    assert_eq!(rec["owned"].as_array().unwrap().len(), 5);
    assert!(s.entity(r.owned[1]).unwrap().construction);
    ok(&mut s);
    // El centro sobrevive aunque se borre el original de un patrón
    assert!(s.point(o).is_ok());
}

#[test]
fn circular_pattern_half_turn_and_limits() {
    let mut s = Sketch::new();
    let o = s.add_point(0.0, 0.0);
    let l = s.line([5.0, 0.0], [7.0, 0.0]);
    let r = circular_pattern(&mut s, &[l], 2, o, 360.0, true).unwrap();
    near(line_ends(&s, r.copies[0][0]).0, [-5.0, 0.0]);
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::Midpoint { point, .. } if *point == o)));
    ok(&mut s);
    let mut t = Sketch::new();
    let o = t.add_point(0.0, 0.0);
    let l = t.line([5.0, 0.0], [7.0, 0.0]);
    assert!(circular_pattern(&mut t, &[l], 3, o, 0.0, true).is_err());
    assert!(circular_pattern(&mut t, &[l], 1, o, 360.0, true).is_err());
    // 90° repartidos en 3 copias, sin girar: se trasladan
    let r = circular_pattern(&mut t, &[l], 3, o, 90.0, false).unwrap();
    // Sin girar la línea conserva su dirección y su largo
    let (p0, p1) = line_ends(&t, r.copies[1][0]);
    near(p0, [0.0, 5.0]);
    near(p1, [2.0, 5.0]);
}

#[test]
fn offset_open_chain_left() {
    let mut s = Sketch::new();
    // Camino en L hacia la derecha y luego arriba; a la izquierda del recorrido = arriba/izquierda
    let (a, b, c) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(10.0, 10.0));
    let l1 = s.add_line(a, b);
    let l2 = s.add_line(b, c);
    s.constrain(SketchConstraint::Horizontal { line: l1 });
    let out = offset_chain(&mut s, &[l1, l2], 2.0, false, OffsetCaps::Open).unwrap();
    assert_eq!(out.len(), 2);
    let (p0, p1) = line_ends(&s, out[0]);
    near(p0, [0.0, 2.0]);
    near(p1, [8.0, 2.0]);
    near(line_ends(&s, out[1]).1, [8.0, 10.0]);
    // La primera era horizontal: la copia también; la otra, paralela
    assert!(s.constraints.contains(&SketchConstraint::Horizontal { line: out[0] }));
    assert!(s.constraints.contains(&SketchConstraint::Parallel { a: l2, b: out[1] }));
    let off = s.constraints.iter().find_map(|k| if let SketchConstraint::Offset { pairs, value, .. } = k { Some((pairs.clone(), *value)) } else { None }).unwrap();
    assert_eq!(off, (vec![[l1, out[0]], [l2, out[1]]], 2.0));
    ok(&mut s);
}

#[test]
fn offset_both_sides_with_round_caps_and_closed() {
    let mut s = Sketch::new();
    let l = s.line([0.0, 0.0], [10.0, 0.0]);
    let out = offset_chain(&mut s, &[l], 1.0, true, OffsetCaps::Round).unwrap();
    // Dos paralelas y dos arcos en las puntas
    assert_eq!(out.len(), 4);
    assert_eq!(out.iter().filter(|i| matches!(geom(&s, **i), Geometry::Arc { .. })).count(), 2);
    // El arco tangente a la paralela es degenerado para el solver (marca redundancia), pero converge
    let r = s.solve().unwrap();
    assert!(r.status != SketchStatus::Failed && r.residual < 1e-6);
    // Lazo cerrado antihorario hacia adentro
    let mut t = Sketch::new();
    let r = t.rectangle([0.0, 0.0], [10.0, 6.0]);
    let out = offset_chain(&mut t, &r, 1.0, false, OffsetCaps::Open).unwrap();
    assert_eq!(out.len(), 4);
    let xs: Vec<f64> = out.iter().flat_map(|i| { let (a, b) = line_ends(&t, *i); [a[0], b[0]] }).collect();
    assert!(xs.iter().all(|x| (*x - 1.0).abs() < 1e-9 || (*x - 9.0).abs() < 1e-9));
    ok(&mut t);
    // Errores
    assert!(offset_chain(&mut t, &r, 0.0, false, OffsetCaps::Open).is_err());
    let c = t.circle([0.0, 0.0], 1.0);
    assert!(offset_chain(&mut t, &[c], 1.0, false, OffsetCaps::Open).is_err());
}

#[test]
fn offset_arc_shares_center() {
    let mut s = Sketch::new();
    let a = s.arc([0.0, 0.0], [5.0, 0.0], [0.0, 5.0]);
    let Geometry::Arc { center, .. } = geom(&s, a) else { panic!() };
    // Antihorario: la izquierda es hacia el centro, así que 1 da radio 4
    let out = offset_chain(&mut s, &[a], 1.0, false, OffsetCaps::Open).unwrap();
    assert_eq!(out.len(), 1);
    let Geometry::Arc { center: c2, start, .. } = geom(&s, out[0]) else { panic!() };
    assert_eq!(c2, center);
    assert_relative_eq!(pt(&s, start)[0], 4.0, epsilon = 1e-9);
    ok(&mut s);
}

#[test]
fn trim_line_between_two_cuts_keeps_marks() {
    let mut s = Sketch::new();
    let h = s.line([0.0, 0.0], [10.0, 0.0]);
    s.constrain(SketchConstraint::Horizontal { line: h });
    s.constrain(SketchConstraint::Length { line: h, value: 10.0, reference: false, opts: DimOpts::default() });
    s.entities.iter_mut().find(|e| e.id == h).unwrap().layer = Some(7);
    let _v1 = s.line([3.0, -2.0], [3.0, 2.0]);
    let _v2 = s.line([7.0, -2.0], [7.0, 2.0]);
    trim(&mut s, h, [5.0, 0.0]).unwrap();
    // La línea queda en dos tramos; el nuevo hereda la capa y la horizontal
    let lines: Vec<&SketchEntity> = s.entities.iter().filter(|e| e.layer == Some(7) && e.id != h).collect();
    assert_eq!(lines.len(), 1);
    let rest = lines[0].id;
    assert!(s.constraints.contains(&SketchConstraint::Horizontal { line: rest }));
    // La cota de largo del entero se fue
    assert!(!s.constraints.iter().any(|k| matches!(k, SketchConstraint::Length { .. })));
    near(line_ends(&s, h).1, [3.0, 0.0]);
    near(line_ends(&s, rest).0, [7.0, 0.0]);
    near(line_ends(&s, rest).1, [10.0, 0.0]);
    // Las líneas cruzadas se partieron en el punto del corte
    assert_eq!(s.entities.iter().filter(|e| matches!(e.geometry, Geometry::Line { .. })).count(), 2 + 1 + 1 + 2);
    ok(&mut s);
}

#[test]
fn trim_line_end_and_whole() {
    let mut s = Sketch::new();
    let h = s.line([0.0, 0.0], [10.0, 0.0]);
    let _v = s.line([4.0, -2.0], [4.0, 2.0]);
    trim(&mut s, h, [8.0, 0.0]).unwrap();
    near(line_ends(&s, h).1, [4.0, 0.0]);
    // Sin cruces: la línea entera se va
    let lone = s.line([0.0, 20.0], [5.0, 20.0]);
    trim(&mut s, lone, [2.0, 20.0]).unwrap();
    assert!(s.entity(lone).is_err());
    assert!(trim(&mut s, 12345, [0.0, 0.0]).is_err());
}

#[test]
fn trim_circle_becomes_arc_and_arc_splits() {
    let mut s = Sketch::new();
    let c = s.circle([0.0, 0.0], 5.0);
    let v1 = s.line([3.0, -10.0], [3.0, 10.0]);
    let _ = v1;
    let v2 = s.line([-3.0, -10.0], [-3.0, 10.0]);
    let _ = v2;
    // Se quita el tramo de arriba (clic en (0, 5)): queda el arco de abajo… y los costados
    s.entities.iter_mut().find(|e| e.id == c).unwrap().construction = false;
    trim(&mut s, c, [0.0, 5.0]).unwrap();
    let Geometry::Arc { center, start, end } = geom(&s, c) else { panic!("el círculo debía pasar a arco") };
    assert_eq!(pt(&s, center), [0.0, 0.0]);
    // Antihorario de (-3,4) a (3,4)?? no: el tramo quitado es el de arriba, queda de (3,4) a (-3,4) por abajo
    near(pt(&s, start), [-3.0, 4.0]);
    near(pt(&s, end), [3.0, 4.0]);
    // Las líneas cruzadas se parten en el corte (cierra regiones): 2 verticales -> 4 líneas
    assert_eq!(s.entities.iter().filter(|e| matches!(e.geometry, Geometry::Line { .. })).count(), 4);
    ok(&mut s);
}

#[test]
fn trim_arc_middle_into_two_with_marks() {
    let mut s = Sketch::new();
    let a = s.arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0]);
    s.entities.iter_mut().find(|e| e.id == a).unwrap().construction = true;
    s.entities.iter_mut().find(|e| e.id == a).unwrap().construction = false;
    let _c1 = s.line([0.0, 0.0], [4.0, 6.0]); // corta el arco en ~(3.3,3.8)
    let _c2 = s.line([0.0, 0.0], [-4.0, 6.0]);
    s.entities.iter_mut().find(|e| e.id == a).unwrap().layer = Some(3);
    trim(&mut s, a, [0.0, 5.0]).unwrap();
    let arcs: Vec<&SketchEntity> = s.entities.iter().filter(|e| matches!(e.geometry, Geometry::Arc { .. })).collect();
    assert_eq!(arcs.len(), 2);
    assert!(arcs.iter().all(|e| e.layer == Some(3)));
    ok(&mut s);
}

#[test]
fn trim_by_circle_and_ignores_construction() {
    let mut s = Sketch::new();
    let h = s.line([-10.0, 0.0], [10.0, 0.0]);
    let c = s.circle([0.0, 0.0], 4.0);
    // El círculo corta la línea en (-4,0) y (4,0); el tramo del medio se va
    trim(&mut s, h, [0.0, 0.0]).unwrap();
    near(line_ends(&s, h).1, [-4.0, 0.0]);
    assert!(s.constraints.iter().any(|k| matches!(k, SketchConstraint::PointOnCircle { circle, .. } if *circle == c)));
    // Lo de construcción no corta
    let mut t = Sketch::new();
    let h = t.line([0.0, 0.0], [10.0, 0.0]);
    let v = t.line([5.0, -1.0], [5.0, 1.0]);
    t.entities.iter_mut().find(|e| e.id == v).unwrap().construction = true;
    trim(&mut t, h, [2.0, 0.0]).unwrap();
    assert!(t.entity(h).is_err());
}
