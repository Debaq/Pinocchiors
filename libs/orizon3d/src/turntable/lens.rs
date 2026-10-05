//! Modelo de cámara estenopeica con distorsión de lente (Brown-Conrady, el
//! mismo que usa OpenCV: radial k1, k2, k3 y tangencial p1, p2).

use super::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraModel {
    pub width: u32,
    pub height: u32,
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub k1: f64,
    pub k2: f64,
    pub k3: f64,
    pub p1: f64,
    pub p2: f64,
}

impl CameraModel {
    /// Cámara ideal centrada con el campo de visión horizontal dado (grados)
    pub fn from_hfov(width: u32, height: u32, hfov_deg: f64) -> Self {
        let f = width as f64 / 2.0 / (hfov_deg.to_radians() / 2.0).tan();
        CameraModel {
            width,
            height,
            fx: f,
            fy: f,
            cx: (width as f64 - 1.0) / 2.0,
            cy: (height as f64 - 1.0) / 2.0,
            k1: 0.0,
            k2: 0.0,
            k3: 0.0,
            p1: 0.0,
            p2: 0.0,
        }
    }

    /// La misma cámara a otra resolución (mismo campo de visión y distorsión)
    pub fn scaled(&self, width: u32, height: u32) -> Self {
        let (sx, sy) = (width as f64 / self.width as f64, height as f64 / self.height as f64);
        CameraModel {
            width,
            height,
            fx: self.fx * sx,
            fy: self.fy * sy,
            cx: (self.cx + 0.5) * sx - 0.5,
            cy: (self.cy + 0.5) * sy - 0.5,
            ..*self
        }
    }

    /// Coordenadas normalizadas ideales → distorsionadas
    pub fn distort(&self, x: f64, y: f64) -> (f64, f64) {
        let r2 = x * x + y * y;
        let radial = 1.0 + r2 * (self.k1 + r2 * (self.k2 + r2 * self.k3));
        let dx = 2.0 * self.p1 * x * y + self.p2 * (r2 + 2.0 * x * x);
        let dy = self.p1 * (r2 + 2.0 * y * y) + 2.0 * self.p2 * x * y;
        (x * radial + dx, y * radial + dy)
    }

    /// Inversa de [`distort`](Self::distort) por punto fijo (converge para las
    /// distorsiones moderadas de una webcam)
    pub fn undistort(&self, xd: f64, yd: f64) -> (f64, f64) {
        let (mut x, mut y) = (xd, yd);
        for _ in 0..30 {
            let r2 = x * x + y * y;
            let radial = 1.0 + r2 * (self.k1 + r2 * (self.k2 + r2 * self.k3));
            let dx = 2.0 * self.p1 * x * y + self.p2 * (r2 + 2.0 * x * x);
            let dy = self.p1 * (r2 + 2.0 * y * y) + 2.0 * self.p2 * x * y;
            let (nx, ny) = ((xd - dx) / radial, (yd - dy) / radial);
            let done = (nx - x).abs() < 1e-12 && (ny - y).abs() < 1e-12;
            (x, y) = (nx, ny);
            if done {
                break;
            }
        }
        (x, y)
    }

    /// Punto en el marco de la cámara → píxel; `None` si está detrás
    pub fn project(&self, p: &Vec3) -> Option<(f64, f64)> {
        if p.z <= 1e-9 {
            return None;
        }
        let (x, y) = self.distort(p.x / p.z, p.y / p.z);
        Some((self.fx * x + self.cx, self.fy * y + self.cy))
    }

    /// Dirección unitaria (marco de la cámara) del rayo que llega al píxel
    pub fn ray(&self, u: f64, v: f64) -> Vec3 {
        let (x, y) = self.undistort((u - self.cx) / self.fx, (v - self.cy) / self.fy);
        Vec3::new(x, y, 1.0).normalize()
    }
}
