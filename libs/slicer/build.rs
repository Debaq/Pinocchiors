//! Embebe los perfiles aplanados de `presets/` (ver tools/flatten_presets.py) y, si
//! `PINOCCHIO_SLICER_EMBED` apunta al motor compilado (o a su carpeta, con las DLL de
//! Windows), también el motor (comprimido): así la app es un solo archivo y lo extrae
//! la primera vez que lamina.
use std::fmt::Write as _;
use std::hash::Hasher as _;
use std::io::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from("pub(crate) static PRESET_FILES: &[(&str, &str, &str)] = &[\n");
    for kind in ["machine", "process", "filament"] {
        let dir = root.join(kind);
        let mut files: Vec<_> = std::fs::read_dir(&dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        files.sort();
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
            writeln!(out, "    ({kind:?}, {stem:?}, include_str!({:?})),", f.display().to_string()).unwrap();
        }
    }
    out.push_str("];\n");
    writeln!(out, "pub(crate) static PRESET_INDEX: &str = include_str!({:?});", root.join("index.json").display().to_string()).unwrap();
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("presets.rs");
    std::fs::write(dest, out).unwrap();
    embed_engine();
}

/// Comprime el motor en `OUT_DIR/engine.gz` y deja `ENGINE_GZ` y `ENGINE_ID` en `engine_embed.rs`.
///
/// `PINOCCHIO_SLICER_EMBED` es el binario del motor o una carpeta con el motor y lo que
/// necesita al lado (en Windows, sus DLL). Formato (dentro del gzip), por archivo:
/// largo del nombre (u16 LE), nombre, largo (u64 LE), contenido.
fn embed_engine() {
    println!("cargo::rustc-check-cfg=cfg(embedded_engine)");
    println!("cargo:rerun-if-env-changed=PINOCCHIO_SLICER_EMBED");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(src) = std::env::var_os("PINOCCHIO_SLICER_EMBED").filter(|s| !s.is_empty()) else {
        return;
    };
    let src = std::path::PathBuf::from(src);
    println!("cargo:rerun-if-changed={}", src.display());
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let engine_name = if windows { "pinocchio-slicer.exe" } else { "pinocchio-slicer" };
    let files: Vec<(String, std::path::PathBuf)> = if src.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(&src)
            .unwrap_or_else(|e| panic!("PINOCCHIO_SLICER_EMBED={}: {e}", src.display()))
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        v.sort();
        assert!(v.iter().any(|(n, _)| n == engine_name), "PINOCCHIO_SLICER_EMBED={}: falta {engine_name}", src.display());
        v
    } else {
        vec![(engine_name.to_string(), src.clone())]
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let gz = out_dir.join("engine.gz");
    let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&gz).unwrap(), flate2::Compression::default());
    for (name, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("PINOCCHIO_SLICER_EMBED: {}: {e}", path.display()));
        hasher.write(name.as_bytes());
        hasher.write(&bytes);
        enc.write_all(&(name.len() as u16).to_le_bytes()).unwrap();
        enc.write_all(name.as_bytes()).unwrap();
        enc.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
        enc.write_all(&bytes).unwrap();
    }
    enc.finish().unwrap();
    let id = format!("{:016x}", hasher.finish());
    let code = format!("pub(crate) static ENGINE_GZ: &[u8] = include_bytes!({:?});\npub(crate) const ENGINE_ID: &str = {id:?};\n", gz.display().to_string());
    std::fs::write(out_dir.join("engine_embed.rs"), code).unwrap();
    println!("cargo:rustc-cfg=embedded_engine");
}
