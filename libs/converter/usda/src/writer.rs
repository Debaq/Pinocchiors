use std::collections::HashMap;

use converter_scene::{Scene, Transform, VertexAttribute};
use glam::{Quat, Vec3};
use std::fmt::Write;
use thiserror::Error;

use crate::materials;
use crate::skeleton::{
    self, NodeAnimSamples, SkelContext,
};

#[derive(Debug, Error)]
pub enum UsdaWriteError {
    #[error("error de formato: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("error escribiendo archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("error decodificando textura: {0}")]
    TextureDecode(String),
    #[error("error codificando textura: {0}")]
    TextureEncode(String),
}

// ---------------------------------------------------------------------------
// Opciones de exportación y resultado
// ---------------------------------------------------------------------------

/// Opciones configurables para la exportación USDA/USDZ.
pub struct UsdaExportOptions {
    /// Factor de escala para el nodo raíz. `None` = sin escala.
    /// Ejemplo: `Some(100.0)` convierte metros → centímetros.
    pub scale_factor: Option<f64>,
    /// Tamaño máximo de textura (ancho o alto). `None` = sin límite.
    pub max_texture_size: Option<u32>,
    /// Si `true`, separa la textura ORM en canales individuales
    /// (metallic, roughness, occlusion).
    pub split_orm_channels: bool,
    /// Si `true`, aplica restricciones de compatibilidad con AR Quick Look (Apple):
    /// - Limita texturas a 2048×2048
    /// - Solo escribe un set de UV (`primvars:st`), ignorando sets adicionales
    pub arkit_compatible: bool,
    /// Frames por segundo para la conversión de timeSamples. Default: 24.0.
    pub fps: f64,
    /// Si `true`, exporta animaciones (esqueleto y rígidas). Default: true.
    pub export_animations: bool,
    /// Tolerancia para eliminar keyframes redundantes. Default: 1e-4.
    pub keyframe_tolerance: f32,
}

impl Default for UsdaExportOptions {
    fn default() -> Self {
        Self {
            scale_factor: None,
            max_texture_size: None,
            split_orm_channels: false,
            arkit_compatible: false,
            fps: 24.0,
            export_animations: true,
            keyframe_tolerance: 1e-4,
        }
    }
}

/// Textura procesada lista para empaquetar.
pub struct ProcessedTexture {
    /// Ruta relativa del asset (e.g. `textures/baseColor.png`).
    pub asset_path: String,
    /// Bytes de la textura (PNG o JPEG).
    pub data: Vec<u8>,
}

/// Resultado de la exportación USDA.
pub struct UsdaOutput {
    /// Contenido USDA como texto.
    pub usda: String,
    /// Texturas procesadas para empaquetar junto al USDA.
    pub textures: Vec<ProcessedTexture>,
}

// ---------------------------------------------------------------------------
// UsdWriter — helper con indentación para generar USDA limpio
// ---------------------------------------------------------------------------

pub(crate) struct UsdWriter {
    pub buf: String,
    pub(crate) depth: usize,
}

impl UsdWriter {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: String::with_capacity(capacity),
            depth: 0,
        }
    }

    pub fn line(&mut self, text: &str) {
        self.write_indent();
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    pub fn open_block(&mut self, header: &str) {
        self.line(header);
        self.line("{");
        self.depth += 1;
    }

    pub fn close_block(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.line("}");
    }

    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Escribe directamente al buffer con indentación + write! format.
    pub fn write_fmt_line(&mut self, args: std::fmt::Arguments<'_>) {
        self.write_indent();
        self.buf.write_fmt(args).unwrap();
        self.buf.push('\n');
    }

    pub(crate) fn write_indent(&mut self) {
        for _ in 0..self.depth {
            self.buf.push_str("    ");
        }
    }

    pub fn finish(self) -> String {
        self.buf
    }
}

// ---------------------------------------------------------------------------
// API pública
// ---------------------------------------------------------------------------

