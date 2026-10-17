//! Vista previa de la información de un archivo 3D sin importarlo: formato, peso y lo que
//! se pueda leer barato (cabeceras y conteos), para decidir antes de esperar la importación.

use serde::Serialize;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Lo que se sabe de un archivo antes de importarlo. Los conteos que el formato no da
/// barato quedan en `None`.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct ModelPeek {
    /// Nombre del archivo
    pub name: String,
    /// Extensión en minúscula
    pub format: String,
    pub bytes: u64,
    pub vertices: Option<u64>,
    pub faces: Option<u64>,
    pub meshes: Option<u64>,
    pub materials: Option<u64>,
    pub textures: Option<u64>,
    pub animations: Option<u64>,
    /// Tiene esqueleto (skins en glTF)
    pub rigged: Option<bool>,
    /// Texto corto adicional (p. ej. «STL binario»)
    pub note: Option<String>,
}

#[tauri::command]
pub async fn peek_model(path: String) -> Result<ModelPeek, String> {
    tauri::async_runtime::spawn_blocking(move || peek(Path::new(&path))).await.map_err(|e| e.to_string())?
}

pub fn peek(path: &Path) -> Result<ModelPeek, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
    let format = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut info = ModelPeek {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        format: format.clone(),
        bytes: meta.len(),
        ..Default::default()
    };
    let file = || std::fs::File::open(path).map_err(|e| format!("No se pudo abrir el archivo: {e}"));
    match format.as_str() {
        "glb" => {
            let mut f = file()?;
            let mut head = [0u8; 20];
            f.read_exact(&mut head).map_err(|_| "GLB incompleto".to_string())?;
            if &head[0..4] != b"glTF" || &head[16..20] != b"JSON" {
                return Err("No es un GLB válido".into());
            }
            let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
            // Un JSON de decenas de MB ya no es «barato»
            if len > 256 << 20 {
                return Ok(info);
            }
            let mut json = vec![0u8; len];
            f.read_exact(&mut json).map_err(|_| "GLB incompleto".to_string())?;
            gltf_counts(&mut info, &json);
        }
        "gltf" => {
            let json = std::fs::read(path).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
            gltf_counts(&mut info, &json);
        }
        "obj" => {
            let (mut v, mut f) = (0u64, 0u64);
            for line in BufReader::new(file()?).split(b'\n').map_while(Result::ok) {
                if line.starts_with(b"v ") {
                    v += 1;
                } else if line.starts_with(b"f ") {
                    // Un polígono de n lados son n-2 triángulos
                    let corners = line[2..].split(|b| b.is_ascii_whitespace()).filter(|s| !s.is_empty()).count() as u64;
                    f += corners.saturating_sub(2).max(1);
                }
            }
            info.vertices = Some(v);
            info.faces = Some(f);
        }
        "stl" => {
            let mut f = file()?;
            let mut head = [0u8; 84];
            let n = f.read(&mut head).map_err(|e| e.to_string())?;
            let count = (n >= 84).then(|| u32::from_le_bytes([head[80], head[81], head[82], head[83]]) as u64);
            // Binario: 84 + 50 por triángulo. Si no cuadra, es de texto
            if let Some(c) = count.filter(|c| 84 + 50 * c == meta.len()) {
                info.faces = Some(c);
                info.note = Some("STL binario".into());
            } else {
                let facets = BufReader::new(file()?)
                    .split(b'\n')
                    .map_while(Result::ok)
                    .filter(|l| l.trim_ascii_start().starts_with(b"facet"))
                    .count() as u64;
                info.faces = Some(facets);
                info.note = Some("STL de texto".into());
            }
        }
        "ply" => {
            for line in BufReader::new(file()?).split(b'\n').map_while(Result::ok).take(200) {
                let text = String::from_utf8_lossy(&line);
                let mut it = text.split_whitespace();
                match (it.next(), it.next(), it.next()) {
                    (Some("element"), Some("vertex"), Some(n)) => info.vertices = n.parse().ok(),
                    (Some("element"), Some("face"), Some(n)) => info.faces = n.parse().ok(),
                    (Some("end_header"), ..) => break,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(info)
}

fn gltf_counts(info: &mut ModelPeek, json: &[u8]) {
    let Ok(doc) = serde_json::from_slice::<serde_json::Value>(json) else { return };
    let count = |key: &str| doc.get(key).and_then(|v| v.as_array()).map(|a| a.len() as u64);
    info.meshes = count("meshes");
    info.materials = count("materials");
    info.textures = count("images").or_else(|| count("textures"));
    info.animations = count("animations");
    info.rigged = Some(count("skins").is_some_and(|n| n > 0));
    // Triángulos y vértices de las primitivas (sin contar instancias de nodos)
    let accessors = doc.get("accessors").and_then(|v| v.as_array());
    let accessor_count = |i: Option<u64>| {
        i.and_then(|i| accessors?.get(i as usize)?.get("count")?.as_u64())
    };
    let (mut verts, mut tris) = (0u64, 0u64);
    for mesh in doc.get("meshes").and_then(|v| v.as_array()).into_iter().flatten() {
        for prim in mesh.get("primitives").and_then(|v| v.as_array()).into_iter().flatten() {
            if let Some(n) = accessor_count(prim.pointer("/attributes/POSITION").and_then(|v| v.as_u64())) {
                verts += n;
                tris += accessor_count(prim.get("indices").and_then(|v| v.as_u64())).unwrap_or(n) / 3;
            }
        }
    }
    if verts > 0 {
        info.vertices = Some(verts);
        info.faces = Some(tris);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pinocchio-peek-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn obj_cuenta_vertices_y_triangulos() {
        let p = tmp("a.obj", b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\nf 1 2 3 4\n");
        let info = peek(&p).unwrap();
        assert_eq!((info.vertices, info.faces), (Some(4), Some(4)));
    }

    #[test]
    fn stl_de_texto_cuenta_facetas() {
        let p = tmp("t.stl", b"solid x\nfacet normal 0 0 1\nouter loop\nendloop\nendfacet\nfacet normal 0 0 1\nendfacet\nendsolid\n");
        let info = peek(&p).unwrap();
        assert_eq!(info.faces, Some(2));
        assert_eq!(info.note.as_deref(), Some("STL de texto"));
    }

    #[test]
    fn ply_lee_la_cabecera() {
        let p = tmp("p.ply", b"ply\nformat ascii 1.0\nelement vertex 10\nelement face 5\nend_header\n");
        let info = peek(&p).unwrap();
        assert_eq!((info.vertices, info.faces), (Some(10), Some(5)));
    }

    #[test]
    fn gltf_cuenta_primitivas() {
        let json = br#"{"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
            "accessors":[{"count":8},{"count":36}],"materials":[{}],"skins":[{}]}"#;
        let p = tmp("m.gltf", json);
        let info = peek(&p).unwrap();
        assert_eq!((info.vertices, info.faces, info.materials, info.rigged), (Some(8), Some(12), Some(1), Some(true)));
    }
}
