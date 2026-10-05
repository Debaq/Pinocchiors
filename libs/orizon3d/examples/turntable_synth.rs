//! Escaneo láser sintético con el rig de plato giratorio: renderiza los cuadros
//! de la webcam, los pasa por la misma cadena que usará el hardware y mide el
//! error contra la forma verdadera.
//!
//! cargo run --release -p orizon3d-core --example turntable_synth -- [opciones]
//!   --stl modelo.stl   malla a escanear (por defecto, el objeto de prueba)
//!   --size 60          lado mayor de la malla en mm
//!   --width 960        ancho de la imagen (la webcam da 1920)
//!   --steps 90         paradas del plato por vuelta
//!   --elev 0,30,60     alturas del arco
//!   --axis-error 0     error del eje del plato en la calibración (mm)
//!   --jitter 0         σ del error de posición del plato (grados)
//!   --elev-error 0     error fijo del arco al moverse (grados)
//!   --lasers arm       láseres en el brazo (suben con la cámara) o en la base
//!   --frames dir       guarda cuadros de muestra en PNG
//!   --out nube.ply     guarda la nube

use std::path::PathBuf;
use std::time::Instant;

use orizon3d_core::turntable::synth::{accuracy, coverage, sample_object, Hit, MotionErrors, RigState, Surface, SynthRig, SynthScene};
use orizon3d_core::turntable::{run_laser_scan, Calibration, FrameSource, LineSettings, Rig, RigGeometry, ScanPlan, TriangulateSettings, Vec3};
use pinocchio_math::Vector3;
use pinocchio_spatial::{Bvh, Triangle};

/// Malla STL apoyada en el plato y centrada en el eje
struct MeshSurface {
    bvh: Bvh,
    bounds: (Vec3, f64),
}

fn v(p: &Vec3) -> Vector3 {
    Vector3::new(p.x, p.y, p.z)
}

impl Surface for MeshSurface {
    fn hit(&self, origin: &Vec3, dir: &Vec3, t_max: f64) -> Option<Hit> {
        let (t, i) = self.bvh.ray_hit(&v(origin), &v(dir), 1e-6, t_max)?;
        let n = self.bvh.triangle(i).normal();
        let mut normal = Vec3::new(n.x(), n.y(), n.z()).normalize();
        if normal.dot(dir) > 0.0 {
            normal = -normal;
        }
        Some(Hit { t, normal })
    }

    fn distance(&self, p: &Vec3) -> f64 {
        self.bvh.query_distance(&v(p))
    }

    fn bounds(&self) -> (Vec3, f64) {
        self.bounds
    }
}

fn load_stl(path: &PathBuf, size: f64) -> MeshSurface {
    let mut file = std::fs::File::open(path).expect("no se pudo abrir el STL");
    let stl = stl_io::read_stl(&mut file).expect("STL inválido");
    let verts: Vec<Vec3> = stl.vertices.iter().map(|p| Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();
    let (mut lo, mut hi) = (Vec3::repeat(f64::MAX), Vec3::repeat(f64::MIN));
    for p in &verts {
        lo = lo.inf(p);
        hi = hi.sup(p);
    }
    let scale = size / (hi - lo).max();
    let shift = Vec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
    let place = |p: &Vec3| (p - shift) * scale;
    let tris = stl
        .faces
        .iter()
        .map(|f| {
            let [a, b, c] = f.vertices.map(|i| v(&place(&verts[i])));
            Triangle::new(a, b, c)
        })
        .collect();
    let (lo, hi) = (place(&lo), place(&hi));
    MeshSurface { bvh: Bvh::build(tris), bounds: ((lo + hi) / 2.0, (hi - lo).norm() / 2.0) }
}

fn save_png(frame: &orizon3d_core::camera::RgbFrame, path: PathBuf) {
    image::save_buffer(&path, &frame.rgb, frame.width, frame.height, image::ExtendedColorType::Rgb8).expect("no se pudo guardar el PNG");
    println!("  {}", path.display());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let num = |name: &str, default: f64| opt(name).map_or(default, |s| s.parse().unwrap());
    let width = num("--width", 960.0) as u32;
    let steps = num("--steps", 90.0) as u32;
    let elevations: Vec<f64> =
        opt("--elev").map_or(vec![0.0, 30.0, 60.0], |s| s.split(',').map(|x| x.parse().unwrap()).collect());

    let mut rig = RigGeometry::webcam_1080p();
    rig.camera = rig.camera.scaled(width, width * 9 / 16);
    if opt("--lasers").as_deref() == Some("base") {
        rig.lasers = RigGeometry::arm_lasers(rig.arc_azimuth, 30.0, 150.0, 40.0);
        rig.lasers_on_arm = false;
    }
    let calib_truth = rig.calibration(&elevations);
    // La calibración que usa la reconstrucción puede estar errada a propósito
    let mut calib: Calibration = calib_truth.clone();
    calib.plate.point.x += num("--axis-error", 0.0);

    match opt("--stl") {
        Some(path) => run(load_stl(&PathBuf::from(path), num("--size", 60.0)), rig, calib, steps, &elevations, &args),
        None => run(sample_object(), rig, calib, steps, &elevations, &args),
    }
}

fn run<S: Surface + 'static>(object: S, rig: RigGeometry, calib: Calibration, steps: u32, elevations: &[f64], args: &[String]) {
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let jitter = opt("--jitter").map_or(0.0, |s| s.parse().unwrap());
    let elevation_offset = opt("--elev-error").map_or(0.0, |s| s.parse().unwrap());
    let lasers = rig.lasers.len();
    let mut synth = SynthRig::new(SynthScene::new(object, rig), MotionErrors { plate_jitter: jitter, elevation_offset, ..Default::default() });
    let mut camera = synth.camera();

    if let Some(dir) = opt("--frames") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        println!("Cuadros de muestra:");
        for &e in elevations {
            for (name, light, laser) in [("color", 1.0, None), ("laser0", 0.0, Some(0)), ("laser1", 0.0, Some(1))] {
                let state = RigState {
                    plate: 30.0,
                    elevation: e,
                    lasers: (0..lasers).map(|i| Some(i) == laser).collect(),
                    light,
                };
                save_png(&synth.scene().render(&state, 1), dir.join(format!("e{e}_{name}.png")));
            }
        }
    }

    let plan = ScanPlan { elevations: elevations.to_vec(), steps, ..Default::default() };
    let started = Instant::now();
    let cloud = run_laser_scan(
        &mut synth as &mut dyn Rig,
        &mut camera as &mut dyn FrameSource,
        &calib,
        &plan,
        &LineSettings::default(),
        &TriangulateSettings::default(),
        |done, total| {
            if done % 30 == 0 || done == total {
                eprint!("\r  {done}/{total} paradas");
            }
            true
        },
    )
    .expect("escaneo");
    eprintln!();
    let acc = accuracy(&cloud, &synth.scene().object);
    let cover = coverage(&cloud, &synth.scene().object, 2.0, TriangulateSettings::default().min_height);
    println!(
        "{} puntos en {:.1} s · cobertura {:.1} % · error medio {:.3} mm · rms {:.3} · p95 {:.3} · máx {:.3}",
        acc.points,
        started.elapsed().as_secs_f64(),
        cover * 100.0,
        acc.mean,
        acc.rms,
        acc.p95,
        acc.max
    );
    if let Some(out) = opt("--out") {
        cloud.export_ply(std::path::Path::new(&out)).expect("PLY");
        println!("  {out}");
    }
}