/// Escribe una `Scene` como string USDA (texto USD).
pub fn write_usda(
    scene: &Scene,
    options: &UsdaExportOptions,
) -> Result<UsdaOutput, UsdaWriteError> {
    let mut w = UsdWriter::new(64 * 1024);

    // Preparar contexto de skeletons
    let skel_ctx = skeleton::prepare_skel_context(scene);
    let node_anim_ctx = skeleton::build_node_anim_context(scene, &skel_ctx, options);

    write_header(&mut w, scene, options);

    w.open_block("def Xform \"Root\" (\n    kind = \"component\"\n)");

    // Escala raíz si se configuró
    if let Some(factor) = options.scale_factor {
        w.write_fmt_line(format_args!(
            "float3 xformOp:scale = ({}, {}, {})",
            factor, factor, factor
        ));
        w.line("token[] xformOpOrder = [\"xformOp:scale\"]");
    }

    // Nodos del grafo de escena
    for (i, &root_idx) in scene.root_nodes.iter().enumerate() {
        if i > 0 {
            w.blank();
        }
        write_node_pub(&mut w, scene, root_idx, false, options, &skel_ctx, &node_anim_ctx);
    }

    // Procesar texturas
    let tex_result = crate::textures::process_textures(scene, options)?;

    // Materiales
    if !scene.materials.is_empty() {
        w.blank();
        materials::write_materials_scope(&mut w, scene, &tex_result);
    }

    w.close_block(); // Root

    Ok(UsdaOutput {
        usda: w.finish(),
        textures: tex_result.outputs,
    })
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn write_header(w: &mut UsdWriter, scene: &Scene, options: &UsdaExportOptions) {
    let meters_per_unit = if let Some(factor) = options.scale_factor {
        scene.meters_per_unit / factor
    } else {
        scene.meters_per_unit
    };

    w.line("#usda 1.0");
    w.line("(");
    w.line("    defaultPrim = \"Root\"");
    w.write_fmt_line(format_args!("    metersPerUnit = {}", meters_per_unit));
    let up = if scene.y_up { "Y" } else { "Z" };
    w.write_fmt_line(format_args!("    upAxis = \"{}\"", up));

    // TimeCodes si hay animaciones
    if options.export_animations
        && let Some((start, end)) = skeleton::compute_time_range(scene, options.fps) {
            w.write_fmt_line(format_args!("    startTimeCode = {}", start));
            w.write_fmt_line(format_args!("    endTimeCode = {}", end));
        }

    w.line(")");
    w.blank();
}

// ---------------------------------------------------------------------------
// Nodos (Xform)
// ---------------------------------------------------------------------------

/// Escribe un nodo con soporte completo: SkelRoot si tiene skin, animación rígida, o Xform normal.
pub(crate) fn write_node_pub(
    w: &mut UsdWriter,
    scene: &Scene,
    node_idx: usize,
    parent_flipped: bool,
    options: &UsdaExportOptions,
    skel_ctx: &SkelContext,
    node_anim_ctx: &HashMap<usize, NodeAnimSamples>,
) {
    let node = &scene.nodes[node_idx];

    // Si el nodo es un joint de skeleton, no lo escribimos como Xform
    // (se escribe dentro del SkelRoot/Skeleton)
    if skel_ctx.node_to_joint.contains_key(&node_idx) {
        return;
    }

    // Si tiene skin, delegar a write_skel_root
    if let Some(skin_idx) = node.skin {
        skeleton::write_skel_root(w, scene, skin_idx, skel_ctx, node_idx, options, node_anim_ctx);
        return;
    }

    let name = sanitize_name(&node.name, "node", node_idx);

    w.open_block(&format!("def Xform \"{}\"", name));

    // Animación rígida o transform estático
    if let Some(anim_samples) = node_anim_ctx.get(&node_idx) {
        skeleton::write_animated_transform(w, anim_samples);
    } else {
        write_transform(w, &node.transform);
    }

    let node_flipped = parent_flipped ^ has_negative_scale(&node.transform);

    if let Some(mesh_idx) = node.mesh {
        write_mesh(w, scene, mesh_idx, node_flipped, options);
    }

    for (i, &child_idx) in node.children.iter().enumerate() {
        if i > 0 || node.mesh.is_some() {
            w.blank();
        }
        write_node_pub(w, scene, child_idx, node_flipped, options, skel_ctx, node_anim_ctx);
    }

    w.close_block();
}

/// Detecta si un transform tiene escala negativa (determinante < 0).
fn has_negative_scale(transform: &Transform) -> bool {
    match transform {
        Transform::Trs { scale, .. } => scale.x * scale.y * scale.z < 0.0,
        Transform::Matrix(m) => m.determinant() < 0.0,
    }
}

/// Wrapper pub(crate) para que skeleton.rs pueda escribir transforms.
pub(crate) fn write_transform_pub(w: &mut UsdWriter, transform: &Transform) {
    write_transform(w, transform);
}

fn write_transform(w: &mut UsdWriter, transform: &Transform) {
    match transform {
        Transform::Trs {
            translation,
            rotation,
            scale,
        } => {
            if is_identity_trs(translation, rotation, scale) {
                return;
            }
            w.write_fmt_line(format_args!(
                "double3 xformOp:translate = ({}, {}, {})",
                translation.x, translation.y, translation.z
            ));
            // USD quaternion: (w, x, y, z)
            w.write_fmt_line(format_args!(
                "quatf xformOp:orient = ({}, {}, {}, {})",
                rotation.w, rotation.x, rotation.y, rotation.z
            ));
            w.write_fmt_line(format_args!(
                "float3 xformOp:scale = ({}, {}, {})",
                scale.x, scale.y, scale.z
            ));
            w.line(
                "token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:orient\", \"xformOp:scale\"]",
            );
        }
        Transform::Matrix(m) => {
            let cols = m.to_cols_array();
            // Verificar si es identidad
            if (*m - glam::Mat4::IDENTITY)
                .to_cols_array()
                .iter()
                .all(|v| v.abs() < 1e-7)
            {
                return;
            }
            w.write_fmt_line(format_args!(
                "matrix4d xformOp:transform = (({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}))",
                cols[0], cols[1], cols[2], cols[3],
                cols[4], cols[5], cols[6], cols[7],
                cols[8], cols[9], cols[10], cols[11],
                cols[12], cols[13], cols[14], cols[15]
            ));
            w.line("token[] xformOpOrder = [\"xformOp:transform\"]");
        }
    }
}

fn is_identity_trs(t: &Vec3, r: &Quat, s: &Vec3) -> bool {
    t.length_squared() < 1e-14
        && (r.x * r.x + r.y * r.y + r.z * r.z) < 1e-14
        && (*s - Vec3::ONE).length_squared() < 1e-14
}

// ---------------------------------------------------------------------------
// Meshes (UsdGeomMesh)
// ---------------------------------------------------------------------------

fn write_mesh(w: &mut UsdWriter, scene: &Scene, mesh_idx: usize, flipped: bool, options: &UsdaExportOptions) {
    let mesh = &scene.meshes[mesh_idx];

    for (pi, prim) in mesh.primitives.iter().enumerate() {
        let name = if mesh.primitives.len() == 1 {
            sanitize_name(&mesh.name, "mesh", mesh_idx)
        } else {
            format!("{}_{}", sanitize_name(&mesh.name, "mesh", mesh_idx), pi)
        };
        write_primitive(w, &name, prim, scene, mesh_idx, pi, flipped, options);
    }
}

#[allow(clippy::too_many_arguments)]
fn write_primitive(
    w: &mut UsdWriter,
    name: &str,
    prim: &converter_scene::Primitive,
    scene: &Scene,
    mesh_idx: usize,
    prim_idx: usize,
    flipped: bool,
    options: &UsdaExportOptions,
) {
    // Extraer atributos
    let positions = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Positions(p) = a {
            Some(p)
        } else {
            None
        }
    });
    let positions = match positions {
        Some(p) => p,
        None => return,
    };

    w.open_block(&format!("def Mesh \"{}\"", name));

    // faceVertexCounts y faceVertexIndices (triángulos y quads); al revés si
    // la escala acumulada es negativa
    let (face_counts, face_indices) = face_arrays(scene, mesh_idx, prim_idx, flipped);

    write_int_array(w, "int[] faceVertexCounts", &face_counts);
    write_int_array(w, "int[] faceVertexIndices", &face_indices);

    // points
    write_point3f_array(w, "point3f[] points", positions);

    // normals
    if let Some(normals) = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Normals(n) = a {
            Some(n)
        } else {
            None
        }
    }) {
        write_normal3f_array(w, normals);
    }

    // texcoords (primvars:st, primvars:st1, ...)
    // ARKit solo soporta un set de UV
    for attr in &prim.attributes {
        if let VertexAttribute::TexCoords(set, uvs) = attr {
            if options.arkit_compatible && *set > 0 {
                continue;
            }
            let varname = if *set == 0 {
                "primvars:st".to_string()
            } else {
                format!("primvars:st{}", set)
            };
            write_texcoord2f_array(w, &varname, uvs);
        }
    }

    // vertex colors
    if let Some(colors) = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Colors(c) = a {
            Some(c)
        } else {
            None
        }
    }) {
        write_vertex_colors(w, colors);
    }

    // extent (bounding box)
    let (min, max) = compute_extent(positions);
    w.write_fmt_line(format_args!(
        "float3[] extent = [({}, {}, {}), ({}, {}, {})]",
        min[0], min[1], min[2], max[0], max[1], max[2]
    ));

    w.line("uniform token subdivisionScheme = \"none\"");

    // doubleSided
    let double_sided = prim
        .material
        .and_then(|idx| scene.materials.get(idx))
        .is_some_and(|mat| mat.double_sided);
    if double_sided {
        w.line("uniform bool doubleSided = 1");
    }

    // material binding
    if let Some(mat_idx) = prim.material
        && let Some(mat) = scene.materials.get(mat_idx) {
            let mat_name = materials::material_prim_name(mat, mat_idx);
            w.write_fmt_line(format_args!(
                "rel material:binding = </Root/Materials/{}>",
                mat_name
            ));
        }

    w.close_block();
}

