//! Importación PLY: ASCII y binario (little y big endian).
//!
//! Lee posiciones, normales, UV por vértice (`s`/`t`, `u`/`v`,
//! `texture_u`/`texture_v`) o por esquina de cara (`texcoord`, como MeshLab),
//! colores de vértice (enteros sobre su máximo, o flotantes tal cual) y caras
//! de cualquier cantidad de lados (en abanico). Otros elementos (aristas,
//! materiales) se saltan. Se lee con Z arriba, como lo escriben Blender y los
//! programas de escaneo, y se pasa a Y arriba. La textura que declara
//! `comment TextureFile` (MeshLab) se carga si está junto al archivo.

use converter_scene::{
    z_up_to_y_up, IndexData, Material, Mesh, Node, Primitive, Scene, Texture, TextureFormat, TextureRef, Transform,
    VertexAttribute,
};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlyImportError {
    #[error("error leyendo PLY: {0}")]
    Io(#[from] std::io::Error),
    #[error("encabezado PLY inválido: {0}")]
    Header(String),
    #[error("datos PLY inválidos: {0}")]
    Data(String),
    #[error("el PLY no tiene caras (¿una nube de puntos?)")]
    NoFaces,
}

/// Importa un archivo PLY. Carga la textura de `comment TextureFile` si
/// existe junto al archivo.
pub fn import_ply(path: impl AsRef<Path>) -> Result<Scene, PlyImportError> {
    let path = path.as_ref();
    let data = std::fs::read(path)?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("ply_mesh");
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    parse(&data, name, |file| std::fs::read(dir.join(file)).ok())
}

/// Importa un PLY desde bytes en memoria (sin texturas externas).
pub fn import_ply_bytes(data: &[u8]) -> Result<Scene, PlyImportError> {
    parse(data, "ply_mesh", |_| None)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Encoding {
    Ascii,
    LittleEndian,
    BigEndian,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Scalar {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Scalar {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "char" | "int8" => Scalar::I8,
            "uchar" | "uint8" => Scalar::U8,
            "short" | "int16" => Scalar::I16,
            "ushort" | "uint16" => Scalar::U16,
            "int" | "int32" => Scalar::I32,
            "uint" | "uint32" => Scalar::U32,
            "float" | "float32" => Scalar::F32,
            "double" | "float64" => Scalar::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Scalar::I8 | Scalar::U8 => 1,
            Scalar::I16 | Scalar::U16 => 2,
            Scalar::I32 | Scalar::U32 | Scalar::F32 => 4,
            Scalar::F64 => 8,
        }
    }

    /// Escala de un color guardado con este tipo: enteros sin signo sobre su
    /// máximo, flotantes tal cual.
    fn color_scale(self) -> f64 {
        match self {
            Scalar::U8 | Scalar::I8 => 1.0 / 255.0,
            Scalar::U16 | Scalar::I16 => 1.0 / 65535.0,
            Scalar::U32 | Scalar::I32 => 1.0 / u32::MAX as f64,
            Scalar::F32 | Scalar::F64 => 1.0,
        }
    }
}

#[derive(Debug, Clone)]
enum Property {
    Scalar { name: String, ty: Scalar },
    List { name: String, count: Scalar, item: Scalar },
}

#[derive(Debug, Clone)]
struct Element {
    name: String,
    count: usize,
    properties: Vec<Property>,
}

struct Header {
    encoding: Encoding,
    elements: Vec<Element>,
    texture_file: Option<String>,
    /// Byte donde empiezan los datos.
    body: usize,
}

fn parse_header(data: &[u8]) -> Result<Header, PlyImportError> {
    let bad = |m: &str| PlyImportError::Header(m.to_string());
    if !data.starts_with(b"ply") {
        return Err(bad("no empieza con \"ply\""));
    }
    let mut encoding = None;
    let mut elements: Vec<Element> = Vec::new();
    let mut texture_file = None;
    let mut pos = 0;
    loop {
        let end = data[pos..].iter().position(|&b| b == b'\n').ok_or_else(|| bad("falta end_header"))? + pos;
        let line = String::from_utf8_lossy(&data[pos..end]);
        let line = line.trim();
        pos = end + 1;
        let mut words = line.split_whitespace();
        match words.next() {
            Some("format") => {
                encoding = Some(match words.next() {
                    Some("ascii") => Encoding::Ascii,
                    Some("binary_little_endian") => Encoding::LittleEndian,
                    Some("binary_big_endian") => Encoding::BigEndian,
                    other => return Err(bad(&format!("formato desconocido {other:?}"))),
                });
            }
            Some("comment") => {
                let rest: Vec<&str> = words.collect();
                if rest.first().is_some_and(|w| w.eq_ignore_ascii_case("texturefile")) && rest.len() > 1 {
                    texture_file = Some(rest[1..].join(" "));
                }
            }
            Some("element") => {
                let name = words.next().ok_or_else(|| bad("element sin nombre"))?.to_string();
                let count = words.next().and_then(|c| c.parse().ok()).ok_or_else(|| bad("element sin cantidad"))?;
                elements.push(Element { name, count, properties: Vec::new() });
            }
            Some("property") => {
                let element = elements.last_mut().ok_or_else(|| bad("property antes de element"))?;
                let ty = words.next().ok_or_else(|| bad("property sin tipo"))?;
                let property = if ty == "list" {
                    let count = words.next().and_then(Scalar::parse).ok_or_else(|| bad("tipo de cantidad inválido"))?;
                    let item = words.next().and_then(Scalar::parse).ok_or_else(|| bad("tipo de lista inválido"))?;
                    let name = words.next().ok_or_else(|| bad("lista sin nombre"))?.to_string();
                    Property::List { name, count, item }
                } else {
                    let ty = Scalar::parse(ty).ok_or_else(|| bad(&format!("tipo desconocido {ty}")))?;
                    let name = words.next().ok_or_else(|| bad("property sin nombre"))?.to_string();
                    Property::Scalar { name, ty }
                };
                element.properties.push(property);
            }
            Some("end_header") => break,
            _ => {}
        }
    }
    let encoding = encoding.ok_or_else(|| bad("falta la línea format"))?;
    Ok(Header { encoding, elements, texture_file, body: pos })
}

/// Lector de valores del cuerpo, en texto o binario.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    encoding: Encoding,
}

impl Reader<'_> {
    fn read(&mut self, ty: Scalar) -> Result<f64, PlyImportError> {
        if self.encoding == Encoding::Ascii {
            return self.read_word();
        }
        let size = ty.size();
        let bytes = self
            .data
            .get(self.pos..self.pos + size)
            .ok_or_else(|| PlyImportError::Data("el archivo termina antes de tiempo".into()))?;
        self.pos += size;
        let mut buf = [0u8; 8];
        buf[..size].copy_from_slice(bytes);
        if self.encoding == Encoding::BigEndian {
            buf[..size].reverse();
        }
        Ok(match ty {
            Scalar::I8 => buf[0] as i8 as f64,
            Scalar::U8 => buf[0] as f64,
            Scalar::I16 => i16::from_le_bytes([buf[0], buf[1]]) as f64,
            Scalar::U16 => u16::from_le_bytes([buf[0], buf[1]]) as f64,
            Scalar::I32 => i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64,
            Scalar::U32 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64,
            Scalar::F32 => f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64,
            Scalar::F64 => f64::from_le_bytes(buf),
        })
    }

    fn read_word(&mut self) -> Result<f64, PlyImportError> {
        while self.data.get(self.pos).is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
        let start = self.pos;
        while self.data.get(self.pos).is_some_and(|b| !b.is_ascii_whitespace()) {
            self.pos += 1;
        }
        let word = std::str::from_utf8(&self.data[start..self.pos]).unwrap_or("");
        word.parse().map_err(|_| {
            PlyImportError::Data(if word.is_empty() { "el archivo termina antes de tiempo".into() } else { format!("número inválido {word:?}") })
        })
    }
}

