//! Escena lista para trazar: triángulos en mundo + materiales + texturas + BVH.

use crate::bvh::{Aabb, Bvh, Hit, Ray};
use crate::texture::Texture;
use converter_scene::{AlphaMode, Scene, TextureRef};
use glam::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RayAlpha {
    Opaque,
    /// Descarta por debajo del umbral
    Mask(f32),
    /// Transparencia estocástica: el rayo se detiene con probabilidad alfa
    Blend,
}

/// Material del motor (modelo metal/rugosidad de glTF).
/// Las texturas son índices a [`RayScene::textures`]; solo se usa el set UV 0.
#[derive(Debug, Clone, Copy)]
pub struct RayMaterial {
    pub base_color: Vec4,
    pub base_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    pub metal_rough_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub normal_scale: f32,
    pub emission: Vec3,
    pub emissive_texture: Option<usize>,
    pub alpha: RayAlpha,
    pub double_sided: bool,
    pub unlit: bool,
    /// Difuso ideal albedo/π (ignora metal, rugosidad y barniz): para pruebas
    pub lambert: bool,
    pub clearcoat: f32,
    pub clearcoat_roughness: f32,
    pub uv_scale: Vec2,
    /// Fracción de luz que atraviesa (vidrio, líquidos); la interfaz es lisa
    pub transmission: f32,
    pub ior: f32,
    /// > 0: volumen cerrado; la luz se refracta al entrar y salir y el color base la
    /// absorbe a lo largo de este recorrido (Beer–Lambert). 0: lámina delgada, sin
    /// refracción, que solo tiñe con el color base
    pub thickness: f32,
}

impl RayMaterial {
    /// Difuso puro con emisión opcional (mate, sin brillo)
    pub fn diffuse(albedo: Vec3, emission: Vec3) -> Self {
        RayMaterial {
            base_color: albedo.extend(1.0),
            metallic: 0.0,
            roughness: 1.0,
            lambert: true,
            emission,
            ..Default::default()
        }
    }

    pub fn has_alpha(&self) -> bool {
        self.alpha != RayAlpha::Opaque
    }
}

impl Default for RayMaterial {
    fn default() -> Self {
        RayMaterial {
            base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
            base_texture: None,
            metallic: 0.0,
            roughness: 1.0,
            metal_rough_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            emission: Vec3::ZERO,
            emissive_texture: None,
            alpha: RayAlpha::Opaque,
            double_sided: true,
            unlit: false,
            lambert: false,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            uv_scale: Vec2::ONE,
            transmission: 0.0,
            ior: 1.5,
            thickness: 0.0,
        }
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
    /// Color de vértice RGBA (blanco si la malla no trae)
    pub color: [Vec4; 3],
    pub material: u32,
}

impl Triangle {
    pub fn new(p: [Vec3; 3], n: Option<[Vec3; 3]>, uv: Option<[Vec2; 3]>, material: u32) -> Self {
        let e1 = p[1] - p[0];
        let e2 = p[2] - p[0];
        let g = e1.cross(e2).normalize_or_zero();
        Triangle {
            v0: p[0],
            e1,
            e2,
            n: n.unwrap_or([g; 3]),
            uv: uv.unwrap_or([Vec2::ZERO; 3]),
            color: [Vec4::ONE; 3],
            material,
        }
    }

    pub fn with_colors(mut self, color: [Vec4; 3]) -> Self {
        self.color = color;
        self
    }

    pub fn bounds(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        b.grow(self.v0);
        b.grow(self.v0 + self.e1);
        b.grow(self.v0 + self.e2);
        b
    }

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

    #[inline]
    pub fn interp_uv(&self, u: f32, v: f32) -> Vec2 {
        self.uv[0] * (1.0 - u - v) + self.uv[1] * u + self.uv[2] * v
    }