// ---------------------------------------------------------------------------
// Helpers de escritura de arrays USD
// ---------------------------------------------------------------------------

fn write_int_array(w: &mut UsdWriter, prefix: &str, values: &[u32]) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "{}", v).unwrap();
    }
    w.buf.push_str("]\n");
}

fn write_point3f_array(w: &mut UsdWriter, prefix: &str, values: &[[f32; 3]]) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "({}, {}, {})", v[0], v[1], v[2]).unwrap();
    }
    w.buf.push_str("]\n");
}

fn write_normal3f_array(w: &mut UsdWriter, values: &[[f32; 3]]) {
    w.write_indent();
    w.buf.push_str("normal3f[] normals = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "({}, {}, {})", v[0], v[1], v[2]).unwrap();
    }
    w.buf.push_str("] (\n");
    w.write_indent();
    w.buf.push_str("    interpolation = \"vertex\"\n");
    w.write_indent();
    w.buf.push_str(")\n");
}

fn write_texcoord2f_array(w: &mut UsdWriter, varname: &str, values: &[[f32; 2]]) {
    w.write_indent();
    write!(w.buf, "texCoord2f[] {} = [", varname).unwrap();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "({}, {})", v[0], v[1]).unwrap();
    }
    w.buf.push_str("] (\n");
    w.write_indent();
    w.buf.push_str("    interpolation = \"vertex\"\n");
    w.write_indent();
    w.buf.push_str(")\n");
}

