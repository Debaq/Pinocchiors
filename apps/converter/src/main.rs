use clap::Parser;
use converter_core::{ConvertOptions, Format};
use std::path::{Path, PathBuf};
use std::process;

/// Conversor de formatos 3D.
///
/// Convierte entre glTF/GLB, USDA, USDZ, STL y OBJ.
///
/// Uso: `converter <entrada> [salida]` o `converter <entrada> --format <fmt>`.
/// Con `--batch` se aceptan varios archivos de entrada.
#[derive(Parser)]
#[command(name = "converter", version, about)]
struct Cli {
    /// Archivo de entrada y, opcionalmente, archivo de salida. Si no se da la
    /// salida, se usa el nombre de la entrada con la extensión de `--format`.
    /// Con `--batch`, todos son archivos de entrada.
    #[arg(required = true, num_args = 1..)]
    paths: Vec<PathBuf>,

    /// Factor de escala (e.g. 100 para metros→centímetros).
    #[arg(long)]
    scale: Option<f64>,

    /// Tamaño máximo de textura (ancho o alto).
    #[arg(long, alias = "texture-max-size")]
    max_texture_size: Option<u32>,

    /// No exportar animaciones.
    #[arg(long)]
    no_animations: bool,

    /// Compatibilidad AR Quick Look (Apple).
    #[arg(long)]
    arkit: bool,

    /// Separar textura ORM en canales individuales.
    #[arg(long)]
    split_orm: bool,

    /// Frames por segundo para animaciones USD.
    #[arg(long, default_value = "24")]
    fps: f64,

    /// Calidad JPEG para texturas (1-100). Solo GLB.
    #[arg(long)]
    texture_quality: Option<u8>,

    /// Deduplicar vértices, optimizar índices, strip degenerados. Solo GLB.
    #[arg(long)]
    optimize_geometry: bool,

    /// Generar normales si faltan. Solo GLB.
    #[arg(long)]
    generate_normals: bool,

    /// Bakear transforms en geometría. Solo GLB (no aplica con skeletons).
    #[arg(long)]
    flatten_transforms: bool,

    /// Eliminar materiales/texturas no referenciados. Solo GLB.
    #[arg(long)]
    strip_unused: bool,

    /// Modo batch: convertir varios archivos de entrada (requiere `--format`).
    #[arg(long)]
    batch: bool,

    /// Formato de salida: glb, usda, usdz, stl, obj. Obligatorio si no se da
    /// archivo de salida y en modo batch.
    #[arg(long)]
    format: Option<String>,

    /// Directorio de salida para el modo batch (por defecto, junto a cada entrada).
    #[arg(long, requires = "batch")]
    out_dir: Option<PathBuf>,
}

/// Normaliza y valida un formato de salida (`"GLB"`, `".glb"` → `"glb"`)
fn parse_format(format: &str) -> Result<String, String> {
    let ext = format.trim_start_matches('.').to_ascii_lowercase();
    match Format::from_extension(Path::new("x").with_extension(&ext)) {
        Some(f) if f.can_export() => Ok(ext),
        _ => Err(format!(
            "formato de salida no soportado: {format} (usa glb, usda, usdz, stl u obj)"
        )),
    }
}

/// Pares (entrada, salida) a convertir
fn plan(cli: &Cli) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let format = cli.format.as_deref().map(parse_format).transpose()?;

    if cli.batch {
        let ext = format.ok_or("el modo batch requiere --format")?;
        return Ok(cli
            .paths
            .iter()
            .map(|input| {
                let output = match &cli.out_dir {
                    Some(dir) => dir.join(input.file_name().unwrap_or_default()).with_extension(&ext),
                    None => input.with_extension(&ext),
                };
                (input.clone(), output)
            })
            .collect());
    }

    match cli.paths.as_slice() {
        [input] => {
            let ext = format.ok_or("falta el archivo de salida o --format")?;
            let output = input.with_extension(&ext);
            if &output == input {
                return Err(format!("la salida sobrescribiría la entrada: {}", input.display()));
            }
            Ok(vec![(input.clone(), output)])
        }
        [input, output] => {
            if let Some(ext) = format {
                let out_ext = output.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
                if out_ext.as_deref() != Some(ext.as_str()) {
                    return Err(format!(
                        "--format {ext} no coincide con la extensión de {}",
                        output.display()
                    ));
                }
            }
            Ok(vec![(input.clone(), output.clone())])
        }
        _ => Err("sin --batch se acepta una entrada y una salida; para varios archivos usa --batch".into()),
    }
}

