//! Plato giratorio con láser: modelo de cámara, geometría del rig, extracción
//! de la línea y escaneo sintético completo medido contra la forma real.

use orizon3d_core::camera::RgbFrame;
use orizon3d_core::turntable::laser::extract_vertical_line;
use orizon3d_core::turntable::synth::{accuracy, sample_object, MotionErrors, SynthRig, SynthScene};
use orizon3d_core::turntable::{
    run_laser_scan, Calibration, CameraModel, FrameSource, LineSettings, Rig, RigGeometry, ScanPlan, TriangulateSettings, Vec3,
};

#[test]
fn lens_projects_back_to_the_same_pixel() {
    let mut cam = CameraModel::from_hfov(1920, 1080, 70.0);
    (cam.k1, cam.k2, cam.p1, cam.p2) = (-0.12, 0.03, 0.001, -0.0005);
    for &(u, v) in &[(0.0, 0.0), (959.5, 539.5), (1919.0, 1079.0), (100.3, 900.7)] {
        let (pu, pv) = cam.project(&(cam.ray(u, v) * 250.0)).unwrap();
        assert!((pu - u).abs() < 1e-6 && (pv - v).abs() < 1e-6, "({u}, {v}) → ({pu}, {pv})");
    }
}

#[test]
fn camera_on_the_arc_aims_at_the_pivot() {
    let rig = RigGeometry::webcam_1080p();
    for e in [0.0, 30.0, 60.0, 85.0] {
        let view = rig.view(e);
        let pivot = view.cam_from_world * nalgebra::Point3::from(rig.arc_pivot);
        let (u, v) = rig.camera.project(&pivot.coords).unwrap();
        assert!((u - rig.camera.cx).abs() < 1e-6 && (v - rig.camera.cy).abs() < 1e-6, "arco a {e}°: ({u}, {v})");
        assert!((view.camera_center() - rig.arc_pivot).norm() - rig.arc_radius < 1e-9);
        // Arriba en el mundo es arriba en la imagen (v menor)
        let above = view.cam_from_world * nalgebra::Point3::from(rig.arc_pivot + Vec3::z() * 5.0);
        assert!(rig.camera.project(&above.coords).unwrap().1 < v);
    }
    for m in &rig.lasers {
        let plane = m.plane();
        assert!(plane.distance(&m.origin).abs() < 1e-9 && plane.distance(&m.target).abs() < 1e-9);
        assert!(plane.normal.z.abs() < 1e-9, "línea vertical: plano vertical");
    }
}

#[test]
fn lasers_on_the_arm_move_rigidly_with_the_camera() {
    let rig = RigGeometry::webcam_1080p();
    assert!(rig.lasers_on_arm);
    let at_zero = rig.lasers_at(0.0);
    for e in [20.0, 55.0] {
        let arm = rig.arm(e);
        let camera = arm * rig.world_from_camera(0.0);
        let expected = rig.world_from_camera(e);
        assert!((camera.translation.vector - expected.translation.vector).norm() < 1e-9);
        assert!(camera.rotation.angle_to(&expected.rotation) < 1e-9);
        // Visto desde la cámara, cada plano de láser es el mismo a toda altura
        for ((p0, _), (pe, origin)) in at_zero.iter().zip(rig.lasers_at(e)) {
            let seen0 = p0.transformed(&rig.world_from_camera(0.0).inverse());
            let seen = pe.transformed(&expected.inverse());
            assert!((seen0.normal - seen.normal).norm() < 1e-9 && (seen0.offset - seen.offset).abs() < 1e-9);
            assert!(pe.distance(&origin).abs() < 1e-9);
        }
    }
}

/// Imagen negra con una línea roja gaussiana en la columna `u0 + slope·v`
fn line_image(w: u32, h: u32, u0: f64, slope: f64, sigma: f64) -> RgbFrame {
    let mut rgb = vec![0u8; (w * h * 3) as usize];
    for v in 0..h {
        let c = u0 + slope * v as f64;
        for u in 0..w {
            let x = (u as f64 - c) / sigma;
            rgb[((v * w + u) * 3) as usize] = (220.0 * (-x * x / 2.0).exp()).round() as u8;
        }
    }
    RgbFrame { width: w, height: h, rgb }
}

#[test]
fn line_is_found_to_a_fraction_of_a_pixel() {
    let img = line_image(200, 60, 80.3, 0.17, 1.6);
    let line = extract_vertical_line(&img, None, &LineSettings::default());
    assert!(line.len() >= 50, "{} filas", line.len());
    let worst = line.iter().map(|p| (p.u - (80.3 + 0.17 * p.v)).abs()).fold(0.0, f64::max);
    assert!(worst < 0.05, "error {worst} px");
}

#[test]
fn reflections_and_wide_blobs_are_rejected() {
    let mut img = line_image(200, 40, 60.0, 0.0, 1.5);
    // Reflejo casi tan fuerte como la línea en las filas 10..20
    for v in 10..20 {
        for u in 140..143 {
            img.rgb[(v * 200 + u) * 3] = 200;
        }
    }
    let line = extract_vertical_line(&img, None, &LineSettings::default());
    assert!(line.iter().all(|p| !(10.0..20.0).contains(&p.v)), "filas ambiguas aceptadas");
    assert!(line.iter().all(|p| (p.u - 60.0).abs() < 0.05));
}

