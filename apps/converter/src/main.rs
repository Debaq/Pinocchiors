use clap::Parser;
use converter_core::ConvertOptions;
use std::path::PathBuf;
use std::process;

/// Conversor de formatos 3D.
///
/// Convierte entre glTF/GLB, USDA, USDZ, STL y OBJ.
#[derive(Parser)]
#[command(name = "converter", version, about)]
struct Cli {
    /// Archivo de entrada.
    input: PathBuf,

    /// Archivo de salida. Si no se especifica, se usa el mismo nombre con la extensión del formato destino.
    output: Option<PathBuf>,

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

    /// Modo batch: convertir múltiples archivos.
    #[arg(long)]
    batch: bool,

    /// Formato de salida (para modo batch). Valores: glb, usda, usdz, stl, obj.
    #[arg(long)]
    format: Option<String>,
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

    if cli.batch {
        run_batch(&cli, &options);
    } else {
        run_single(&cli, &options);
    }
}

fn run_single(cli: &Cli, options: &ConvertOptions) {
    let output = match &cli.output {
        Some(p) => p.clone(),
        None => {
            eprintln!("Error: se requiere archivo de salida");
            process::exit(1);
        }
    };

    match converter_core::convert(&cli.input, &output, options) {
        Ok(()) => {
            eprintln!(
                "Convertido: {} → {}",
                cli.input.display(),
                output.display()
            );
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}

fn run_batch(cli: &Cli, options: &ConvertOptions) {
    let ext = match &cli.format {
        Some(f) => f.clone(),
        None => {
            // Intentar deducir del output
            match &cli.output {
                Some(p) => p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("usdz")
                    .to_string(),
                None => {
                    eprintln!("Error: modo batch requiere --format o un archivo de salida");
                    process::exit(1);
                }
            }
        }
    };

    // En modo batch, el input puede ser un glob pattern.
    // Por simplicidad, tratamos el input como un solo archivo y usamos el format.
    let input = &cli.input;
    let output = input.with_extension(&ext);

    match converter_core::convert(input, &output, options) {
        Ok(()) => {
            eprintln!("Convertido: {} → {}", input.display(), output.display());
        }
        Err(e) => {
            eprintln!("Error convirtiendo {}: {}", input.display(), e);
            process::exit(1);
        }
    }
}
