//! Grabaciones de cuadros crudos del escáner, para reproducir un escaneo sin el
//! escáner conectado y depurar la alineación y el mallado con datos reales.
//!
//! Una grabación es una carpeta con:
//! - `meta.txt`: `clave=valor` con los intrínsecos base de la profundidad y los
//!   ajustes vigentes al empezar ([`ScanSettings`]);
//! - `depth_00000.y16`, `depth_00001.y16`, …: cada mapa de profundidad tal como
//!   llega del escáner (u16 little-endian, fila a fila, ancho × alto de meta).
//!
//! No se guarda el color: la geometría es lo que interesa depurar.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::camera::{DepthFrame, Extrinsics, Intrinsics};
use crate::pointcloud::CloudParams;
use crate::scan::{ScanSession, ScanStats};
use crate::scanner::{scan_frame_cloud, ScanSettings};

pub(crate) fn write_meta(dir: &Path, params: &CloudParams, settings: &ScanSettings) -> io::Result<()> {
    let i = &params.depth_intr;
    let text = format!(
        "width={}\nheight={}\nfx={}\nfy={}\ncx={}\ncy={}\n\
         clip_min_mm={}\nclip_max_mm={}\nbox_mm={}\nclean_noise={}\nisolate_object={}\n\
         edge_filter={}\ntemporal_frames={}\nfx_scale={}\ndepth_scale={}\n",
        i.width,
        i.height,
        i.fx,
        i.fy,
        i.cx,
        i.cy,
        settings.clip_min_mm,
        settings.clip_max_mm,
        settings.box_mm,
        settings.clean_noise,
        settings.isolate_object,
        settings.edge_filter,
        settings.temporal_frames,
        settings.fx_scale,
        settings.depth_scale,
    );
    fs::write(dir.join("meta.txt"), text)
}

pub(crate) fn write_depth(dir: &Path, index: u32, depth: &DepthFrame) -> io::Result<()> {
    let bytes: Vec<u8> = depth.depth.iter().flat_map(|v| v.to_le_bytes()).collect();
    fs::write(dir.join(format!("depth_{index:05}.y16")), bytes)
}

/// Grabación abierta: parámetros, ajustes y rutas de los cuadros en orden
pub struct Recording {
    pub params: CloudParams,
    pub settings: ScanSettings,
    pub frames: Vec<PathBuf>,
}

impl Recording {
    pub fn open(dir: &Path) -> io::Result<Recording> {
        let meta = fs::read_to_string(dir.join("meta.txt"))?;
        let get = |key: &str| -> io::Result<&str> {
            meta.lines()
                .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
                .map(str::trim)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("meta.txt sin `{key}`")))
        };
        let num = |key: &str| -> io::Result<f32> {
            get(key)?.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("`{key}` no es un número")))
        };
        let flag = |key: &str| -> io::Result<bool> { Ok(get(key)? == "true") };

        let depth_intr = Intrinsics {
            width: num("width")? as i16,
            height: num("height")? as i16,
            fx: num("fx")?,
            fy: num("fy")?,
            cx: num("cx")?,
            cy: num("cy")?,
            ..Default::default()
        };
        let settings = ScanSettings {
            clip_min_mm: num("clip_min_mm")?,
            clip_max_mm: num("clip_max_mm")?,
            box_mm: num("box_mm")?,
            clean_noise: flag("clean_noise")?,
            isolate_object: flag("isolate_object")?,
            edge_filter: flag("edge_filter")?,
            temporal_frames: num("temporal_frames")? as usize,
            fx_scale: num("fx_scale")?,
            depth_scale: num("depth_scale")?,
        };
        let params = CloudParams {
            depth_intr,
            rgb_intr: None,
            extrinsics: Extrinsics::default(),
            depth_scale: settings.depth_scale,
            clip_min_mm: 0.0,
            clip_max_mm: 0.0,
            roi: None,
            edge_filter: settings.edge_filter,
        };
        let mut frames: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "y16"))
            .collect();
        frames.sort();
        Ok(Recording { params, settings, frames })
    }

    pub fn depth(&self, index: usize) -> io::Result<DepthFrame> {
        let bytes = fs::read(&self.frames[index])?;
        let (w, h) = (self.params.depth_intr.width as u32, self.params.depth_intr.height as u32);
        Ok(DepthFrame { width: w, height: h, depth: crate::camera::depth_from_y16(&bytes, w, h), timestamp_ms: 0.0 })
    }

    /// Reproduce el escaneo con `settings` (los de la grabación si `None`),
    /// igual que el escáner: devuelve la sesión y el resultado de cada cuadro
    pub fn replay(&self, settings: Option<ScanSettings>) -> io::Result<(ScanSession, Vec<FrameReport>)> {
        let settings = settings.unwrap_or(self.settings);
        let mut session = ScanSession::new();
        let mut reports = Vec::with_capacity(self.frames.len());
        for i in 0..self.frames.len() {
            let cloud = scan_frame_cloud(&self.depth(i)?, None, &self.params, &settings);
            let points = cloud.points.len();
            let fused = session.integrate_frame(&cloud);
            reports.push(FrameReport { points, fused, stats: session.stats });
        }
        Ok((session, reports))
    }
}

/// Lo que pasó con un cuadro al reproducir
#[derive(Debug, Clone, Copy)]
pub struct FrameReport {
    /// Puntos del cuadro después de recortar y limpiar
    pub points: usize,
    /// Se alineó y se sumó al modelo
    pub fused: bool,
    pub stats: ScanStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::default_depth_intrinsics;

    #[test]
    fn roundtrip_meta_and_frames() {
        let dir = std::env::temp_dir().join(format!("orizon3d_rec_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let params = CloudParams {
            depth_intr: default_depth_intrinsics(4, 2),
            rgb_intr: None,
            extrinsics: Extrinsics::default(),
            depth_scale: 0.1,
            clip_min_mm: 0.0,
            clip_max_mm: 0.0,
            roi: None,
            edge_filter: true,
        };
        let settings = ScanSettings { fx_scale: 1.25, isolate_object: false, ..Default::default() };
        write_meta(&dir, &params, &settings).unwrap();
        let depth = DepthFrame { width: 4, height: 2, depth: vec![0, 1, 2, 3, 4000, 5, 6, 65535], timestamp_ms: 0.0 };
        write_depth(&dir, 0, &depth).unwrap();
        write_depth(&dir, 1, &depth).unwrap();

        let rec = Recording::open(&dir).unwrap();
        assert_eq!(rec.frames.len(), 2);
        assert_eq!(rec.settings.fx_scale, 1.25);
        assert!(!rec.settings.isolate_object);
        assert_eq!(rec.params.depth_intr.fx, params.depth_intr.fx);
        assert_eq!(rec.depth(1).unwrap().depth, depth.depth);
        fs::remove_dir_all(&dir).unwrap();
    }
}
