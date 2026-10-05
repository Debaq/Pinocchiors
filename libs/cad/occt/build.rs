//! Compila el puente C++ con OpenCASCADE.
//!
//! Búsqueda de OCCT, en orden:
//! 1. `OCCT_INCLUDE_DIR` / `OCCT_LIB_DIR` (CI, conda, vcpkg, compilación propia).
//! 2. Rutas comunes (Linux, Homebrew).
//! 3. pkg-config.
//!
//! Sin OCCT se compila un stub: todo devuelve "OpenCASCADE no disponible" y el
//! resto del workspace sigue compilando. `CAD_REQUIRE_OCCT=1` lo vuelve error
//! (para builds de release que no deben salir sin CAD).
//!
//! `CAD_OCCT_STUB=1` fuerza el stub (probar que todo compila sin CAD).
//! `OCCT_STATIC=1` enlaza las librerías en estático (ver F7 en el ROADMAP).

use std::env;
use std::path::{Path, PathBuf};

// Toolkits usados por el puente, en orden de dependencia (importa al enlazar estático).
const TOOLKITS: &[&str] = &[
    "TKDESTEP", "TKDE", "TKXSBase", "TKFeat", "TKOffset", "TKFillet", "TKBool", "TKBO",
    "TKMesh", "TKShHealing", "TKPrim", "TKTopAlgo", "TKGeomAlgo", "TKBRep", "TKGeomBase",
    "TKG3d", "TKG2d", "TKMath", "TKernel",
];
// Nombres anteriores a 7.8 para STEP.
const TOOLKITS_OLD_STEP: &[&str] = &["TKSTEP", "TKSTEPAttr", "TKSTEP209", "TKSTEPBase"];

fn find_include() -> Option<PathBuf> {
    if let Ok(dir) = env::var("OCCT_INCLUDE_DIR")
        && !dir.is_empty()
    {
        return Some(dir.into());
    }
    for dir in [
        "/usr/include/opencascade",
        "/usr/local/include/opencascade",
        "/opt/homebrew/include/opencascade",
        "/opt/opencascade/include/opencascade",
    ] {
        if Path::new(dir).join("Standard.hxx").exists() {
            return Some(dir.into());
        }
    }
    let out = std::process::Command::new("pkg-config")
        .args(["--cflags-only-I", "OpenCASCADE"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|f| f.strip_prefix("-I"))
        .map(PathBuf::from)
        .find(|d| d.join("Standard.hxx").exists())
}

fn lib_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = env::var("OCCT_LIB_DIR")
        && !dir.is_empty()
    {
        dirs.push(dir.into());
    }
    for d in [
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib64",
        "/usr/lib",
        "/usr/local/lib",
        "/opt/homebrew/lib",
        "/opt/opencascade/lib",
    ] {
        dirs.push(d.into());
    }
    dirs
}

fn has_lib(dirs: &[PathBuf], name: &str) -> bool {
    dirs.iter().any(|d| {
        ["lib{}.so", "lib{}.dylib", "{}.lib", "lib{}.a"]
            .iter()
            .any(|pat| d.join(pat.replace("{}", name)).exists())
    })
}

fn main() {
    println!("cargo:rerun-if-changed=cpp");
    for v in ["OCCT_INCLUDE_DIR", "OCCT_LIB_DIR", "OCCT_STATIC", "CAD_REQUIRE_OCCT", "CAD_OCCT_STUB"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    println!("cargo::rustc-check-cfg=cfg(cad_occt_stub)");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let forced_stub = env::var("CAD_OCCT_STUB").is_ok_and(|v| v == "1");
    let Some(include) = find_include().filter(|_| !forced_stub) else {
        if env::var("CAD_REQUIRE_OCCT").is_ok_and(|v| v == "1") {
            panic!("OpenCASCADE no encontrado y CAD_REQUIRE_OCCT=1. Definir OCCT_INCLUDE_DIR/OCCT_LIB_DIR.");
        }
        println!("cargo:warning=OpenCASCADE no encontrado: cad-occt se compila sin CAD (stub)");
        println!("cargo:rustc-cfg=cad_occt_stub");
        cc::Build::new().file("cpp/cad_occt_stub.c").compile("cad_occt");
        return;
    };

    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").file("cpp/cad_occt.cpp").include(&include);
    if target_os != "windows" {
        build.flag_if_supported("-w");
    } else {
        build.flag("/EHsc").flag("/bigobj");
    }
    build.compile("cad_occt");

    let dirs = lib_dirs();
    if let Ok(dir) = env::var("OCCT_LIB_DIR")
        && !dir.is_empty()
    {
        println!("cargo:rustc-link-search=native={dir}");
    }
    let kind = if env::var("OCCT_STATIC").is_ok_and(|v| v == "1") { "static" } else { "dylib" };
    let new_step = has_lib(&dirs, "TKDESTEP");
    for tk in TOOLKITS {
        if !new_step && (*tk == "TKDESTEP" || *tk == "TKDE") {
            for old in TOOLKITS_OLD_STEP {
                println!("cargo:rustc-link-lib={kind}={old}");
            }
            continue;
        }
        println!("cargo:rustc-link-lib={kind}={tk}");
    }
    match target_os.as_str() {
        "macos" => println!("cargo:rustc-link-lib=c++"),
        "windows" => {}
        _ => println!("cargo:rustc-link-lib=stdc++"),
    }
}
