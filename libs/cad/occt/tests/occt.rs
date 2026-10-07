use approx::assert_relative_eq;
use cad_occt::*;
use std::f64::consts::PI;

fn require() -> bool {
    if !available() {
        eprintln!("OpenCASCADE no disponible: test omitido");
        return false;
    }
    true
}

fn cube(size: f64) -> Shape {
    Shape::make_box(Frame::WORLD, size, size, size).unwrap()
}

fn rect(w: f64, h: f64) -> Vec<Curve> {
    let p = [[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, h, 0.0], [0.0, h, 0.0]];
    (0..4).map(|i| Curve::Line(p[i], p[(i + 1) % 4])).collect()
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[test]
fn box_topology_and_mass() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    assert_eq!(b.kind(), ShapeKind::Solid);
    assert!(b.is_valid());
    assert_eq!(b.face_count(), 6);
    assert_eq!(b.edge_count(), 12);
    let m = b.mass().unwrap();
    assert_relative_eq!(m.volume, 1000.0, epsilon = 1e-6);
    assert_relative_eq!(m.area, 600.0, epsilon = 1e-6);
    assert_relative_eq!(m.center[2], 5.0, epsilon = 1e-9);
    assert_relative_eq!(m.bbox_max[0], 10.0, epsilon = 1e-6);
    // Normales salientes
    for f in b.faces().unwrap() {
        assert_eq!(f.surface, SurfaceKind::Plane);
        assert!(dot(f.normal, sub(f.point, [5.0, 5.0, 5.0])) > 0.0, "{f:?}");
    }
    for e in 0..b.edge_count() {
        assert_eq!(b.edge_faces(e).unwrap().len(), 2);
        assert_eq!(b.edge_info(e).unwrap().curve, CurveKind::Line);
    }
}

#[test]
fn plate_with_hole() {
    if !require() {
        return;
    }
    let hole = vec![Curve::Circle { center: [50.0, 25.0, 0.0], normal: [0.0, 0.0, 1.0], radius: 10.0 }];
    let face = Shape::face(&[rect(100.0, 50.0), hole]).unwrap();
    assert_eq!(face.kind(), ShapeKind::Face);
    let plate = face.prism([0.0, 0.0, 5.0]).unwrap();
    assert!(plate.is_valid());
    assert_eq!(plate.face_count(), 7);
    let v = plate.mass().unwrap().volume;
    assert_relative_eq!(v, 100.0 * 50.0 * 5.0 - PI * 100.0 * 5.0, epsilon = 1e-6);
    let cyl: Vec<_> = plate.faces().unwrap().into_iter().filter(|f| f.surface == SurfaceKind::Cylinder).collect();
    assert_eq!(cyl.len(), 1);
    assert_relative_eq!(cyl[0].radius.unwrap(), 10.0, epsilon = 1e-9);
    // La normal del agujero apunta hacia el eje (hacia afuera del material)
    let toward_axis = sub([50.0, 25.0, cyl[0].point[2]], cyl[0].point);
    assert!(dot(cyl[0].normal, toward_axis) > 0.0);
}

#[test]
fn slot_profile_with_arcs() {
    if !require() {
        return;
    }
    // Ranura: dos rectas de 20 y dos semicírculos de radio 5
    let c = vec![
        Curve::Line([0.0, 0.0, 0.0], [20.0, 0.0, 0.0]),
        Curve::Arc([20.0, 0.0, 0.0], [25.0, 5.0, 0.0], [20.0, 10.0, 0.0]),
        Curve::Line([20.0, 10.0, 0.0], [0.0, 10.0, 0.0]),
        Curve::Arc([0.0, 10.0, 0.0], [-5.0, 5.0, 0.0], [0.0, 0.0, 0.0]),
    ];
    let s = Shape::face(&[c]).unwrap().prism([0.0, 0.0, 1.0]).unwrap();
    assert_relative_eq!(s.mass().unwrap().volume, 200.0 + PI * 25.0, epsilon = 1e-6);
}