fn main() {
    let cli = Cli::parse();

    let options = ConvertOptions {
        scale_factor: cli.scale,
        max_texture_size: cli.max_texture_size,
        split_orm_channels: cli.split_orm,
        arkit_compatible: cli.arkit,
        fps: cli.fps,
        export_animations: !cli.no_animations,
        keyframe_tolerance: 1e-4,
        texture_quality: cli.texture_quality,
        optimize_geometry: cli.optimize_geometry,
        generate_normals: cli.generate_normals,
        flatten_transforms: cli.flatten_transforms,
        strip_unused: cli.strip_unused,
    };

    let jobs = match plan(&cli) {
        Ok(jobs) => jobs,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(2);
        }
    };

    let mut failed = 0;
    for (input, output) in &jobs {
        match converter_core::convert(input, output, &options) {
            Ok(()) => eprintln!("Convertido: {} → {}", input.display(), output.display()),
            Err(e) => {
                eprintln!("Error convirtiendo {}: {e}", input.display());
                failed += 1;
            }
        }
    }

    if jobs.len() > 1 {
        eprintln!("{} de {} archivos convertidos", jobs.len() - failed, jobs.len());
    }
    if failed > 0 {
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_of(args: &[&str]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
        let cli = Cli::try_parse_from(std::iter::once("converter").chain(args.iter().copied()))
            .map_err(|e| e.to_string())?;
        plan(&cli)
    }

    fn pair(a: &str, b: &str) -> (PathBuf, PathBuf) {
        (PathBuf::from(a), PathBuf::from(b))
    }

    #[test]
    fn input_and_output() {
        assert_eq!(plan_of(&["a.glb", "b.usdz"]).unwrap(), vec![pair("a.glb", "b.usdz")]);
    }

    #[test]
    fn output_from_format() {
        assert_eq!(plan_of(&["a.glb", "--format", "USDZ"]).unwrap(), vec![pair("a.glb", "a.usdz")]);
        assert!(plan_of(&["a.glb"]).is_err());
        assert!(plan_of(&["a.glb", "--format", "glb"]).is_err(), "sobrescribiría la entrada");
    }

    #[test]
    fn format_must_match_output() {
        assert!(plan_of(&["a.glb", "b.stl", "--format", "obj"]).is_err());
        assert!(plan_of(&["a.glb", "b.stl", "--format", ".stl"]).is_ok());
        assert!(plan_of(&["a.glb", "--format", "fbx"]).is_err());
    }

    #[test]
    fn batch_many_inputs() {
        let jobs = plan_of(&["--batch", "--format", "stl", "x/a.glb", "b.obj"]).unwrap();
        assert_eq!(jobs, vec![pair("x/a.glb", "x/a.stl"), pair("b.obj", "b.stl")]);

        let jobs = plan_of(&["--batch", "--format", "stl", "--out-dir", "out", "x/a.glb"]).unwrap();
        assert_eq!(jobs, vec![pair("x/a.glb", "out/a.stl")]);

        assert!(plan_of(&["--batch", "a.glb", "b.glb"]).is_err(), "batch sin --format");
        assert!(plan_of(&["a.glb", "b.glb", "c.glb"]).is_err(), "varias entradas sin --batch");
    }
}
