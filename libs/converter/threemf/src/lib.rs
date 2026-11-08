//! Exportación 3MF (3D Manufacturing Format), el formato de impresión que
//! reemplaza a STL: declara unidades, guarda malla indexada (sin vértices
//! repetidos) y colores por material.
//!
//! Cada primitiva en espacio mundo es un objeto; los materiales se escriben
//! como `basematerials` con su color base. Igual que STL, las coordenadas se
//! pasan a milímetros según `scene.meters_per_unit` y salen con Z arriba (la
//! plataforma del slicer).

use converter_scene::Scene;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Cursor, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;

#[derive(Debug, Error)]
pub enum ThreeMfExportError {
    #[error("error escribiendo 3MF: {0}")]
    Io(#[from] std::io::Error),
    #[error("error empaquetando 3MF: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("la escena no contiene geometría")]
    NoGeometry,
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>
"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
 <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>
"#;

/// Exporta una `Scene` a un archivo 3MF.
pub fn export_3mf(scene: &Scene, path: impl AsRef<Path>) -> Result<(), ThreeMfExportError> {
    let bytes = export_3mf_bytes(scene)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Exporta una `Scene` a 3MF en memoria.
pub fn export_3mf_bytes(scene: &Scene) -> Result<Vec<u8>, ThreeMfExportError> {
    let model = model_xml(scene)?;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", RELS),
        ("3D/3dmodel.model", model.as_str()),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(content.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

fn model_xml(scene: &Scene) -> Result<String, ThreeMfExportError> {
    let prims: Vec<_> = scene.world_primitives_z_up().into_iter().filter(|p| !p.triangles.is_empty()).collect();
    if prims.is_empty() {
        return Err(ThreeMfExportError::NoGeometry);
    }
    let to_mm = (scene.meters_per_unit * 1000.0) as f32;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n\
         <metadata name=\"Application\">Pinocchiors</metadata>\n<resources>\n",
    );
    // Id 1: materiales; los objetos empiezan en 2
    let has_materials = !scene.materials.is_empty();
    if has_materials {
        xml.push_str("<basematerials id=\"1\">\n");
        for (i, mat) in scene.materials.iter().enumerate() {
            let name = if mat.name.is_empty() { format!("Material {i}") } else { mat.name.clone() };
            let [r, g, b, a] = mat.base_color_factor.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let _ = writeln!(
                xml,
                " <base name=\"{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}{a:02X}\"/>",
                escape(&name)
            );
        }
        xml.push_str("</basematerials>\n");
    }

    for (i, prim) in prims.iter().enumerate() {
        let mesh_name = &scene.meshes[prim.mesh].name;
        let name = if mesh_name.is_empty() { format!("Objeto {}", i + 1) } else { mesh_name.clone() };
        let _ = write!(xml, "<object id=\"{}\" type=\"model\" name=\"{}\"", i + 2, escape(&name));
        if has_materials && let Some(m) = prim.material.filter(|&m| m < scene.materials.len()) {
            let _ = write!(xml, " pid=\"1\" pindex=\"{m}\"");
        }
        xml.push_str(">\n<mesh>\n<vertices>\n");
        let (positions, triangles) = weld(&prim.positions, &prim.triangles);
        for p in &positions {
            let [x, y, z] = p.map(|c| c * to_mm);
            let _ = writeln!(xml, "<vertex x=\"{x}\" y=\"{y}\" z=\"{z}\"/>");
        }
        xml.push_str("</vertices>\n<triangles>\n");
        for [a, b, c] in &triangles {
            // 3MF no admite triángulos degenerados
            if a != b && b != c && a != c {
                let _ = writeln!(xml, "<triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>");
            }
        }
        xml.push_str("</triangles>\n</mesh>\n</object>\n");
    }
    xml.push_str("</resources>\n<build>\n");
    for i in 0..prims.len() {
        let _ = writeln!(xml, "<item objectid=\"{}\"/>", i + 2);
    }
    xml.push_str("</build>\n</model>\n");
    Ok(xml)
}

/// Una parte de un objeto para imprimir: malla en milímetros con Z arriba.
pub struct PrintVolume<'a> {
    pub name: &'a str,
    pub positions: &'a [[f32; 3]],
    pub triangles: &'a [[u32; 3]],
    /// Volumen negativo: el laminador lo resta de lo demás (grabados).
    pub negative: bool,
}

/// Un solo objeto con varias partes (la pieza, lo agregado y los volúmenes
/// negativos), como lo guardan PrusaSlicer y OrcaSlicer: la malla de todas
/// juntas y, en `Metadata/Slic3r_PE_model.config`, el rango de triángulos y el
/// tipo de cada parte. Sin booleanas: el laminador junta lo que se toca y resta
/// lo negativo.
pub fn export_3mf_volumes(name: &str, volumes: &[PrintVolume], path: impl AsRef<Path>) -> Result<(), ThreeMfExportError> {
    std::fs::write(path, export_3mf_volumes_bytes(name, volumes)?)?;
    Ok(())
}

/// Como [`export_3mf_volumes`], en memoria.
pub fn export_3mf_volumes_bytes(name: &str, volumes: &[PrintVolume]) -> Result<Vec<u8>, ThreeMfExportError> {
    let (mut vertices, mut triangles) = (String::new(), String::new());
    let mut config = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n<object id=\"2\" instances_count=\"1\">\n");
    let _ = writeln!(config, " <metadata type=\"object\" key=\"name\" value=\"{}\"/>", escape(name));
    let (mut base, mut first) = (0u32, 0usize);
    for v in volumes {
        let (positions, tris) = weld(v.positions, v.triangles);
        // 3MF no admite triángulos degenerados (y los rangos cuentan los que quedan)
        let tris: Vec<[u32; 3]> = tris.into_iter().filter(|[a, b, c]| a != b && b != c && a != c).collect();
        if tris.is_empty() {
            continue;
        }
        for [x, y, z] in &positions {
            let _ = writeln!(vertices, "<vertex x=\"{x}\" y=\"{y}\" z=\"{z}\"/>");
        }
        for [a, b, c] in &tris {
            let _ = writeln!(triangles, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", a + base, b + base, c + base);
        }
        let last = first + tris.len() - 1;
        let kind = if v.negative { "NegativeVolume" } else { "ModelPart" };
        let _ = writeln!(config, " <volume firstid=\"{first}\" lastid=\"{last}\">");
        let _ = writeln!(config, "  <metadata type=\"volume\" key=\"name\" value=\"{}\"/>", escape(v.name));
        let _ = writeln!(config, "  <metadata type=\"volume\" key=\"volume_type\" value=\"{kind}\"/>");
        let _ = writeln!(config, "  <metadata type=\"volume\" key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/>");
        config.push_str(" </volume>\n");
        base += positions.len() as u32;
        first = last + 1;
    }
    if first == 0 {
        return Err(ThreeMfExportError::NoGeometry);
    }
    config.push_str("</object>\n</config>\n");
    let model = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" \
         xmlns:slic3rpe=\"http://schemas.slic3r.org/3mf/2017/06\">\n\
         <metadata name=\"Application\">Pinocchiors</metadata>\n\
         <metadata name=\"slic3rpe:Version3mf\">1</metadata>\n\
         <resources>\n<object id=\"2\" type=\"model\" name=\"{}\">\n<mesh>\n<vertices>\n{vertices}</vertices>\n\
         <triangles>\n{triangles}</triangles>\n</mesh>\n</object>\n</resources>\n\
         <build>\n<item objectid=\"2\"/>\n</build>\n</model>\n",
        escape(name)
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (file, content) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", RELS),
        ("3D/3dmodel.model", model.as_str()),
        ("Metadata/Slic3r_PE_model.config", config.as_str()),
    ] {
        zip.start_file(file, options)?;
        zip.write_all(content.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Une los vértices con la misma posición. Las costuras de UV y de normales
/// duplican vértices; en una malla indexada el slicer las vería como bordes
/// abiertos (STL no tiene el problema porque los slicers sueldan al leerlo).
fn weld(positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let mut index = HashMap::with_capacity(positions.len());
    let mut unique = Vec::new();
    let remap: Vec<u32> = positions
        .iter()
        .map(|p| {
            *index.entry(p.map(f32::to_bits)).or_insert_with(|| {
                unique.push(*p);
                unique.len() as u32 - 1
            })
        })
        .collect();
    let triangles = triangles.iter().map(|t| t.map(|i| remap[i as usize])).collect();
    (unique, triangles)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{IndexData, Material, Mesh, Node, Primitive, Transform, VertexAttribute};
    use std::io::Read;

    fn scene() -> Scene {
        let mut scene = Scene::new();
        scene.meters_per_unit = 0.01; // centímetros
        scene.materials.push(Material {
            name: "Rojo & brillante".into(),
            base_color_factor: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        });
        scene.meshes.push(Mesh {
            name: "pieza".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U16(vec![0, 1, 2, 0, 0, 1])),
                material: Some(0),
            }],
        });
        scene.nodes.push(Node {
            name: "pieza".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn package_has_model_in_millimeters() {
        let bytes = export_3mf_bytes(&scene()).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        for name in ["[Content_Types].xml", "_rels/.rels"] {
            assert!(archive.by_name(name).is_ok(), "falta {name}");
        }
        let mut model = String::new();
        archive.by_name("3D/3dmodel.model").unwrap().read_to_string(&mut model).unwrap();

        assert!(model.contains("unit=\"millimeter\""));
        assert!(model.contains("<vertex x=\"10\" y=\"0\" z=\"0\"/>"), "cm → mm");
        // Lo que en la escena es arriba (Y) sale arriba en la plataforma (Z)
        assert!(model.contains("<vertex x=\"0\" y=\"0\" z=\"10\"/>"), "Y arriba → Z arriba");
        assert!(model.contains("displaycolor=\"#FF0000FF\""));
        assert!(model.contains("name=\"Rojo &amp; brillante\""));
        assert!(model.contains("pid=\"1\" pindex=\"0\""));
        // El triángulo degenerado se omite
        assert_eq!(model.matches("<triangle ").count(), 1);
        assert!(model.contains("<item objectid=\"2\"/>"));
    }

    #[test]
    fn seam_vertices_are_welded() {
        // Dos triángulos que comparten arista, pero con vértices duplicados
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        let (unique, triangles) = weld(&positions, &[[0, 1, 2], [3, 5, 4]]);
        assert_eq!(unique.len(), 4);
        assert_eq!(triangles, vec![[0, 1, 2], [1, 3, 2]]);
    }

    #[test]
    fn negative_volume_keeps_its_triangle_range() {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // La primera parte trae un triángulo degenerado que no cuenta en el rango
        let a = PrintVolume { name: "escaneo", positions: &tri, triangles: &[[0, 1, 2], [0, 0, 1]], negative: false };
        let b = PrintVolume { name: "grabado", positions: &tri, triangles: &[[0, 1, 2]], negative: true };
        let bytes = export_3mf_volumes_bytes("pieza", &[a, b]).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut model = String::new();
        archive.by_name("3D/3dmodel.model").unwrap().read_to_string(&mut model).unwrap();
        let mut config = String::new();
        archive.by_name("Metadata/Slic3r_PE_model.config").unwrap().read_to_string(&mut config).unwrap();
        assert_eq!(model.matches("<triangle ").count(), 2);
        assert!(model.contains("<triangle v1=\"3\" v2=\"4\" v3=\"5\"/>"), "la segunda parte usa sus propios vértices");
        assert!(config.contains("firstid=\"0\" lastid=\"0\""));
        assert!(config.contains("firstid=\"1\" lastid=\"1\""));
        assert!(config.contains("value=\"NegativeVolume\""));
    }

    #[test]
    fn empty_scene_is_an_error() {
        assert!(matches!(export_3mf_bytes(&Scene::new()), Err(ThreeMfExportError::NoGeometry)));
    }
}