#[test]
fn spline_closed_profile() {
    if !require() {
        return;
    }
    let pts: Vec<P3> = (0..=12).map(|i| {
        let a = i as f64 / 12.0 * 2.0 * PI;
        [10.0 * a.cos(), 10.0 * a.sin(), 0.0]
    }).collect();
    let s = Shape::face(&[vec![Curve::Spline(pts)]]).unwrap().prism([0.0, 0.0, 1.0]).unwrap();
    assert!(s.is_valid());
    // Se aproxima a un círculo de radio 10
    assert_relative_eq!(s.mass().unwrap().volume, PI * 100.0, max_relative = 0.01);
}

#[test]
fn open_profile_is_an_error() {
    if !require() {
        return;
    }
    let c = vec![Curve::Line([0.0; 3], [1.0, 0.0, 0.0]), Curve::Line([1.0, 0.0, 0.0], [1.0, 1.0, 0.0])];
    let e = Shape::face(&[c]).unwrap_err();
    assert!(e.0.contains("cerrado"), "{e}");
}

#[test]
fn fillet_and_chamfer() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let all: Vec<usize> = (0..b.edge_count()).collect();
    let f = b.fillet(&all, 1.0).unwrap();
    assert!(f.is_valid());
    assert_eq!(f.face_count(), 26);
    // Cubo con aristas redondeadas r=1: V = a³ − (12·(4−π)·a' + 8·(1−π/6)·... ) simplificado:
    // a³ − 3·(4−π)·r²·(a−2r)·... calculamos la fórmula exacta.
    let (a, r) = (10.0, 1.0);
    let expected = a * a * a - 12.0 * (1.0 - PI / 4.0) * r * r * (a - 2.0 * r)
        - 8.0 * (r * r * r - PI * r * r * r / 6.0)
        - 0.0;
    // Esquinas: cubo r³ menos octante de esfera, ya restado; aristas: cuadrado r² menos cuarto de círculo.
    assert_relative_eq!(f.mass().unwrap().volume, expected, max_relative = 1e-4);

    let c = b.chamfer(&[0], 2.0).unwrap();
    assert_relative_eq!(c.mass().unwrap().volume, 1000.0 - 0.5 * 4.0 * 10.0, epsilon = 1e-6);

    let err = b.fillet(&all, 6.0).unwrap_err();
    assert!(!err.0.is_empty());
    assert!(b.fillet(&[99], 1.0).unwrap_err().0.contains("arista"));
}

#[test]
fn booleans() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let c = Shape::cylinder(Frame::at([5.0, 5.0, -1.0]), 2.0, 12.0).unwrap();
    let cut = b.cut(&c).unwrap();
    assert!(cut.is_valid());
    assert_relative_eq!(cut.mass().unwrap().volume, 1000.0 - PI * 4.0 * 10.0, epsilon = 1e-6);
    let inter = b.intersect(&c).unwrap();
    assert_relative_eq!(inter.mass().unwrap().volume, PI * 4.0 * 10.0, epsilon = 1e-6);
    let other = cube(10.0).translate([5.0, 0.0, 0.0]).unwrap();
    let u = b.union(&other).unwrap();
    assert_relative_eq!(u.mass().unwrap().volume, 1500.0, epsilon = 1e-6);
    // SimplifyResult funde las caras coplanares: queda una caja de 6 caras
    assert_eq!(u.face_count(), 6);

    let many: Vec<Shape> = (0..4).map(|i| cube(1.0).translate([i as f64 * 2.0, 0.0, 0.0]).unwrap()).collect();
    let fused = Shape::fuse_all(&many).unwrap();
    assert_relative_eq!(fused.mass().unwrap().volume, 4.0, epsilon = 1e-9);
}

