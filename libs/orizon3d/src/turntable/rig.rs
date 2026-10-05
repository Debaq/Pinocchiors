//! Geometría del rig: eje del plato, arco de la cámara y planos de los láseres.
//!
//! [`RigGeometry`] describe el rig físico (lo que se mide con regla): genera
//! las poses del render sintético y sirve de valor inicial a la calibración.
//! [`Calibration`] es lo que usa la reconstrucción: una [`View`] por cada
//! altura del arco, con la pose de la cámara y los planos de láser que ve.

use nalgebra::{Isometry3, Matrix3, Rotation3, Translation3, Unit, UnitQuaternion};

use super::Vec3;
use super::lens::CameraModel;

/// Recta de giro (el eje del plato)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub point: Vec3,
    /// Unitario, apunta hacia arriba
    pub dir: Vec3,
}

impl Axis {
    pub fn vertical() -> Self {
        Axis { point: Vec3::zeros(), dir: Vec3::z() }
    }

    /// Mundo ← objeto con el plato girado `deg` grados (antihorario visto
    /// desde arriba)
    pub fn world_from_object(&self, deg: f64) -> Isometry3<f64> {
        let rot = UnitQuaternion::from_axis_angle(&Unit::new_normalize(self.dir), deg.to_radians());
        let a = Translation3::from(self.point);
        a * Isometry3::from_parts(Translation3::identity(), rot) * a.inverse()
    }

    /// Altura de `p` sobre el plato, medida a lo largo del eje
    pub fn height(&self, p: &Vec3) -> f64 {
        (p - self.point).dot(&self.dir)
    }

    /// Distancia de `p` al eje
    pub fn radius(&self, p: &Vec3) -> f64 {
        let d = p - self.point;
        (d - self.dir * d.dot(&self.dir)).norm()
    }
}

/// Plano `normal·p = offset` (normal unitaria)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub offset: f64,
}

impl Plane {
    pub fn from_point_normal(point: &Vec3, normal: &Vec3) -> Self {
        let n = normal.normalize();
        Plane { normal: n, offset: n.dot(point) }
    }

    pub fn distance(&self, p: &Vec3) -> f64 {
        self.normal.dot(p) - self.offset
    }

    /// `t` del corte del rayo `origin + t·dir`; `None` si es casi paralelo
    pub fn intersect(&self, origin: &Vec3, dir: &Vec3) -> Option<f64> {
        let den = self.normal.dot(dir);
        (den.abs() > 1e-9).then(|| (self.offset - self.normal.dot(origin)) / den)
    }

    pub fn transformed(&self, iso: &Isometry3<f64>) -> Plane {
        let n = iso.rotation * self.normal;
        let p = iso * nalgebra::Point3::from(self.normal * self.offset);
        Plane::from_point_normal(&p.coords, &n)
    }
}

/// Una altura del arco ya calibrada
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// Ángulo del arco (grados) con que se pide al rig
    pub elevation: f64,
    pub cam_from_world: Isometry3<f64>,
    /// Plano de cada láser en el marco del mundo, en el orden del rig
    pub lasers: Vec<Plane>,
}

impl View {
    pub fn camera_center(&self) -> Vec3 {
        self.cam_from_world.inverse().translation.vector
    }
}

/// Lo que la reconstrucción necesita saber del rig
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub camera: CameraModel,
    pub plate: Axis,
    pub views: Vec<View>,
}

impl Calibration {
    /// La vista calibrada para un ángulo del arco
    pub fn view(&self, elevation: f64) -> Option<&View> {
        self.views.iter().find(|v| (v.elevation - elevation).abs() < 1e-6)
    }

    /// La misma calibración para otra resolución de la cámara
    pub fn scaled(&self, width: u32, height: u32) -> Calibration {
        Calibration { camera: self.camera.scaled(width, height), ..self.clone() }
    }
}

/// Láser de línea: emite desde `origin` hacia `target` y abre un abanico
/// vertical (girado `roll` grados alrededor del haz). En el brazo, las
/// coordenadas son las del mundo con el arco en 0°
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserMount {
    pub origin: Vec3,
    pub target: Vec3,
    pub roll: f64,
}

impl LaserMount {
    /// Láser apuntado al eje del plato desde el acimut `azimuth` (grados desde
    /// +X), a `radius` mm del eje y `height` mm sobre el plato
    pub fn aimed(azimuth: f64, radius: f64, height: f64) -> Self {
        let (s, c) = azimuth.to_radians().sin_cos();
        LaserMount { origin: Vec3::new(radius * c, radius * s, height), target: Vec3::new(0.0, 0.0, height), roll: 0.0 }
    }


    pub fn plane(&self) -> Plane {
        let beam = (self.target - self.origin).normalize();
        let fan = Rotation3::from_axis_angle(&Unit::new_normalize(beam), self.roll.to_radians()) * Vec3::z();
        Plane::from_point_normal(&self.origin, &beam.cross(&fan))
    }
}

