//! Simetría izquierda/derecha de un esqueleto, para editarlo en espejo.
//!
//! Los pares se reconocen por el nombre (`hand_l`/`hand_r`, `paw_fl`/`paw_fr`,
//! `Arm.L`/`Arm.R`, `LeftArm`/`RightArm`…) y el plano de simetría se estima
//! con las posiciones de esos pares.

use crate::Skeleton;
use pinocchio_math::{Real, Vector3};

/// Sufijos y prefijos que distinguen un lado del otro.
const SIDES: [(&str, &str); 8] = [
    ("_l", "_r"),
    ("_fl", "_fr"),
    ("_bl", "_br"),
    (".l", ".r"),
    ("_L", "_R"),
    (".L", ".R"),
    ("Left", "Right"),
    ("left", "right"),
];

/// Nombre del lado opuesto, si el nombre indica un lado.
pub fn mirror_name(name: &str) -> Option<String> {
    for (a, b) in SIDES {
        for (from, to) in [(a, b), (b, a)] {
            if let Some(stem) = name.strip_suffix(from) {
                return Some(format!("{stem}{to}"));
            }
            if (from == "Left" || from == "Right" || from == "left" || from == "right")
                && let Some(rest) = name.strip_prefix(from)
            {
                return Some(format!("{to}{rest}"));
            }
        }
    }
    None
}

/// Hueso par de cada hueso (`None` en los del medio).
pub fn mirror_pairs<S: Skeleton + ?Sized>(skeleton: &S) -> Vec<Option<usize>> {
    let names: Vec<&str> = skeleton.bones().iter().map(|b| b.name.as_str()).collect();
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let other = mirror_name(name)?;
            names.iter().position(|n| *n == other).filter(|&j| j != i)
        })
        .collect()
}

/// Plano de simetría `(punto, normal unitaria)` estimado con los pares: la
/// normal es la dirección media de derecha a izquierda y el punto el
/// promedio de los puntos medios. `None` si no hay pares separados.
pub fn symmetry_plane<S: Skeleton + ?Sized>(skeleton: &S) -> Option<(Vector3, Vector3)> {
    let pairs = mirror_pairs(skeleton);
    let bones = skeleton.bones();
    let mut normal = Vector3::zero();
    let mut center = Vector3::zero();
    let mut count = 0.0;
    for (i, pair) in pairs.iter().enumerate() {
        let Some(j) = *pair else { continue };
        if i > j {
            continue;
        }
        let d = bones[i].position - bones[j].position;
        // Orientar todas las diferencias igual antes de sumarlas
        let d = if normal.dot(&d) < 0.0 { d * -1.0 } else { d };
        normal += d;
        center += (bones[i].position + bones[j].position) * 0.5;
        count += 1.0;
    }
    let normal = normal.try_normalize()?;
    Some((center * (1.0 / count as Real), normal))
}

/// Reflejo de `p` en el plano `(punto, normal)`.
pub fn reflect(p: Vector3, plane: (Vector3, Vector3)) -> Vector3 {
    let (point, normal) = plane;
    p - normal * (2.0 * normal.dot(&(p - point)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HumanSkeleton, QuadSkeleton};

    #[test]
    fn names_are_mirrored() {
        assert_eq!(mirror_name("hand_l").as_deref(), Some("hand_r"));
        assert_eq!(mirror_name("paw_fr").as_deref(), Some("paw_fl"));
        assert_eq!(mirror_name("Arm.L").as_deref(), Some("Arm.R"));
        assert_eq!(mirror_name("LeftHand").as_deref(), Some("RightHand"));
        assert_eq!(mirror_name("spine"), None);
    }

    #[test]
    fn presets_have_their_pairs_and_plane() {
        let human = HumanSkeleton::new();
        let pairs = mirror_pairs(&human);
        let hand_l = human.bones().iter().position(|b| b.name == "hand_l").unwrap();
        assert_eq!(pairs[hand_l].map(|j| human.bones()[j].name.as_str()), Some("hand_r"));
        assert_eq!(pairs[0], None, "la pelvis está en el medio");

        let (point, normal) = symmetry_plane(&QuadSkeleton::new()).unwrap();
        assert!(normal.x().abs() > 0.99, "plano YZ: {normal:?}");
        assert!(point.x().abs() < 1e-9);
        let p = Vector3::new(0.3, 1.0, 2.0);
        let r = reflect(p, (point, normal));
        assert!((r.x() + 0.3).abs() < 1e-9 && (r.y() - 1.0).abs() < 1e-9);
    }
}