    #[inline]
    pub fn interp_color(&self, u: f32, v: f32) -> Vec4 {
        self.color[0] * (1.0 - u - v) + self.color[1] * u + self.color[2] * v
    }
}

/// Triángulos emisivos que se muestrean como luces de área (emisión sin textura)
#[derive(Debug, Default)]
pub struct AreaLights {
    /// Índices de triángulo
    pub tris: Vec<u32>,
    /// CDF por potencia (luminancia × área), normalizada a 1
    pub cdf: Vec<f32>,
    /// Posición en `tris` de cada triángulo de la escena (`u32::MAX` si no es luz)
    pub slot: Vec<u32>,
}

impl AreaLights {
    fn build(tris: &[Triangle], materials: &[RayMaterial]) -> AreaLights {
        let mut slot = vec![u32::MAX; tris.len()];
        let mut list = Vec::new();
        let mut weights = Vec::new();
        for (i, t) in tris.iter().enumerate() {
            let m = &materials[(t.material as usize).min(materials.len() - 1)];
            let area = 0.5 * t.e1.cross(t.e2).length();
            let power = m.emission.dot(Vec3::new(0.2126, 0.7152, 0.0722)) * area;
            if m.emissive_texture.is_none() && !m.unlit && power > 0.0 {
                slot[i] = list.len() as u32;
                list.push(i as u32);
                weights.push(power);
            }
        }
        let total: f32 = weights.iter().sum();
        let mut acc = 0.0;
        let cdf = weights
            .iter()
            .map(|w| {
                acc += w / total;
                acc
            })
            .collect();
        AreaLights { tris: list, cdf, slot }
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Elige una luz: (índice de triángulo, probabilidad de elegirla)
    pub fn pick(&self, u: f32) -> (u32, f32) {
        let k = self.cdf.partition_point(|&c| c <= u).min(self.tris.len() - 1);
        let prev = if k == 0 { 0.0 } else { self.cdf[k - 1] };
        (self.tris[k], self.cdf[k] - prev)
    }

    /// Probabilidad de elegir el triángulo (0 si no es luz)
    pub fn select_prob(&self, tri: u32) -> f32 {
        match self.slot[tri as usize] {
            u32::MAX => 0.0,
            k => self.cdf[k as usize] - if k == 0 { 0.0 } else { self.cdf[k as usize - 1] },
        }
    }
}

/// Rectángulo de `corner` con aristas `u` y `v`; la cara frontal mira hacia `u × v`.
/// Con un material emisivo es una luz de área.
pub fn quad(corner: Vec3, u: Vec3, v: Vec3, material: u32) -> [Triangle; 2] {
    let (a, b, c, d) = (corner, corner + u, corner + u + v, corner + v);
    [
        Triangle::new([a, b, c], None, Some([Vec2::ZERO, Vec2::X, Vec2::ONE]), material),
        Triangle::new([a, c, d], None, Some([Vec2::ZERO, Vec2::ONE, Vec2::Y]), material),
    ]
}

pub struct RayScene {
    pub triangles: Vec<Triangle>,
    pub area_lights: AreaLights,
    pub materials: Vec<RayMaterial>,
    /// Una por textura de la escena; `None` si no se pudo decodificar
    pub textures: Vec<Option<Texture>>,
    pub bvh: Bvh,
    /// Diagonal de la caja de la escena: escala para los márgenes numéricos
    pub extent: f32,
}

fn tex_index(r: &Option<TextureRef>) -> Option<usize> {
    r.as_ref().map(|t| t.texture_index)
}

impl RayScene {
    pub fn new(triangles: Vec<Triangle>, materials: Vec<RayMaterial>) -> Self {
        Self::with_textures(triangles, materials, Vec::new())
    }

    pub fn with_textures(
        triangles: Vec<Triangle>,
        mut materials: Vec<RayMaterial>,
        textures: Vec<Option<Texture>>,
    ) -> Self {
        if materials.is_empty() {
            materials.push(RayMaterial::default());
        }
        let bvh = Bvh::build(&triangles);
        let b = bvh.bounds();
        let extent = if triangles.is_empty() { 1.0 } else { (b.max - b.min).length().max(1e-6) };
        let area_lights = AreaLights::build(&triangles, &materials);
        RayScene { triangles, area_lights, materials, textures, bvh, extent }
    }

