//! Embebe los perfiles aplanados de `presets/` (ver tools/flatten_presets.py).
use std::fmt::Write as _;
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
}