fn write_vertex_colors(w: &mut UsdWriter, colors: &[[f32; 4]]) {
    // displayColor (RGB)
    w.write_indent();
    w.buf.push_str("color3f[] primvars:displayColor = [");
    for (i, c) in colors.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "({}, {}, {})", c[0], c[1], c[2]).unwrap();
    }
    w.buf.push_str("] (\n");
    w.write_indent();
    w.buf.push_str("    interpolation = \"vertex\"\n");
    w.write_indent();
    w.buf.push_str(")\n");

    // displayOpacity (A) — solo si hay valores < 1.0
    if colors.iter().any(|c| c[3] < 1.0 - 1e-6) {
        w.write_indent();
        w.buf.push_str("float[] primvars:displayOpacity = [");
        for (i, c) in colors.iter().enumerate() {
            if i > 0 {
                w.buf.push_str(", ");
            }
            write!(w.buf, "{}", c[3]).unwrap();
        }
        w.buf.push_str("] (\n");
        w.write_indent();
        w.buf.push_str("    interpolation = \"vertex\"\n");
        w.write_indent();
        w.buf.push_str(")\n");
    }
}

fn compute_extent(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            if p[i] < min[i] {
                min[i] = p[i];
            }
            if p[i] > max[i] {
                max[i] = p[i];
            }
        }
    }
    (min, max)
}

// ---------------------------------------------------------------------------
// pub(crate) wrappers para skeleton.rs
// ---------------------------------------------------------------------------

/// `faceVertexCounts` y `faceVertexIndices` de una primitiva: triángulos y
/// quads (los pares que la escena marca), recorridos al revés si `flipped`
pub(crate) fn face_arrays(scene: &Scene, mesh_idx: usize, prim_idx: usize, flipped: bool) -> (Vec<u32>, Vec<u32>) {
    let mut counts = Vec::new();
    let mut indices = Vec::new();
    for face in scene.polygons(mesh_idx, prim_idx) {
        let face = if flipped { face.reversed() } else { face };
        counts.push(face.vertices().len() as u32);
        indices.extend_from_slice(face.vertices());
    }
    (counts, indices)
}

pub(crate) fn write_int_array_pub(w: &mut UsdWriter, prefix: &str, values: &[u32]) {
    write_int_array(w, prefix, values);
}