    /// Aplana un `Scene` de converter a triángulos en mundo.
    pub fn from_scene(scene: &Scene) -> Self {
        let textures: Vec<Option<Texture>> = scene.textures.iter().map(|t| Texture::decode(&t.data)).collect();
        let valid = |i: Option<usize>| i.filter(|&i| textures.get(i).is_some_and(|t| t.is_some()));

        let mut materials: Vec<RayMaterial> = scene
            .materials
            .iter()
            .map(|m| {
                let b = m.base_color_factor;
                RayMaterial {
                    base_color: Vec4::from(b),
                    base_texture: valid(tex_index(&m.base_color_texture)),
                    metallic: m.metallic_factor,
                    roughness: m.roughness_factor,
                    metal_rough_texture: valid(tex_index(&m.metallic_roughness_texture)),
                    normal_texture: valid(tex_index(&m.normal_texture)),
                    normal_scale: m.normal_scale,
                    emission: Vec3::from(m.emissive_factor),
                    emissive_texture: valid(tex_index(&m.emissive_texture)),
                    alpha: match m.alpha_mode {
                        AlphaMode::Opaque => RayAlpha::Opaque,
                        AlphaMode::Mask(c) => RayAlpha::Mask(c),
                        AlphaMode::Blend => RayAlpha::Blend,
                    },
                    double_sided: m.double_sided,
                    unlit: m.unlit,
                    lambert: false,
                    clearcoat: m.clearcoat_factor,
                    clearcoat_roughness: m.clearcoat_roughness_factor,
                    uv_scale: Vec2::from(m.uv_scale),
                    transmission: m.transmission_factor,
                    ior: m.ior,
                    thickness: m.thickness_factor,
                }
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
                let mut tri = Triangle::new(p, n, uv, mat);
                if let Some(cs) = &prim.colors {
                    tri = tri.with_colors(t.map(|i| Vec4::from(cs[i as usize])));
                }
                tris.push(tri);
            }
        }
        RayScene::with_textures(tris, materials, textures)
    }

    pub fn material(&self, tri: &Triangle) -> &RayMaterial {
        &self.materials[(tri.material as usize).min(self.materials.len() - 1)]
    }

    pub fn closest(&self, ray: &Ray, t_max: f32) -> Option<Hit> {
        self.bvh.closest(&self.triangles, ray, t_max)
    }

    pub fn occluded(&self, ray: &Ray, t_max: f32) -> bool {
        self.bvh.occluded(&self.triangles, ray, t_max)
    }

    /// Opacidad en un punto: factor × textura × color de vértice
    pub fn alpha_at(&self, tri: &Triangle, mat: &RayMaterial, u: f32, v: f32) -> f32 {
        let mut a = mat.base_color.w * tri.interp_color(u, v).w;
        if let Some(Some(tex)) = mat.base_texture.map(|i| &self.textures[i]) {
            a *= tex.sample(tri.interp_uv(u, v) * mat.uv_scale, true).w;
        }
        a
    }

    /// Cruce más cercano respetando máscara y mezcla
    pub fn closest_alpha(&self, ray: &Ray, t_max: f32, rng: &mut crate::rng::Rng) -> Option<Hit> {
        self.bvh.closest_with(&self.triangles, ray, t_max, &mut |ti, u, v| self.passes(ti, u, v, rng, false))
    }

    pub fn occluded_alpha(&self, ray: &Ray, t_max: f32, rng: &mut crate::rng::Rng) -> bool {
        self.bvh.occluded_with(&self.triangles, ray, t_max, &mut |ti, u, v| self.passes(ti, u, v, rng, true))
    }

    /// ¿Este cruce cuenta como golpe? En rayos de sombra el vidrio deja pasar la luz con
    /// probabilidad igual a lo que transmite (sombras falsas sin cáusticas: la luz no se
    /// desvía, pero tampoco queda un vidrio negro)
    fn passes(&self, ti: u32, u: f32, v: f32, rng: &mut crate::rng::Rng, shadow: bool) -> bool {
        let tri = &self.triangles[ti as usize];
        let mat = self.material(tri);
        let solid = match mat.alpha {
            RayAlpha::Opaque => true,
            RayAlpha::Mask(cut) => self.alpha_at(tri, mat, u, v) >= cut,
            RayAlpha::Blend => rng.next_f32() < self.alpha_at(tri, mat, u, v),
        };
        if solid && shadow && mat.transmission > 0.0 {
            let tint = mat.base_color.truncate();
            let pass = mat.transmission * (tint.x + tint.y + tint.z) / 3.0;
            return rng.next_f32() >= pass;
        }
        solid
    }
}
