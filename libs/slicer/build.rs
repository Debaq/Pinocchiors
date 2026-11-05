//! Embebe los perfiles aplanados de `presets/` (ver tools/flatten_presets.py) y, si
//! `PINOCCHIO_SLICER_EMBED` apunta al motor compilado, también el motor (comprimido):
//! así la app es un solo archivo y lo extrae la primera vez que lamina.
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
fn embed_engine() {
    println!("cargo::rustc-check-cfg=cfg(embedded_engine)");
    println!("cargo:rerun-if-env-changed=PINOCCHIO_SLICER_EMBED");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(src) = std::env::var_os("PINOCCHIO_SLICER_EMBED").filter(|s| !s.is_empty()) else {
        return;
    };
    let src = std::path::PathBuf::from(src);
    println!("cargo:rerun-if-changed={}", src.display());
    let bytes = std::fs::read(&src).unwrap_or_else(|e| panic!("PINOCCHIO_SLICER_EMBED={}: {e}", src.display()));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(&bytes);
    let id = format!("{:016x}", hasher.finish());
    let gz = out_dir.join("engine.gz");
    let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&gz).unwrap(), flate2::Compression::default());
    enc.write_all(&bytes).unwrap();
    enc.finish().unwrap();
    let code = format!("pub(crate) static ENGINE_GZ: &[u8] = include_bytes!({:?});\npub(crate) const ENGINE_ID: &str = {id:?};\n", gz.display().to_string());
    std::fs::write(out_dir.join("engine_embed.rs"), code).unwrap();
    println!("cargo:rustc-cfg=embedded_engine");
}
