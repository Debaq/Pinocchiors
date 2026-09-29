//! Exportación BVH: esqueleto y una animación, sin malla.
//!
//! BVH es el formato clásico de captura de movimiento (Blender, MotionBuilder,
//! three.js…): una jerarquía de articulaciones con su desplazamiento respecto
//! del padre y, por cuadro, el giro de cada una. Calza con el modelo de la
//! línea de tiempo: el giro de la articulación `J` gira lo que cuelga de ella,
//! como un `JOINT` de BVH. La raíz además se desplaza.
//!
//! Las hojas son `End Site` cuando son la única hija; si comparten padre con
//! otras ramas van como `JOINT` (sin giro) con una punta corta, porque BVH no
//! admite mezclar `End Site` con otras hijas.

use crate::animation::{sample_rotation, sample_translation, AnimationClip, JointTrack};
use converter_scene::glam::{EulerRot, Vec3};
use std::fmt::Write;

/// Articulación: nombre, padre y posición de reposo (en el espacio del modelo)
pub struct BvhJoint<'a> {
    pub name: &'a str,
    pub parent: Option<usize>,
    pub position: Vec3,
}

/// Texto BVH del esqueleto con los cuadros `clip.frames()` de la animación
pub fn write_bvh(joints: &[BvhJoint], clip: &AnimationClip) -> Result<String, String> {
    let n = joints.len();
    let children: Vec<Vec<usize>> =
        (0..n).map(|j| (0..n).filter(|&c| joints[c].parent == Some(j)).collect()).collect();
    // Una sola raíz: la que tiene más descendientes
    let size = |r: usize| {
        let mut stack = vec![r];
        let mut count = 0;
        while let Some(j) = stack.pop() {
            count += 1;
            stack.extend(&children[j]);
        }
        count
    };
    let root = (0..n)
        .filter(|&j| joints[j].parent.is_none())
        .max_by_key(|&r| size(r))
        .ok_or("El esqueleto no tiene raíz")?;

    // Nombres únicos y sin espacios (BVH separa por espacios)
    let mut used = std::collections::HashSet::new();
    let names: Vec<String> = joints
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let base: String = j.name.chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect();
            let base = if base.is_empty() { format!("joint{i}") } else { base };
            let mut name = base.clone();
            let mut k = 2;
            while !used.insert(name.clone()) {
                name = format!("{base}_{k}");
                k += 1;
            }
            name
        })
        .collect();

    let mut out = String::from("HIERARCHY\n");
    // Orden de los canales en MOTION: una entrada por JOINT escrito
    let mut order = Vec::new();
    write_joint(&mut out, &mut order, joints, &children, &names, root, 0);

    let tracks: Vec<Option<&JointTrack>> = (0..n).map(|j| clip.tracks.iter().find(|t| t.joint == j)).collect();
    let fps = if clip.fps > 0.0 { clip.fps } else { 24.0 };
    let frames = clip.frames();
    let _ = writeln!(out, "MOTION\nFrames: {}\nFrame Time: {:.6}", frames.clone().count(), 1.0 / fps);
    for frame in frames {
        let f = frame as f32;
        let mut line = String::new();
        for &j in &order {
            if j == root {
                let p = joints[root].position + sample_translation(tracks[root], f);
                let _ = write!(line, "{} {} {} ", num(p.x), num(p.y), num(p.z));
            }
            // Canales Z X Y: R = Rz · Rx · Ry
            let (z, x, y) = sample_rotation(tracks[j], f).to_euler(EulerRot::ZXY);
            let _ = write!(line, "{} {} {} ", num(z.to_degrees()), num(x.to_degrees()), num(y.to_degrees()));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Ok(out)
}

fn num(x: f32) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn write_joint(
    out: &mut String,
    order: &mut Vec<usize>,
    joints: &[BvhJoint],
    children: &[Vec<usize>],
    names: &[String],
    j: usize,
    depth: usize,
) {
    let pad = "  ".repeat(depth);
    let is_root = depth == 0;
    // La raíz va en el origen: sus canales de posición son absolutos
    let offset = match joints[j].parent {
        Some(p) if !is_root => joints[j].position - joints[p].position,
        _ => Vec3::ZERO,
    };
    let _ = writeln!(out, "{pad}{} {}", if is_root { "ROOT" } else { "JOINT" }, names[j]);
    let _ = writeln!(out, "{pad}{{");
    let _ = writeln!(out, "{pad}  OFFSET {} {} {}", num(offset.x), num(offset.y), num(offset.z));
    if is_root {
        let _ = writeln!(out, "{pad}  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation");
    } else {
        let _ = writeln!(out, "{pad}  CHANNELS 3 Zrotation Xrotation Yrotation");
    }
    order.push(j);

    let kids = &children[j];
    match kids.as_slice() {
        [] => {
            // Hoja con hermanos (o raíz suelta): punta corta hacia afuera
            let dir = joints[j].parent.map_or(Vec3::Y, |p| (joints[j].position - joints[p].position).normalize_or_zero());
            let tip = if dir == Vec3::ZERO { Vec3::Y * 0.01 } else { dir * 0.01 };
            write_end_site(out, &pad, tip);
        }
        [only] if children[*only].is_empty() => {
            write_end_site(out, &pad, joints[*only].position - joints[j].position);
        }
        _ => {
            for &c in kids {
                write_joint(out, order, joints, children, names, c, depth + 1);
            }
        }
    }
    let _ = writeln!(out, "{pad}}}");
}

fn write_end_site(out: &mut String, pad: &str, offset: Vec3) {
    let _ = writeln!(out, "{pad}  End Site\n{pad}  {{\n{pad}    OFFSET {} {} {}\n{pad}  }}", num(offset.x), num(offset.y), num(offset.z));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Key, KeyInterpolation};
    use converter_scene::glam::Quat;

    /// cadera → columna → cabeza; cadera → pierna → pie
    fn joints() -> Vec<BvhJoint<'static>> {
        vec![
            BvhJoint { name: "hip", parent: None, position: Vec3::new(0.0, 1.0, 0.0) },
            BvhJoint { name: "spine", parent: Some(0), position: Vec3::new(0.0, 1.5, 0.0) },
            BvhJoint { name: "head", parent: Some(1), position: Vec3::new(0.0, 1.8, 0.0) },
            BvhJoint { name: "leg", parent: Some(0), position: Vec3::new(0.2, 0.5, 0.0) },
            BvhJoint { name: "foot", parent: Some(3), position: Vec3::new(0.2, 0.0, 0.0) },
        ]
    }

    fn key<T>(frame: f32, value: T) -> Key<T> {
        Key { frame, value, interpolation: KeyInterpolation::Linear }
    }

    fn channels(bvh: &str, frame: usize) -> Vec<f32> {
        let motion = bvh.split("Frame Time:").nth(1).unwrap();
        motion.lines().nth(1 + frame).unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect()
    }

    #[test]
    fn hierarchy_uses_end_sites_for_single_leaves() {
        let clip = AnimationClip { name: "a".into(), fps: 30.0, tracks: vec![], start: Some(0.0), end: Some(9.0) };
        let bvh = write_bvh(&joints(), &clip).unwrap();
        assert!(bvh.starts_with("HIERARCHY\nROOT hip"));
        assert_eq!(bvh.matches("JOINT").count(), 2, "spine y leg; head y foot son End Site");
        assert_eq!(bvh.matches("End Site").count(), 2);
        assert!(bvh.contains("OFFSET 0.2 -0.5 0"), "leg respecto de hip");
        assert!(bvh.contains("Frames: 10\nFrame Time: 0.033333"));
        // Reposo: la raíz en su lugar, sin giros (6 + 3 + 3 canales)
        assert_eq!(channels(&bvh, 0), vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn motion_samples_rotations_and_root_translation() {
        let bend = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2).to_array();
        let clip = AnimationClip {
            name: "a".into(),
            fps: 24.0,
            tracks: vec![
                JointTrack { joint: 3, rotation: vec![key(0.0, [0.0, 0.0, 0.0, 1.0]), key(10.0, bend)], translation: vec![] },
                JointTrack { joint: 0, rotation: vec![], translation: vec![key(0.0, [0.0, 0.0, 0.0]), key(10.0, [0.0, 0.0, 2.0])] },
            ],
            start: None,
            end: None,
        };
        let bvh = write_bvh(&joints(), &clip).unwrap();
        if let Ok(dir) = std::env::var("PINOCCHIO_EXPORT_DIR") {
            std::fs::write(format!("{dir}/prueba.bvh"), &bvh).unwrap();
        }
        assert!(bvh.contains("Frames: 11"));
        // Cuadro 5: la raíz a mitad de camino y la pierna a 45° en X
        let c = channels(&bvh, 5);
        assert_eq!(&c[0..3], &[0.0, 1.0, 1.0]);
        let leg = &c[9..12];
        assert!((leg[1] - 45.0).abs() < 1e-3 && leg[0].abs() < 1e-3 && leg[2].abs() < 1e-3, "{leg:?}");
    }

    #[test]
    fn leaves_next_to_branches_become_joints() {
        let mut j = joints();
        // Una segunda hoja colgando de la cadera, junto a spine y leg
        j.push(BvhJoint { name: "tail tip", parent: Some(0), position: Vec3::new(0.0, 0.9, -0.3) });
        let clip = AnimationClip { name: "a".into(), fps: 24.0, tracks: vec![], start: None, end: None };
        let bvh = write_bvh(&j, &clip).unwrap();
        assert!(bvh.contains("JOINT tail_tip"), "sin espacios en el nombre");
        assert!(bvh.contains("Frames: 1"));
        assert_eq!(channels(&bvh, 0).len(), 6 + 3 * 3);
    }
}