#[test]
fn revolve_loft_sweep() {
    if !require() {
        return;
    }
    // Rectángulo [5,7]×[0,10] en XZ girado en Z: tubo
    let p = [[5.0, 0.0, 0.0], [7.0, 0.0, 0.0], [7.0, 0.0, 10.0], [5.0, 0.0, 10.0]];
    let tube = Shape::polygon(&p).unwrap().revolve(Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] }, 2.0 * PI).unwrap();
    assert!(tube.is_valid());
    assert_relative_eq!(tube.mass().unwrap().volume, PI * (49.0 - 25.0) * 10.0, epsilon = 1e-6);
    let half = Shape::polygon(&p).unwrap().revolve(Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] }, PI).unwrap();
    assert_relative_eq!(half.mass().unwrap().volume, PI * 24.0 * 5.0, epsilon = 1e-6);

    // Loft entre cuadrados de lado 10 (z=0) y 5 (z=10): tronco de pirámide
    let sq = |s: f64, z: f64| {
        let q = [[-s / 2.0, -s / 2.0, z], [s / 2.0, -s / 2.0, z], [s / 2.0, s / 2.0, z], [-s / 2.0, s / 2.0, z]];
        Shape::wire(&(0..4).map(|i| Curve::Line(q[i], q[(i + 1) % 4])).collect::<Vec<_>>()).unwrap()
    };
    let frustum = Shape::loft(&[sq(10.0, 0.0), sq(5.0, 10.0)], true, true).unwrap();
    let (a1, a2, h) = (100.0f64, 25.0f64, 10.0);
    assert_relative_eq!(frustum.mass().unwrap().volume, h / 3.0 * (a1 + a2 + (a1 * a2).sqrt()), epsilon = 1e-6);

    // Barrido de un círculo r=1 por una recta de 10: cilindro
    let circle = Shape::face(&[vec![Curve::Circle { center: [0.0; 3], normal: [0.0, 0.0, 1.0], radius: 1.0 }]]).unwrap();
    let spine = Shape::wire(&[Curve::Line([0.0; 3], [0.0, 0.0, 10.0])]).unwrap();
    let rod = circle.sweep(&spine).unwrap();
    assert_relative_eq!(rod.mass().unwrap().volume, PI * 10.0, epsilon = 1e-6);
}

#[test]
fn shell_split_mirror_rotate() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let top = b.faces().unwrap().iter().position(|f| f.normal[2] > 0.9).unwrap();
    let cup = b.shell(&[top], -1.0).unwrap();
    assert!(cup.is_valid());
    assert_relative_eq!(cup.mass().unwrap().volume, 1000.0 - 8.0 * 8.0 * 9.0, epsilon = 1e-6);

    let half = b.split_keep([0.0, 0.0, 5.0], [0.0, 0.0, 1.0]).unwrap();
    let m = half.mass().unwrap();
    assert_relative_eq!(m.volume, 500.0, epsilon = 1e-6);
    assert!(m.bbox_min[2] > 4.99);

    let mirrored = b.mirror([0.0; 3], [1.0, 0.0, 0.0]).unwrap();
    let m = mirrored.mass().unwrap();
    assert_relative_eq!(m.volume, 1000.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[0], 0.0, epsilon = 1e-6);

    let rotated = b.rotate(Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] }, PI / 2.0).unwrap();
    let m = rotated.mass().unwrap();
    assert_relative_eq!(m.center[0], -5.0, epsilon = 1e-9);
    assert_relative_eq!(m.center[1], 5.0, epsilon = 1e-9);

    // Escala no uniforme cae en la transformación general
    let stretched = b.transform([[2.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]).unwrap();
    assert_relative_eq!(stretched.mass().unwrap().volume, 2000.0, max_relative = 1e-6);
}

#[test]
fn draft_faces() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let sides: Vec<usize> = b.faces().unwrap().iter().enumerate()
        .filter(|(_, f)| f.normal[2].abs() < 0.1).map(|(i, _)| i).collect();
    let d = b.draft(&sides, [0.0, 0.0, 1.0], 5f64.to_radians(), [0.0; 3], [0.0, 0.0, 1.0]).unwrap();
    assert!(d.is_valid());
    let v = d.mass().unwrap().volume;
    assert!((v - 1000.0).abs() > 1.0, "el desmolde cambia el volumen: {v}");
}