pub(crate) fn write_point3f_array_pub(w: &mut UsdWriter, prefix: &str, values: &[[f32; 3]]) {
    write_point3f_array(w, prefix, values);
}

pub(crate) fn write_normal3f_array_pub(w: &mut UsdWriter, values: &[[f32; 3]]) {
    write_normal3f_array(w, values);
}

pub(crate) fn write_texcoord2f_array_pub(w: &mut UsdWriter, varname: &str, values: &[[f32; 2]]) {
    write_texcoord2f_array(w, varname, values);
}

pub(crate) fn write_vertex_colors_pub(w: &mut UsdWriter, colors: &[[f32; 4]]) {
    write_vertex_colors(w, colors);
}

pub(crate) fn compute_extent_pub(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    compute_extent(positions)
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

/// Genera un nombre USD válido (alfanumérico + underscore, no empieza con dígito).
#[allow(clippy::manual_pattern_char_comparison)]
pub(crate) fn sanitize_name(name: &str, prefix: &str, fallback_idx: usize) -> String {
    if name.is_empty() {
        return format!("{}_{}", prefix, fallback_idx);
    }
    let mut result = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            if i == 0 && c.is_ascii_digit() {
                result.push('_');
            }
            result.push(c);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        format!("{}_{}", prefix, fallback_idx)
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{
        IndexData, Material, Mesh, Node, Primitive, Scene, Texture, TextureFormat, TextureRef,
        VertexAttribute,
    };

    fn triangle_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "Triangle".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![
                        [0.0, 0.0, 0.0],
                        [1.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0],
                    ]),
                    VertexAttribute::Normals(vec![
                        [0.0, 0.0, 1.0],
                        [0.0, 0.0, 1.0],
                        [0.0, 0.0, 1.0],
                    ]),
                ],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "Root_Node".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    fn default_opts() -> UsdaExportOptions {
        UsdaExportOptions::default()
    }

    #[test]
    fn usda_header() {
        let scene = triangle_scene();
        let output = write_usda(&scene, &default_opts()).unwrap();

        assert!(output.usda.starts_with("#usda 1.0\n"));
        assert!(output.usda.contains("defaultPrim = \"Root\""));
        assert!(output.usda.contains("metersPerUnit = 1"));
        assert!(output.usda.contains("upAxis = \"Y\""));
    }

    #[test]
    fn usda_writes_quads_as_four_sided_faces() {
        let mut scene = triangle_scene();
        // Un cuadrado (0,1,2,3) como par de triángulos más un triángulo suelto
        let prim = &mut scene.meshes[0].primitives[0];
        prim.attributes = vec![VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0]])];
        prim.indices = Some(IndexData::U32(vec![0, 1, 2, 0, 2, 3, 1, 4, 2]));
        scene.set_quad_pairs(0, 0, 1);
        let usda = write_usda(&scene, &default_opts()).unwrap().usda;
        assert!(usda.contains("int[] faceVertexCounts = [4, 3]"), "{usda}");
        assert!(usda.contains("int[] faceVertexIndices = [0, 1, 2, 3, 1, 4, 2]"), "{usda}");
    }

    #[test]
    fn usda_mesh_geometry() {
        let scene = triangle_scene();
        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("def Mesh \"Triangle\""));
        assert!(usda.contains("int[] faceVertexCounts = [3]"));
        assert!(usda.contains("int[] faceVertexIndices = [0, 1, 2]"));
        assert!(usda.contains("point3f[] points = ["));
        assert!(usda.contains("normal3f[] normals = ["));
        assert!(usda.contains("interpolation = \"vertex\""));
        assert!(usda.contains("subdivisionScheme = \"none\""));
        assert!(usda.contains("float3[] extent = ["));
    }

    #[test]
    fn usda_xform_identity_omitted() {
        let scene = triangle_scene();
        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        // Identity transform no debería generar xformOps en el nodo
        assert!(usda.contains("def Xform \"Root_Node\""));
    }

    #[test]
    fn usda_xform_with_transform() {
        let mut scene = triangle_scene();
        scene.nodes[0].transform = Transform::Trs {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("xformOp:translate = (1, 2, 3)"));
        assert!(usda.contains("xformOpOrder"));
    }

    #[test]
    fn usda_material_scalar() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Red".into(),
            base_color_factor: [1.0, 0.0, 0.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.8,
            double_sided: true,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("def Material \"Red\""));
        assert!(usda.contains("UsdPreviewSurface"));
        assert!(usda.contains("diffuseColor = (1, 0, 0)"));
        assert!(usda.contains("metallic = 0"));
        assert!(usda.contains("roughness = 0.8"));
        assert!(usda.contains("rel material:binding = </Root/Materials/Red>"));
        assert!(usda.contains("doubleSided = 1"));
    }

    #[test]
    fn usda_material_with_texture() {
        let mut scene = triangle_scene();

        scene.textures.push(Texture {
            name: "baseColor".into(),
            data: vec![0x89, b'P', b'N', b'G'],
            format: TextureFormat::Png,
            width: 512,
            height: 512,
        });

        scene.materials.push(Material {
            name: "Textured".into(),
            base_color_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("def Shader \"diffuseColorTex\""));
        assert!(usda.contains("UsdUVTexture"));
        assert!(usda.contains("@textures/baseColor.png@"));
        assert!(usda.contains("def Shader \"stReader\""));
        assert!(usda.contains("UsdPrimvarReader_float2"));
        assert!(usda.contains("varname = \"st\""));
        assert!(usda.contains("diffuseColor.connect"));
    }

    #[test]
    fn usda_emissive() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Glow".into(),
            emissive_factor: [0.5, 0.3, 0.0],
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;
        assert!(usda.contains("emissiveColor = (0.5, 0.3, 0)"));
    }

    #[test]
    fn usda_normal_map_scale_bias() {
        let mut scene = triangle_scene();
        scene.textures.push(Texture {
            name: "normal".into(),
            data: vec![],
            format: TextureFormat::Png,
            width: 256,
            height: 256,
        });
        scene.materials.push(Material {
            name: "NormalMapped".into(),
            normal_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;
        assert!(usda.contains("def Shader \"normalTex\""));
        assert!(usda.contains("inputs:scale = (2, 2, 2, 1)"));
        assert!(usda.contains("inputs:bias = (-1, -1, -1, 0)"));
    }

    #[test]
    fn sanitize_name_cases() {
        assert_eq!(sanitize_name("hello", "x", 0), "hello");
        assert_eq!(sanitize_name("", "mesh", 5), "mesh_5");
        assert_eq!(sanitize_name("3d-model", "x", 0), "_3d_model");
        assert_eq!(sanitize_name("my mesh.001", "x", 0), "my_mesh_001");
    }

    #[test]
    fn usda_unlit_material() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Unlit".into(),
            base_color_factor: [0.8, 0.2, 0.1, 1.0],
            unlit: true,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("emissiveColor = (0.8, 0.2, 0.1)"), "falta emissiveColor");
        assert!(usda.contains("diffuseColor = (0, 0, 0)"), "diffuse debería ser negro");
        assert!(usda.contains("metallic = 0"), "metallic debería ser 0");
        assert!(usda.contains("roughness = 0.9"), "roughness debería ser 0.9");
    }

    #[test]
    fn usda_unlit_material_with_texture() {
        let mut scene = triangle_scene();
        scene.textures.push(Texture {
            name: "baseColor".into(),
            data: vec![0x89, b'P', b'N', b'G'],
            format: TextureFormat::Png,
            width: 512,
            height: 512,
        });
        scene.materials.push(Material {
            name: "UnlitTex".into(),
            base_color_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            unlit: true,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        assert!(usda.contains("def Shader \"emissiveTex\""), "falta emissiveTex shader");
        assert!(!usda.contains("def Shader \"diffuseColorTex\""), "no debería haber diffuseColorTex");
        assert!(!usda.contains("def Shader \"metallicRoughnessTex\""), "no debería haber metallicRoughnessTex");
        assert!(usda.contains("emissiveColor.connect"), "falta conexión emissiveColor");
    }

    #[test]
    fn usda_scale_factor() {
        let scene = triangle_scene();
        let opts = UsdaExportOptions {
            scale_factor: Some(100.0),
            ..default_opts()
        };
        let usda = write_usda(&scene, &opts).unwrap().usda;

        assert!(usda.contains("metersPerUnit = 0.01"));
        assert!(usda.contains("xformOp:scale = (100, 100, 100)"));
    }

    #[test]
    fn usda_flip_winding_negative_scale() {
        let mut scene = triangle_scene();
        scene.nodes[0].transform = Transform::Trs {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(-1.0, 1.0, 1.0),
        };
        let usda = write_usda(&scene, &default_opts()).unwrap().usda;

        // Con escala negativa, los índices [0, 1, 2] se convierten en [0, 2, 1]
        assert!(usda.contains("int[] faceVertexIndices = [0, 2, 1]"));
    }
}
