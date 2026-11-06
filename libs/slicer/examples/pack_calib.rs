//! Pasa los modelos de calibración de Orca (`resources/calib`, Draco y 3MF) al formato que
//! embebe el laminador (`calib/<nombre>.bin.gz`): una lista de piezas con nombre y triángulos.
//!
//!   cargo run -p pinocchio-slicer --example pack_calib -- <orca>/resources/calib libs/slicer/calib
//!
//! Formato (little endian, comprimido con gzip): u32 piezas; por pieza: u32 largo del nombre,
//! el nombre en UTF-8, u32 triángulos y 9 f32 por triángulo (mm, Z arriba, como en Orca).

use std::io::{Read, Write};
use std::path::Path;

type Tri = [[f32; 3]; 3];

fn draco(path: &Path) -> Vec<(String, Vec<Tri>)> {
    use draco_core::{DecoderBuffer, FaceIndex, Mesh, MeshDecoder};
    let data = std::fs::read(path).expect("leer .drc");
    let mut mesh = Mesh::new();
    MeshDecoder::new().decode(&mut DecoderBuffer::new(&data), &mut mesh).expect("decodificar Draco");
    let n = mesh.num_points();
    // El primer atributo es la posición
    let pos = mesh.attribute(0).read_f32s(n, 3);
    let p = |i: u32| [pos[3 * i as usize], pos[3 * i as usize + 1], pos[3 * i as usize + 2]];
    let tris = (0..mesh.num_faces()).map(|f| mesh.face(FaceIndex(f as u32)).map(|v| p(v.0))).collect();
    let name = path.file_stem().unwrap().to_string_lossy().into_owned();
    vec![(name, tris)]
}

/// Atributo `name="…"` de una etiqueta
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

fn threemf(path: &Path) -> Vec<(String, Vec<Tri>)> {
    let file = std::fs::File::open(path).expect("abrir 3MF");
    let mut zip = zip::ZipArchive::new(file).expect("3MF");
    let mut xml = String::new();
    zip.by_name("3D/3dmodel.model").expect("3dmodel.model").read_to_string(&mut xml).unwrap();
    let mut out = Vec::new();
    for chunk in xml.split("<object ").skip(1) {
        let head = &chunk[..chunk.find('>').unwrap()];
        let name = attr(&format!(" {head}"), "name").unwrap_or("pieza").to_string();
        let body = &chunk[..chunk.find("</object>").unwrap_or(chunk.len())];
        let mut verts: Vec<[f32; 3]> = Vec::new();
        for v in body.split("<vertex ").skip(1) {
            let tag = format!(" {}", &v[..v.find('>').unwrap()]);
            let f = |k| attr(&tag, k).unwrap().parse::<f32>().unwrap();
            verts.push([f("x"), f("y"), f("z")]);
        }
        let mut tris = Vec::new();
        for t in body.split("<triangle ").skip(1) {
            let tag = format!(" {}", &t[..t.find('>').unwrap()]);
            let i = |k| attr(&tag, k).unwrap().parse::<usize>().unwrap();
            tris.push([verts[i("v1")], verts[i("v2")], verts[i("v3")]]);
        }
        if !tris.is_empty() {
            out.push((name, tris));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, dst) = (Path::new(&args[1]), Path::new(&args[2]));
    std::fs::create_dir_all(dst).unwrap();
    let files = [
        ("temperature_tower", "temperature_tower/temperature_tower.drc"),
        ("pa_tower", "pressure_advance/tower_with_seam.drc"),
        ("retraction_tower", "retraction/retraction_tower.drc"),
        ("volumetric_speed", "volumetric_speed/SpeedTestStructure.drc"),
        ("vfa", "vfa/vfa.drc"),
        ("ringing_tower", "input_shaping/ringing_tower.drc"),
        ("fast_tower", "input_shaping/fast_tower_test.drc"),
        ("cornering", "cornering/SCV-V2.drc"),
        ("flow_coarse", "filament_flow/Orca-LinearFlow.3mf"),
        ("flow_fine", "filament_flow/Orca-LinearFlow_fine.3mf"),
    ];
    for (name, rel) in files {
        let path = src.join(rel);
        let parts = if rel.ends_with(".drc") { draco(&path) } else { threemf(&path) };
        let mut raw = Vec::new();
        raw.extend((parts.len() as u32).to_le_bytes());
        for (n, tris) in &parts {
            raw.extend((n.len() as u32).to_le_bytes());
            raw.extend(n.as_bytes());
            raw.extend((tris.len() as u32).to_le_bytes());
            for t in tris {
                for p in t {
                    for c in p {
                        raw.extend(c.to_le_bytes());
                    }
                }
            }
        }
        let out = dst.join(format!("{name}.bin.gz"));
        let mut gz = flate2::write::GzEncoder::new(std::fs::File::create(&out).unwrap(), flate2::Compression::best());
        gz.write_all(&raw).unwrap();
        gz.finish().unwrap();
        let tris: usize = parts.iter().map(|p| p.1.len()).sum();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for t in parts.iter().flat_map(|p| p.1.iter()) {
            for p in t {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        println!("{name}: {} piezas, {tris} triángulos, {:.1} × {:.1} × {:.1} mm (z {:.1}..{:.1}), {} KB", parts.len(), hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], lo[2], hi[2], std::fs::metadata(&out).unwrap().len() / 1024);
    }
}