#[test]
fn tessellation() {
    if !require() {
        return;
    }
    let c = Shape::cylinder(Frame::WORLD, 5.0, 10.0).unwrap();
    let t = c.tessellate(0.05, 0.3).unwrap();
    assert!(t.triangles.len() > 20);
    assert_eq!(t.positions.len(), t.normals.len());
    assert_eq!(t.triangle_face.len(), t.triangles.len());
    assert!(t.triangle_face.iter().all(|&f| (f as usize) < c.face_count()));
    assert_eq!(t.edges.len(), c.edge_count());
    let center = [0.0, 0.0, 5.0];
    let mut outward = 0;
    for tri in &t.triangles {
        let [a, b, cc] = tri.map(|i| t.positions[i as usize]);
        // Normal geométrica coherente con la de los vértices y saliente
        let u = sub(b, a);
        let v = sub(cc, a);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let vn = t.normals[tri[0] as usize];
        assert!(dot(n, vn) > 0.0, "orientación incoherente");
        let mid = [(a[0] + b[0] + cc[0]) / 3.0, (a[1] + b[1] + cc[1]) / 3.0, (a[2] + b[2] + cc[2]) / 3.0];
        if dot(n, sub(mid, center)) > 0.0 {
            outward += 1;
        }
    }
    assert_eq!(outward, t.triangles.len());
    for n in &t.normals {
        assert_relative_eq!(dot(*n, *n), 1.0, epsilon = 1e-6);
    }
    // Un círculo de radio 5 con deflexión 0.05 tiene bastantes puntos
    assert!(t.edges.iter().map(|e| e.len()).max().unwrap() > 16);
}

#[test]
fn step_and_brep_roundtrip() {
    if !require() {
        return;
    }
    let b = cube(10.0).cut(&Shape::sphere([10.0, 10.0, 10.0], 4.0).unwrap()).unwrap();
    let v = b.mass().unwrap().volume;
    let step = b.to_step().unwrap();
    assert!(String::from_utf8_lossy(&step[..64]).contains("ISO-10303-21"));
    let back = Shape::from_step(&step).unwrap();
    assert_relative_eq!(back.mass().unwrap().volume, v, max_relative = 1e-6);
    assert_eq!(back.face_count(), b.face_count());

    let brep = b.to_brep().unwrap();
    let back = Shape::from_brep(&brep).unwrap();
    assert_relative_eq!(back.mass().unwrap().volume, v, epsilon = 1e-9);

    assert!(Shape::from_step(b"esto no es un STEP").is_err());
}

#[test]
fn mesh_to_solid() {
    if !require() {
        return;
    }
    // Tetraedro con caras salientes
    let v = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]];
    let t = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let s = Shape::from_mesh(&v, &t, 1e-6).unwrap();
    assert_eq!(s.kind(), ShapeKind::Solid);
    assert_relative_eq!(s.mass().unwrap().volume.abs(), 1000.0 / 6.0, epsilon = 1e-6);
}

#[test]
fn clone_is_independent_handle() {
    if !require() {
        return;
    }
    let a = cube(2.0);
    let b = a.clone();
    drop(a);
    assert_relative_eq!(b.mass().unwrap().volume, 8.0, epsilon = 1e-9);
    assert!(!occt_version().is_empty());
}