/// Índice de la primera propiedad escalar con alguno de los nombres.
fn find(element: &Element, names: &[&str]) -> Option<(usize, Scalar)> {
    element.properties.iter().enumerate().find_map(|(i, p)| match p {
        Property::Scalar { name, ty } if names.contains(&name.as_str()) => Some((i, *ty)),
        _ => None,
    })
}

fn find_list(element: &Element, names: &[&str]) -> Option<usize> {
    element.properties.iter().position(|p| matches!(p, Property::List { name, .. } if names.contains(&name.as_str())))
}

/// Valores de una fila de un elemento: escalares y listas.
struct Row {
    scalars: Vec<f64>,
    lists: Vec<Vec<f64>>,
}

fn read_row(reader: &mut Reader, element: &Element, row: &mut Row) -> Result<(), PlyImportError> {
    for (k, property) in element.properties.iter().enumerate() {
        match property {
            Property::Scalar { ty, .. } => row.scalars[k] = reader.read(*ty)?,
            Property::List { count, item, .. } => {
                let n = reader.read(*count)?;
                if !(0.0..=1e6).contains(&n) {
                    return Err(PlyImportError::Data(format!("lista de {n} elementos")));
                }
                let list = &mut row.lists[k];
                list.clear();
                for _ in 0..n as usize {
                    list.push(reader.read(*item)?);
                }
            }
        }
    }
    Ok(())
}