/// Rig físico: plato, arco de la cámara y láseres
#[derive(Debug, Clone, PartialEq)]
pub struct RigGeometry {
    pub camera: CameraModel,
    pub plate: Axis,
    pub plate_radius: f64,
    /// Centro de giro del arco
    pub arc_pivot: Vec3,
    /// Distancia del centro óptico al centro del arco
    pub arc_radius: f64,
    /// Dirección (grados desde +X) en que está la cámara vista desde arriba
    pub arc_azimuth: f64,
    /// Inclinación extra de la cámara respecto de apuntar al centro del arco
    /// (grados, positiva = mira más arriba): la que dará el servo
    pub camera_tilt: f64,
    /// Giro de montaje de la cámara alrededor de su eje óptico (grados)
    pub camera_roll: f64,
    pub lasers: Vec<LaserMount>,
    /// Los láseres suben con la cámara (montados en el brazo del arco) en
    /// vez de quedar fijos en la base
    pub lasers_on_arm: bool,
}

impl RigGeometry {
    /// Rig de referencia con webcam 1080p: arco de 120 mm, dos láseres de
    /// línea vertical a ±30° de la cámara. Las medidas no dichas son supuestas
    pub fn webcam_1080p() -> Self {
        let mut camera = CameraModel::from_hfov(1920, 1080, 70.0);
        camera.k1 = -0.08;
        camera.k2 = 0.02;
        let arc_azimuth = -90.0;
        RigGeometry {
            camera,
            plate: Axis::vertical(),
            plate_radius: 60.0,
            arc_pivot: Vec3::new(0.0, 0.0, 30.0),
            arc_radius: 120.0,
            arc_azimuth,
            camera_tilt: 0.0,
            camera_roll: 0.0,
            lasers: Self::arm_lasers(arc_azimuth, 30.0, 120.0, 30.0),
            lasers_on_arm: true,
        }
    }

    /// Dos láseres a ±`spread` grados de la cámara, a `radius` mm del eje y
    /// `height` mm de altura, apuntados al eje
    pub fn arm_lasers(arc_azimuth: f64, spread: f64, radius: f64, height: f64) -> Vec<LaserMount> {
        vec![
            LaserMount::aimed(arc_azimuth + spread, radius, height),
            LaserMount::aimed(arc_azimuth - spread, radius, height),
        ]
    }

    /// Mundo ← brazo: el giro del arco alrededor de su eje
    pub fn arm(&self, elevation: f64) -> Isometry3<f64> {
        let (s, c) = self.arc_azimuth.to_radians().sin_cos();
        let axis = Unit::new_normalize(Vec3::new(c, s, 0.0).cross(&Vec3::z()));
        let pivot = Translation3::from(self.arc_pivot);
        pivot * Isometry3::from_parts(Translation3::identity(), UnitQuaternion::from_axis_angle(&axis, elevation.to_radians())) * pivot.inverse()
    }

    /// Plano y origen (en el mundo) de cada láser con el arco en `elevation`
    pub fn lasers_at(&self, elevation: f64) -> Vec<(Plane, Vec3)> {
        let arm = if self.lasers_on_arm { self.arm(elevation) } else { Isometry3::identity() };
        self.lasers
            .iter()
            .map(|m| (m.plane().transformed(&arm), (arm * nalgebra::Point3::from(m.origin)).coords))
            .collect()
    }

    /// Mundo ← cámara con el arco en `elevation` grados
    pub fn world_from_camera(&self, elevation: f64) -> Isometry3<f64> {
        let (s, c) = self.arc_azimuth.to_radians().sin_cos();
        let out = Vec3::new(c, s, 0.0);
        // El eje del arco es horizontal y perpendicular a la dirección de la
        // cámara: es también el eje X de la cámara (la derecha de la imagen)
        let right = Vec3::z().cross(&out);
        let (se, ce) = elevation.to_radians().sin_cos();
        let center = self.arc_pivot + (out * ce + Vec3::z() * se) * self.arc_radius;
        let forward = -(out * ce + Vec3::z() * se);
        let down = forward.cross(&right);
        let base = Rotation3::from_matrix_unchecked(Matrix3::from_columns(&[right, down, forward]));
        // Inclinación (servo) alrededor de X y giro de montaje alrededor de Z,
        // ambos en el marco de la cámara
        let tilt = Rotation3::from_axis_angle(&Vec3::x_axis(), self.camera_tilt.to_radians());
        let roll = Rotation3::from_axis_angle(&Vec3::z_axis(), self.camera_roll.to_radians());
        let rot = UnitQuaternion::from_rotation_matrix(&(base * tilt * roll));
        Isometry3::from_parts(Translation3::from(center), rot)
    }

    pub fn view(&self, elevation: f64) -> View {
        View {
            elevation,
            cam_from_world: self.world_from_camera(elevation).inverse(),
            lasers: self.lasers_at(elevation).into_iter().map(|(plane, _)| plane).collect(),
        }
    }

    /// La calibración exacta de este rig en las alturas dadas
    pub fn calibration(&self, elevations: &[f64]) -> Calibration {
        Calibration { camera: self.camera, plate: self.plate, views: elevations.iter().map(|&e| self.view(e)).collect() }
    }
}