#[test]
fn closest_face_and_edge() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let faces = b.faces().unwrap();
    // Punto apenas sobre la tapa: la tapa, también filtrando por normal
    let (top, d) = b.closest_face([3.0, 4.0, 10.5], None, 0.0).unwrap();
    assert!(faces[top].normal[2] > 0.99);
    assert_relative_eq!(d, 0.5, epsilon = 1e-9);
    // Cerca de la arista superior frontal pero pidiendo normal −Y: la cara frontal
    let (front, _) = b.closest_face([5.0, 0.1, 9.9], Some([0.0, -1.0, 0.0]), 0.9).unwrap();
    assert!(faces[front].normal[1] < -0.99);
    // Arista paralela a X más cercana a (5, 0, 10)
    let (e, d) = b.closest_edge([5.0, -0.2, 10.0], Some([1.0, 0.0, 0.0]), 0.9).unwrap();
    let info = b.edge_info(e).unwrap();
    assert_relative_eq!(info.mid[2], 10.0, epsilon = 1e-9);
    assert_relative_eq!(info.mid[1], 0.0, epsilon = 1e-9);
    assert_relative_eq!(d, 0.2, epsilon = 1e-9);
    // Ninguna arista paralela a una dirección imposible
    assert!(b.closest_edge([5.0, 0.0, 10.0], Some([1.0, 1.0, 0.0]), 0.99).is_none());
}

#[test]
fn history_tracks_faces_through_operations() {
    if !require() {
        return;
    }
    let b = cube(10.0);
    let top = b.faces().unwrap().iter().position(|f| f.normal[2] > 0.99).unwrap();
    // Agujero que atraviesa la tapa: la tapa sigue siendo una cara (modificada)
    let tool = Shape::cylinder(Frame::at([5.0, 5.0, -1.0]), 2.0, 12.0).unwrap();
    let (cut, h) = with_history(|| b.cut(&tool)).unwrap();
    assert_eq!(h.images.len(), b.face_count() + tool.face_count());
    assert_eq!(h.images[top].len(), 1);
    let new_top = h.images[top][0];
    assert!(cut.face_info(new_top).unwrap().normal[2] > 0.99);
    // El lateral del cilindro termina como la pared del agujero
    let side = tool.faces().unwrap().iter().position(|f| f.surface == SurfaceKind::Cylinder).unwrap();
    let wall = &h.images[b.face_count() + side];
    assert_eq!(wall.len(), 1);
    assert_eq!(cut.face_info(wall[0]).unwrap().surface, SurfaceKind::Cylinder);

    // Redondeo: las caras generadas por la arista elegida van al final
    let e = (0..cut.edge_count()).find(|&e| {
        let i = cut.edge_info(e).unwrap();
        i.curve == CurveKind::Line && (i.mid[2] - 10.0).abs() < 1e-9 && i.mid[1].abs() < 1e-9
    }).unwrap();
    let (f, h) = with_history(|| cut.fillet(&[e], 1.0)).unwrap();
    assert_eq!(h.images.len(), cut.face_count() + 1);
    let generated = &h.images[cut.face_count()];
    assert_eq!(generated.len(), 1);
    assert_eq!(f.face_info(generated[0]).unwrap().surface, SurfaceKind::Cylinder);

    // Transformar: cada cara tiene su copia
    let (_, h) = with_history(|| b.translate([1.0, 0.0, 0.0])).unwrap();
    assert!(h.images.iter().all(|v| v.len() == 1));
}

#[test]
fn ellipse_faces_have_exact_area() {
    if !require() {
        return;
    }
    // Eje mayor en x (a = 5, b = 2) y con b > a (se gira a 90° por dentro)
    for (a, b) in [(5.0, 2.0), (2.0, 5.0)] {
        let face = Shape::face(&[vec![Curve::Ellipse { center: [1.0, 2.0, 0.0], normal: [0.0, 0.0, 1.0], major: [1.0, 0.0, 0.0], a, b }]]).unwrap();
        let m = face.mass().unwrap();
        assert_relative_eq!(m.area, PI * a * b, epsilon = 1e-6);
        // El semieje `a` sigue en x aunque sea el menor
        assert_relative_eq!(m.bbox_max[0] - m.bbox_min[0], 2.0 * a, epsilon = 1e-3);
    }
}