fn parse(data: &[u8], name: &str, load: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Scene, PlyImportError> {
    let header = parse_header(data)?;
    let mut reader = Reader { data, pos: header.body, encoding: header.encoding };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    // Caras: índices de vértice y, si las hay, UV por esquina
    let mut faces: Vec<Vec<usize>> = Vec::new();
    let mut corner_uvs: Vec<Vec<[f32; 2]>> = Vec::new();

    for element in &header.elements {
        let mut row = Row { scalars: vec![0.0; element.properties.len()], lists: vec![Vec::new(); element.properties.len()] };
        match element.name.as_str() {
            "vertex" => {
                let xyz = [&["x"][..], &["y"], &["z"]].map(|n| find(element, n));
                let Some(xyz) = xyz.into_iter().collect::<Option<Vec<_>>>() else {
                    return Err(PlyImportError::Header("el vértice no tiene x, y, z".into()));
                };
                let normal = [&["nx"][..], &["ny"], &["nz"]].map(|n| find(element, n)).into_iter().collect::<Option<Vec<_>>>();
                let uv = [&["s", "u", "texture_u", "texture_s"][..], &["t", "v", "texture_v", "texture_t"]]
                    .map(|n| find(element, n))
                    .into_iter()
                    .collect::<Option<Vec<_>>>();
                let rgb = [&["red", "r", "diffuse_red"][..], &["green", "g", "diffuse_green"], &["blue", "b", "diffuse_blue"]]
                    .map(|n| find(element, n))
                    .into_iter()
                    .collect::<Option<Vec<_>>>();
                let alpha = find(element, &["alpha", "a"]);
                for _ in 0..element.count {
                    read_row(&mut reader, element, &mut row)?;
                    let get = |(k, _): (usize, Scalar)| row.scalars[k] as f32;
                    positions.push(z_up_to_y_up([get(xyz[0]), get(xyz[1]), get(xyz[2])]));
                    if let Some(n) = &normal {
                        normals.push(z_up_to_y_up([get(n[0]), get(n[1]), get(n[2])]));
                    }
                    if let Some(t) = &uv {
                        // PLY tiene el origen de textura abajo a la izquierda, glTF arriba
                        uvs.push([get(t[0]), 1.0 - get(t[1])]);
                    }
                    if let Some(c) = &rgb {
                        let channel = |(k, ty): (usize, Scalar)| (row.scalars[k] * ty.color_scale()).clamp(0.0, 1.0) as f32;
                        colors.push([channel(c[0]), channel(c[1]), channel(c[2]), alpha.map_or(1.0, channel)]);
                    }
                }
            }
            "face" => {
                let indices = find_list(element, &["vertex_indices", "vertex_index"])
                    .ok_or_else(|| PlyImportError::Header("la cara no tiene vertex_indices".into()))?;
                let texcoord = find_list(element, &["texcoord"]);
                for _ in 0..element.count {
                    read_row(&mut reader, element, &mut row)?;
                    let face: Vec<usize> = row.lists[indices].iter().map(|&i| i as usize).collect();
                    if let Some(t) = texcoord {
                        let list = &row.lists[t];
                        let uv = (0..face.len())
                            .map(|c| match (list.get(2 * c), list.get(2 * c + 1)) {
                                (Some(&u), Some(&v)) => [u as f32, 1.0 - v as f32],
                                _ => [0.0, 0.0],
                            })
                            .collect();
                        corner_uvs.push(uv);
                    }
                    faces.push(face);
                }
            }
            _ => {
                for _ in 0..element.count {
                    read_row(&mut reader, element, &mut row)?;
                }
            }
        }
    }

    let count = positions.len();
    faces.retain(|f| f.len() >= 3);
    if faces.is_empty() || count == 0 {
        return Err(PlyImportError::NoFaces);
    }
    if faces.iter().flatten().any(|&i| i >= count) {
        return Err(PlyImportError::Data("una cara apunta a un vértice que no existe".into()));
    }

    // Con UV por esquina, un vértice se duplica por cada UV distinta
    let with_corner_uvs = corner_uvs.len() == faces.len();
    let mut vertex_of: HashMap<(usize, [u32; 2]), u32> = HashMap::new();
    let mut remap: Vec<usize> = Vec::new();
    let mut new_uvs: Vec<[f32; 2]> = Vec::new();
    let mut triangles: Vec<u32> = Vec::new();
    for (f, face) in faces.iter().enumerate() {
        let ids: Vec<u32> = if with_corner_uvs {
            face.iter()
                .zip(&corner_uvs[f])
                .map(|(&v, uv)| {
                    *vertex_of.entry((v, uv.map(f32::to_bits))).or_insert_with(|| {
                        remap.push(v);
                        new_uvs.push(*uv);
                        (remap.len() - 1) as u32
                    })
                })
                .collect()
        } else {
            face.iter().map(|&v| v as u32).collect()
        };
        for k in 1..ids.len() - 1 {
            triangles.extend([ids[0], ids[k], ids[k + 1]]);
        }
    }
    let pick = |values: Vec<[f32; 3]>| -> Vec<[f32; 3]> {
        if with_corner_uvs { remap.iter().map(|&v| values[v]).collect() } else { values }
    };
    let has_normals = normals.len() == count;
    let has_colors = colors.len() == count;
    let colors = if with_corner_uvs && has_colors { remap.iter().map(|&v| colors[v]).collect() } else { colors };
    let uvs = if with_corner_uvs { Some(new_uvs) } else { (uvs.len() == count).then_some(uvs) };

    let mut attributes = vec![VertexAttribute::Positions(pick(positions))];
    if has_normals {
        attributes.push(VertexAttribute::Normals(pick(normals)));
    }
    let has_uvs = uvs.is_some();
    if let Some(uvs) = uvs {
        attributes.push(VertexAttribute::TexCoords(0, uvs));
    }
    if has_colors {
        attributes.push(VertexAttribute::Colors(colors));
    }

    let mut scene = Scene::new();
    // Textura de MeshLab: un material que la usa como color base
    let texture = header.texture_file.as_deref().filter(|_| has_uvs).and_then(|file| {
        let data = load(file)?;
        let format = if data.starts_with(b"\x89PNG") {
            TextureFormat::Png
        } else if data.starts_with(b"\xFF\xD8\xFF") {
            TextureFormat::Jpeg
        } else if data.len() > 12 && &data[8..12] == b"WEBP" {
            TextureFormat::WebP
        } else {
            return None;
        };
        Some(Texture { name: file.to_string(), data, format, width: 0, height: 0 })
    });
    let material = texture.map(|texture| {
        scene.textures.push(texture);
        scene.materials.push(Material {
            name: name.to_string(),
            base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
            metallic_factor: 0.0,
            ..Material::default()
        });
        0
    });

    scene.meshes.push(Mesh {
        name: name.to_string(),
        primitives: vec![Primitive { attributes, indices: Some(IndexData::U32(triangles)), material }],
    });
    scene.nodes.push(Node { name: name.to_string(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
    scene.root_nodes.push(0);
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_ply_bytes;

    fn primitive(scene: &Scene) -> &Primitive {
        &scene.meshes[0].primitives[0]
    }

    fn attribute<'a, T>(scene: &'a Scene, f: impl Fn(&'a VertexAttribute) -> Option<&'a T>) -> Option<&'a T> {
        primitive(scene).attributes.iter().find_map(f)
    }

    fn positions(scene: &Scene) -> &Vec<[f32; 3]> {
        attribute(scene, |a| if let VertexAttribute::Positions(p) = a { Some(p) } else { None }).unwrap()
    }

    fn indices(scene: &Scene) -> Vec<u32> {
        match &primitive(scene).indices {
            Some(IndexData::U32(i)) => i.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ascii_quad_with_colors() {
        let text = "ply\nformat ascii 1.0\ncomment hecho a mano\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\n\
                    property uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n\
                    0 0 0 255 0 0\n1 0 0 0 255 0\n1 1 0 0 0 255\n0 1 0 255 255 255\n4 0 1 2 3\n";
        let scene = import_ply_bytes(text.as_bytes()).unwrap();
        // Z arriba → Y arriba: (x, y, z) → (x, z, −y)
        assert_eq!(positions(&scene)[2], [1.0, 0.0, -1.0]);
        // El quad se abre en dos triángulos
        assert_eq!(indices(&scene), vec![0, 1, 2, 0, 2, 3]);
        let colors = attribute(&scene, |a| if let VertexAttribute::Colors(c) = a { Some(c) } else { None }).unwrap();
        assert_eq!(colors[1], [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn big_endian_skips_unknown_elements() {
        let mut data = b"ply\nformat binary_big_endian 1.0\nelement vertex 3\nproperty double x\nproperty double y\nproperty double z\n\
                         element face 1\nproperty list uchar uint vertex_indices\nproperty uchar flags\n\
                         element edge 1\nproperty int vertex1\nproperty int vertex2\nend_header\n"
            .to_vec();
        for p in [[0.0f64, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                data.extend(c.to_be_bytes());
            }
        }
        data.push(3);
        for i in [0u32, 1, 2] {
            data.extend(i.to_be_bytes());
        }
        data.push(7);
        data.extend(0i32.to_be_bytes());
        data.extend(1i32.to_be_bytes());
        let scene = import_ply_bytes(&data).unwrap();
        assert_eq!(positions(&scene)[1], [2.0, 0.0, 0.0]);
        assert_eq!(positions(&scene)[2], [0.0, 0.0, -3.0]);
        assert_eq!(indices(&scene), vec![0, 1, 2]);
    }

    #[test]
    fn corner_texcoords_split_vertices() {
        // Dos triángulos que comparten una arista con UV distintas a cada lado
        let text = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\n\
                    element face 2\nproperty list uchar int vertex_indices\nproperty list uchar float texcoord\nend_header\n\
                    0 0 0\n1 0 0\n1 1 0\n0 1 0\n\
                    3 0 1 2 6 0 0 0.5 0 0.5 0.5\n3 0 2 3 6 0.6 0 1 1 0.6 1\n";
        let scene = import_ply_bytes(text.as_bytes()).unwrap();
        // Los vértices 0 y 2 se duplican: 6 vértices
        assert_eq!(positions(&scene).len(), 6);
        let uvs = attribute(&scene, |a| if let VertexAttribute::TexCoords(0, t) = a { Some(t) } else { None }).unwrap();
        // v invertida: PLY abajo a la izquierda → glTF arriba a la izquierda
        assert_eq!(uvs[1], [0.5, 1.0]);
        assert_eq!(uvs[3], [0.6, 1.0]);
    }

    #[test]
    fn roundtrip_with_the_exporter() {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.5]]),
                    VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; 3]),
                    VertexAttribute::TexCoords(0, vec![[0.25, 0.75]; 3]),
                    VertexAttribute::Colors(vec![[1.0, 0.0, 0.0, 1.0]; 3]),
                ],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node { name: "tri".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        scene.root_nodes.push(0);

        let back = import_ply_bytes(&export_ply_bytes(&scene).unwrap()).unwrap();
        let p = positions(&back);
        assert!(p[2].iter().zip([0.0, 1.0, 0.5]).all(|(a, b)| (a - b).abs() < 1e-6), "{p:?}");
        let n = attribute(&back, |a| if let VertexAttribute::Normals(n) = a { Some(n) } else { None }).unwrap();
        assert!((n[0][2] - 1.0).abs() < 1e-6);
        let uv = attribute(&back, |a| if let VertexAttribute::TexCoords(0, t) = a { Some(t) } else { None }).unwrap();
        assert!((uv[0][0] - 0.25).abs() < 1e-6 && (uv[0][1] - 0.75).abs() < 1e-6);
        assert_eq!(indices(&back), vec![0, 1, 2]);
    }

    #[test]
    fn meshlab_texture_file_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        // PNG mínimo: basta la firma para reconocer el formato
        std::fs::write(dir.path().join("piel.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        let text = "ply\nformat ascii 1.0\ncomment TextureFile piel.png\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\n\
                    element face 1\nproperty list uchar int vertex_indices\nproperty list uchar float texcoord\nend_header\n\
                    0 0 0\n1 0 0\n0 1 0\n3 0 1 2 6 0 0 1 0 0 1\n";
        let path = dir.path().join("escaneo.ply");
        std::fs::write(&path, text).unwrap();
        let scene = import_ply(&path).unwrap();
        assert_eq!(scene.textures.len(), 1);
        assert_eq!(scene.textures[0].format, TextureFormat::Png);
        assert_eq!(primitive(&scene).material, Some(0));
        assert!(scene.materials[0].base_color_texture.is_some());
        // Sin el archivo de textura se importa igual, sin material
        std::fs::remove_file(dir.path().join("piel.png")).unwrap();
        let scene = import_ply(&path).unwrap();
        assert!(scene.textures.is_empty() && primitive(&scene).material.is_none());
    }

    #[test]
    fn point_cloud_and_bad_indices_are_errors() {
        let cloud = "ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0 0\n";
        assert!(matches!(import_ply_bytes(cloud.as_bytes()), Err(PlyImportError::NoFaces)));
        let bad = "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\n\
                   element face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 9\n";
        assert!(matches!(import_ply_bytes(bad.as_bytes()), Err(PlyImportError::Data(_))));
        let short = "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0\n";
        assert!(import_ply_bytes(short.as_bytes()).is_err());
    }
}
