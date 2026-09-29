//! Reproduce una grabación del escáner y deja la nube fusionada y la malla.
//!
//! cargo run --release -p orizon3d-core --example replay -- <carpeta> [fx_scale=1.3] [voxel=2]
//!
//! Los ajustes `clave=valor` reemplazan a los de la grabación (`fx_scale`,
//! `depth_scale`, `clip_min_mm`, `clip_max_mm`, `box_mm`) o a los de la malla
//! (`voxel`, `fill`, `smooth`). Escribe `fusionada.ply` y `malla.ply` en la carpeta.

use std::path::PathBuf;

use orizon3d_core::recording::Recording;
use orizon3d_core::{mesh, MeshSettings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or("falta la carpeta de la grabación")?);
    let rec = Recording::open(&dir)?;
    let mut settings = rec.settings;
    let mut mesh_settings = MeshSettings::default();
    for arg in args {
        let (key, value) = arg.split_once('=').ok_or_else(|| format!("se esperaba clave=valor: {arg}"))?;
        let v: f32 = value.parse()?;
        match key {
            "fx_scale" => settings.fx_scale = v,
            "depth_scale" => settings.depth_scale = v,
            "clip_min_mm" => settings.clip_min_mm = v,
            "clip_max_mm" => settings.clip_max_mm = v,
            "box_mm" => settings.box_mm = v,
            "voxel" => mesh_settings.voxel_mm = v,
            "fill" => mesh_settings.fill = v as u32,
            "smooth" => mesh_settings.smooth = v as u32,
            _ => return Err(format!("ajuste desconocido: {key}").into()),
        }
    }
    println!("{} cuadros · {settings:?}", rec.frames.len());

    let (session, reports) = rec.replay(Some(settings))?;
    for (i, r) in reports.iter().enumerate() {
        println!(
            "{i:4}  {:6} pts  {}  rmse {:5.2} mm  corr {:5}",
            r.points,
            if r.fused { "sumado    " } else { "descartado" },
            r.stats.last_rmse,
            r.stats.last_corr
        );
    }
    let s = session.stats;
    println!("sumados {} / descartados {} de {}", s.registered, s.dropped, s.frames);

    let cloud = session.fused_cloud();
    cloud.export_ply(&dir.join("fusionada.ply"))?;
    let m = mesh::reconstruct(&cloud, mesh_settings.voxel_mm, mesh_settings.fill, mesh_settings.smooth);
    m.export_ply(&dir.join("malla.ply"))?;
    println!("fusionada.ply: {} puntos · malla.ply: {} triángulos", cloud.points.len(), m.tris.len());
    Ok(())
}