#[test]
fn spline_end_tangents_change_the_shape() {
    if !require() {
        return;
    }
    // Arco de spline por tres puntos, cerrado con una línea: con las tangentes
    // abiertas hacia afuera el área crece
    let pts = vec![[0.0, 0.0, 0.0], [5.0, 3.0, 0.0], [10.0, 0.0, 0.0]];
    let close = Curve::Line([10.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let free = Shape::face(&[vec![Curve::Spline(pts.clone()), close.clone()]]).unwrap();
    let wide = Shape::face(&[vec![Curve::SplineEnds { points: pts, start: [0.0, 1.0, 0.0], end: [0.0, -1.0, 0.0] }, close]]).unwrap();
    let (a0, a1) = (free.mass().unwrap().area, wide.mass().unwrap().area);
    assert!(a1 > a0 * 1.05, "{a0} → {a1}");
}

#[test]
fn min_distance_between_faces_edges_and_points() {
    if !cad_occt::available() {
        return;
    }
    let b = Shape::make_box(Frame::at([0.0; 3]), 10.0, 20.0, 30.0).unwrap();
    let face_with_normal = |n: [f64; 3]| {
        (0..b.face_count()).find(|&i| {
            let f = b.face_info(i).unwrap();
            (0..3).all(|k| (f.normal[k] - n[k]).abs() < 1e-9)
        })
    };
    let bottom = b.face_shape(face_with_normal([0.0, 0.0, -1.0]).unwrap()).unwrap();
    let top = b.face_shape(face_with_normal([0.0, 0.0, 1.0]).unwrap()).unwrap();
    let (d, p, q) = bottom.min_distance(&top).unwrap();
    assert!((d - 30.0).abs() < 1e-9, "{d}");
    assert!((p[2] - 0.0).abs() < 1e-9 && (q[2] - 30.0).abs() < 1e-9);
    // Un punto afuera: distancia a la cara de arriba
    let v = Shape::vertex([5.0, 10.0, 40.0]).unwrap();
    assert!((v.min_distance(&top).unwrap().0 - 10.0).abs() < 1e-9);
    // Vértices y aristas como formas propias
    assert_eq!(b.vertex_count(), 8);
    let vs = b.vertices().unwrap();
    assert!(vs.iter().any(|p| p == &[10.0, 20.0, 30.0]));
    let e = b.edge_shape(0).unwrap();
    assert!(e.min_distance(&b).unwrap().0 < 1e-9);
}

#[test]
fn principal_moments_of_a_box() {
    if !cad_occt::available() {
        return;
    }
    // Caja 10×20×30: I = V/12 (b² + c²) respecto a cada eje por el centro
    let m = Shape::make_box(Frame::at([0.0; 3]), 10.0, 20.0, 30.0).unwrap().mass().unwrap();
    let v = 6000.0;
    let mut got = m.inertia.to_vec();
    got.sort_by(f64::total_cmp);
    let mut want = vec![v / 12.0 * (400.0 + 900.0), v / 12.0 * (100.0 + 900.0), v / 12.0 * (100.0 + 400.0)];
    want.sort_by(f64::total_cmp);
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() < 1e-6 * w, "{got:?} vs {want:?}");
    }
    assert!((m.center[2] - 15.0).abs() < 1e-9);
}

#[test]
fn solids_of_a_compound_keep_their_faces() {
    if !cad_occt::available() {
        return;
    }
    let a = Shape::make_box(Frame::at([0.0; 3]), 10.0, 10.0, 10.0).unwrap();
    let b = Shape::make_box(Frame::at([20.0, 0.0, 0.0]), 10.0, 10.0, 10.0).unwrap();
    let both = a.union(&b).unwrap();
    let solids = both.solids().unwrap();
    assert_eq!(solids.len(), 2);
    let mut seen = vec![false; both.face_count()];
    for s in &solids {
        assert_eq!(s.face_count(), 6);
        for i in both.face_indices_of(s).unwrap() {
            let i = i.expect("cada cara del sólido está en el compuesto");
            assert!(!seen[i]);
            seen[i] = true;
        }
    }
    assert!(seen.iter().all(|&x| x));
}
