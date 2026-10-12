//! BSDF de glTF metal/rugosidad: difuso Lambert + especular GGX (Smith correlacionado,
//! Schlick) + barniz GGX. Muestreo por mezcla de lóbulos; el especular usa VNDF.
//! Marco local: z = normal de sombreado.

use crate::rng::Rng;
use glam::Vec3;
use std::f32::consts::PI;

const MIN_ALPHA: f32 = 0.002;

#[derive(Debug, Clone, Copy)]
pub struct Bsdf {
    /// Color difuso ya multiplicado por (1 - metal)
    diffuse: Vec3,
    f0: Vec3,
    alpha: f32,
    coat: f32,
    coat_alpha: f32,
    p_diffuse: f32,
    p_spec: f32,
    p_coat: f32,
    /// Lambert puro (sin especular): para pruebas de conservación del integrador
    lambert: bool,
}

#[inline]
fn schlick(f0: Vec3, cos: f32) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - cos).clamp(0.0, 1.0).powi(5)
}

#[inline]
fn schlick_scalar(f0: f32, cos: f32) -> f32 {
    f0 + (1.0 - f0) * (1.0 - cos).clamp(0.0, 1.0).powi(5)
}

#[inline]
fn d_ggx(n_h: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = n_h * n_h * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d)
}

/// Visibilidad de Smith correlacionada (incluye 1 / (4 NoL NoV))
#[inline]
fn vis_smith(n_v: f32, n_l: f32, a: f32) -> f32 {
    let a2 = a * a;
    let gv = n_l * (n_v * n_v * (1.0 - a2) + a2).sqrt();
    let gl = n_v * (n_l * n_l * (1.0 - a2) + a2).sqrt();
    0.5 / (gv + gl).max(1e-9)
}

#[inline]
fn g1(n_v: f32, a: f32) -> f32 {
    let a2 = a * a;
    2.0 * n_v / (n_v + (a2 + (1.0 - a2) * n_v * n_v).sqrt())
}

/// pdf (respecto a wi) de muestrear wi con VNDF
#[inline]
fn pdf_vndf(n_v: f32, n_h: f32, a: f32) -> f32 {
    g1(n_v, a) * d_ggx(n_h, a) / (4.0 * n_v)
}

fn sample_vndf(wo: Vec3, a: f32, rng: &mut Rng) -> Vec3 {
    let vh = Vec3::new(a * wo.x, a * wo.y, wo.z).normalize();
    let t1 = if vh.z < 0.9999 { Vec3::Z.cross(vh).normalize() } else { Vec3::X };
    let t2 = vh.cross(t1);
    let (u1, u2) = (rng.next_f32(), rng.next_f32());
    let r = u1.sqrt();
    let phi = 2.0 * PI * u2;
    let (p1, mut p2) = (r * phi.cos(), r * phi.sin());
    let s = 0.5 * (1.0 + vh.z);
    p2 = (1.0 - s) * (1.0 - p1 * p1).max(0.0).sqrt() + s * p2;
    let nh = t1 * p1 + t2 * p2 + vh * (1.0 - p1 * p1 - p2 * p2).max(0.0).sqrt();
    Vec3::new(a * nh.x, a * nh.y, nh.z.max(0.0)).normalize()
}