fn scan(calib_error: impl Fn(&mut Calibration)) -> orizon3d_core::turntable::synth::Accuracy {
    let mut rig = RigGeometry::webcam_1080p();
    rig.camera = rig.camera.scaled(480, 270);
    let elevations = [0.0, 45.0];
    let mut calib = rig.calibration(&elevations);
    calib_error(&mut calib);
    let mut synth = SynthRig::new(SynthScene::new(sample_object(), rig), MotionErrors::default());
    let mut camera = synth.camera();
    let plan = ScanPlan { elevations: elevations.to_vec(), steps: 24, ..Default::default() };
    let cloud = run_laser_scan(
        &mut synth as &mut dyn Rig,
        &mut camera as &mut dyn FrameSource,
        &calib,
        &plan,
        &LineSettings::default(),
        &TriangulateSettings::default(),
        |_, _| true,
    )
    .unwrap();
    assert!(cloud.has_color && cloud.points.iter().any(|p| p.rgb != [0, 0, 0]));
    accuracy(&cloud, &synth.scene().object)
}

#[test]
fn synthetic_laser_scan_matches_the_true_shape() {
    let acc = scan(|_| {});
    assert!(acc.points > 3000, "{acc:?}");
    assert!(acc.mean < 0.1 && acc.rms < 0.2, "{acc:?}");
}

#[test]
fn a_misplaced_plate_axis_shows_up_in_the_error() {
    let good = scan(|_| {});
    let bad = scan(|c| c.plate.point.x += 1.0);
    assert!(bad.mean > good.mean * 3.0, "bien {good:?} / mal {bad:?}");
}

mod scanear {
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};
    use std::time::Duration;

    use orizon3d_core::turntable::scanear::{parse_reply, Reply, ScanEarRig, Status, Timing};
    use orizon3d_core::turntable::Rig;

    /// Arduino simulado con el protocolo de scanEar: los movimientos tardan
    /// unas consultas de estado y no hay comandos de láser
    #[derive(Default)]
    struct FakeArduino {
        pending: Vec<u8>,
        out: VecDeque<u8>,
        y: f64,
        z: f64,
        target: (f64, f64),
        busy: u32,
        log: Vec<String>,
    }

    impl FakeArduino {
        fn handle(&mut self, cmd: &str) {
            self.log.push(cmd.to_string());
            let reply = if cmd == "PY:GET_STATUS" {
                if self.busy > 0 {
                    self.busy -= 1;
                    // En camino: ángulos intermedios
                    self.y = (self.y + self.target.0) / 2.0;
                    self.z = (self.z + self.target.1) / 2.0;
                } else {
                    (self.y, self.z) = self.target;
                }
                let state = if self.busy > 0 { 1 } else { 0 };
                format!(r#"STATUS:{{"state":{state},"y_angle":{},"z_angle":{},"z_step":0,"total_z_steps":0}}"#, self.y, self.z)
            } else if let Some(a) = cmd.strip_prefix("PY:MOVE_Y:") {
                self.target.0 = a.parse().unwrap();
                self.busy = 3;
                r#"OK:{"cmd":"MOVE_Y"}"#.to_string()
            } else if let Some(a) = cmd.strip_prefix("PY:MOVE_Z:") {
                self.target.1 = a.parse().unwrap();
                self.busy = 3;
                r#"OK:{"cmd":"MOVE_Z"}"#.to_string()
            } else if cmd == "PY:RESET" {
                self.target = (0.0, 0.0);
                self.busy = 2;
                "OK:{}".to_string()
            } else {
                r#"ERROR:{"message":"Comando desconocido"}"#.to_string()
            };
            self.out.extend(format!("{reply}\r\n").bytes());
        }
    }

    impl Write for FakeArduino {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.pending.extend_from_slice(buf);
            while let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=i).collect();
                self.handle(String::from_utf8_lossy(&line).trim());
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for FakeArduino {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.out.len());
            for (b, x) in buf.iter_mut().zip(self.out.drain(..n)) {
                *b = x;
            }
            Ok(n)
        }
    }

    fn rig() -> ScanEarRig<FakeArduino> {
        let mut rig = ScanEarRig::new(FakeArduino::default());
        rig.timing = Timing { poll: Duration::ZERO, settle: Duration::ZERO, ..Timing::default() };
        rig
    }

    #[test]
    fn replies_are_parsed() {
        assert_eq!(
            parse_reply(r#"STATUS:{"state":1,"y_angle":12.5,"z_angle":30,"z_step":2,"total_z_steps":9}"#),
            Reply::Status(Status { state: 1, y_angle: 12.5, z_angle: 30.0 })
        );
        assert_eq!(parse_reply(r#"ERROR:{"message":"fuera de rango"}"#), Reply::Error("fuera de rango".into()));
        assert_eq!(parse_reply(r#"OK:{"cmd":"MOVE_Y"}"#), Reply::Ok);
        assert_eq!(parse_reply("fotografiar"), Reply::Other("fotografiar".into()));
    }

    #[test]
    fn moves_wait_until_the_rig_is_still_at_the_target() {
        let mut rig = rig();
        rig.home().unwrap();
        rig.move_to(370.0, 45.0).unwrap();
        let s = rig.status().unwrap();
        assert_eq!((s.state, s.y_angle, s.z_angle), (0, 10.0, 45.0));
        assert!(rig.move_to(0.0, 140.0).is_err(), "el arco llega a 135°");
    }

    #[test]
    fn missing_laser_commands_are_reported() {
        let mut rig = rig();
        let err = rig.set_laser(0, true).unwrap_err();
        assert!(err.to_string().contains("Comando desconocido"), "{err}");
    }
}
