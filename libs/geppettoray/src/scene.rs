//! Escena lista para trazar: triángulos en mundo + materiales + BVH.

use crate::bvh::{Aabb, Bvh, Hit, Ray};
use converter_scene::Scene;
use glam::{Vec2, Vec3};

/// Material del motor. En 3a solo difuso + emisión; el resto del glTF llega en 3b.
#[derive(Debug, Clone, Copy)]
pub struct RayMaterial {
    pub albedo: Vec3,
    pub emission: Vec3,
    pub double_sided: bool,
}

impl Default for RayMaterial {
    fn default() -> Self {
        RayMaterial { albedo: Vec3::splat(0.8), emission: Vec3::ZERO, double_sided: true }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub v0: Vec3,
    pub e1: Vec3,
    pub e2: Vec3,
    /// Normales por vértice (si la malla no trae, la geométrica repetida)
    pub n: [Vec3; 3],
    pub uv: [Vec2; 3],
    pub material: u32,
}

impl Triangle {
    pub fn new(p: [Vec3; 3], n: Option<[Vec3; 3]>, uv: Option<[Vec2; 3]>, material: u32) -> Self {
        let e1 = p[1] - p[0];
        let e2 = p[2] - p[0];
        let g = e1.cross(e2).normalize_or_zero();
        Triangle { v0: p[0], e1, e2, n: n.unwrap_or([g; 3]), uv: uv.unwrap_or([Vec2::ZERO; 3]), material }
    }

    pub fn bounds(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        b.grow(self.v0);
        b.grow(self.v0 + self.e1);
        b.grow(self.v0 + self.e2);
        b
    }

    /// Normal geométrica (sin normalizar su signo respecto a las normales de vértice)
    pub fn geometric_normal(&self) -> Vec3 {
        self.e1.cross(self.e2).normalize_or_zero()
    }

    /// Möller–Trumbore. Devuelve (t, u, v) con t en (0, t_max).
    #[inline]
    pub fn intersect(&self, ray: &Ray, t_max: f32) -> Option<(f32, f32, f32)> {
        let p = ray.dir.cross(self.e2);
        let det = self.e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = ray.origin - self.v0;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(self.e1);
        let v = ray.dir.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = self.e2.dot(q) * inv;
        (t > 0.0 && t < t_max).then_some((t, u, v))
    }
}

pub struct RayScene {
    pub triangles: Vec<Triangle>,
    pub materials: Vec<RayMaterial>,
    pub bvh: Bvh,
    /// Diagonal de la caja de la escena: escala para los márgenes numéricos
    pub extent: f32,
}

impl RayScene {
    pub fn new(triangles: Vec<Triangle>, mut materials: Vec<RayMaterial>) -> Self {
        if materials.is_empty() {
            materials.push(RayMaterial::default());
        }
        let bvh = Bvh::build(&triangles);
        let b = bvh.bounds();
        let extent = if triangles.is_empty() { 1.0 } else { (b.max - b.min).length().max(1e-6) };
        RayScene { triangles, materials, bvh, extent }
    }

    /// Aplana un `Scene` de converter a triángulos en mundo.
    pub fn from_scene(scene: &Scene) -> Self {
        let mut materials: Vec<RayMaterial> = scene
            .materials
            .iter()
            .map(|m| RayMaterial {
                albedo: Vec3::new(m.base_color_factor[0], m.base_color_factor[1], m.base_color_factor[2]),
                emission: Vec3::from(m.emissive_factor),
                double_sided: m.double_sided,
            })
            .collect();
        let default_idx = materials.len() as u32;
        materials.push(RayMaterial::default());

        let mut tris = Vec::new();
        for prim in scene.world_primitives() {
            let mat = prim.material.map_or(default_idx, |m| m as u32);
            let pos = |i: u32| Vec3::from(prim.positions[i as usize]);
            for t in &prim.triangles {
                let p = [pos(t[0]), pos(t[1]), pos(t[2])];
                let n = prim.normals.as_ref().map(|ns| t.map(|i| Vec3::from(ns[i as usize])));
                let uv = prim.uvs.as_ref().map(|us| t.map(|i| Vec2::from(us[i as usize])));
                tris.push(Triangle::new(p, n, uv, mat));
            }
        }
        RayScene::new(tris, materials)
    }

    pub fn closest(&self, ray: &Ray, t_max: f32) -> Option<Hit> {
        self.bvh.closest(&self.triangles, ray, t_max)
    }

    pub fn occluded(&self, ray: &Ray, t_max: f32) -> bool {
        self.bvh.occluded(&self.triangles, ray, t_max)
    }
}