fn lum(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

impl Bsdf {
    pub fn new(base: Vec3, metallic: f32, roughness: f32, coat: f32, coat_roughness: f32) -> Self {
        Self::with_ior(base, metallic, roughness, coat, coat_roughness, 1.5)
    }

    /// Como [`Bsdf::new`], con el IOR del dieléctrico (fija su reflectancia frontal F0)
    pub fn with_ior(base: Vec3, metallic: f32, roughness: f32, coat: f32, coat_roughness: f32, ior: f32) -> Self {
        let f0_diel = ((ior - 1.0) / (ior + 1.0)).powi(2);
        let metallic = metallic.clamp(0.0, 1.0);
        let roughness = roughness.clamp(0.0, 1.0);
        let diffuse = base * (1.0 - metallic);
        let f0 = Vec3::splat(f0_diel).lerp(base, metallic);
        let coat = coat.clamp(0.0, 1.0);
        let wd = lum(diffuse);
        let ws = lum(f0).max(0.06);
        let wc = if coat > 0.0 { 0.25 * coat + 0.02 } else { 0.0 };
        let sum = wd + ws + wc;
        Bsdf {
            diffuse,
            f0,
            alpha: (roughness * roughness).max(MIN_ALPHA),
            coat,
            coat_alpha: (coat_roughness.clamp(0.0, 1.0).powi(2)).max(MIN_ALPHA),
            p_diffuse: wd / sum,
            p_spec: ws / sum,
            p_coat: wc / sum,
            lambert: false,
        }
    }

    /// Difuso ideal albedo/π, sin especular ni barniz
    pub fn lambert(albedo: Vec3) -> Self {
        Bsdf { diffuse: albedo, f0: Vec3::ZERO, alpha: 1.0, coat: 0.0, coat_alpha: 1.0, p_diffuse: 1.0, p_spec: 0.0, p_coat: 0.0, lambert: true }
    }

    /// (f, pdf) para el par de direcciones locales (hacia fuera de la superficie).
    /// `f` no incluye el coseno.
    pub fn eval(&self, wo: Vec3, wi: Vec3) -> (Vec3, f32) {
        let n_v = wo.z;
        let n_l = wi.z;
        if n_v <= 0.0 || n_l <= 0.0 {
            return (Vec3::ZERO, 0.0);
        }
        if self.lambert {
            return (self.diffuse / PI, n_l / PI);
        }
        let h = (wo + wi).normalize_or_zero();
        let (n_h, v_h) = (h.z, wo.dot(h).max(0.0));

        // El barniz atenúa lo de debajo; usa el ángulo más rasante para ser simétrico
        let coat_fresnel = schlick_scalar(0.04, n_v.min(n_l));
        let under = 1.0 - self.coat * coat_fresnel;

        // Difuso de Ashikhmin–Shirley: simétrico y, junto al especular, no crea energía
        let fall = |c: f32| 1.0 - (1.0 - 0.5 * c).powi(5);
        let diffuse = self.diffuse * (28.0 / (23.0 * PI) * 0.96 * fall(n_v) * fall(n_l));
        let spec = schlick(self.f0, v_h) * (d_ggx(n_h, self.alpha) * vis_smith(n_v, n_l, self.alpha));
        let mut f = (diffuse + spec) * under;
        let mut pdf = self.p_diffuse * n_l / PI + self.p_spec * pdf_vndf(n_v, n_h, self.alpha);

        if self.coat > 0.0 {
            let fc = schlick_scalar(0.04, v_h);
            f += Vec3::splat(self.coat * fc * d_ggx(n_h, self.coat_alpha) * vis_smith(n_v, n_l, self.coat_alpha));
            pdf += self.p_coat * pdf_vndf(n_v, n_h, self.coat_alpha);
        }
        (f, pdf)
    }

    /// Elige un lóbulo y una dirección. Devuelve (wi, f, pdf) con la pdf de la mezcla.
    pub fn sample(&self, wo: Vec3, rng: &mut Rng) -> Option<(Vec3, Vec3, f32)> {
        if wo.z <= 0.0 {
            return None;
        }
        let pick = rng.next_f32();
        let wi = if pick < self.p_diffuse {
            let (u1, u2) = (rng.next_f32(), rng.next_f32());
            let r = u1.sqrt();
            let phi = 2.0 * PI * u2;
            Vec3::new(r * phi.cos(), r * phi.sin(), (1.0 - u1).max(0.0).sqrt())
        } else {
            let a = if pick < self.p_diffuse + self.p_spec { self.alpha } else { self.coat_alpha };
            let h = sample_vndf(wo, a, rng);
            2.0 * wo.dot(h) * h - wo
        };
        if wi.z <= 0.0 {
            return None;
        }
        let (f, pdf) = self.eval(wo, wi);
        (pdf > 1e-9 && f != Vec3::ZERO).then_some((wi, f, pdf))
    }
}
