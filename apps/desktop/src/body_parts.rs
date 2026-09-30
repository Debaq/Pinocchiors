//! Partes del cuerpo según el rig, para cortar el mapa UV para pintar: cada
//! cara va a su hueso dominante y los huesos se agrupan en cadenas.
//!
//! Una cadena empieza en un hijo de la raíz o de una bifurcación y sigue
//! mientras cada articulación tenga un solo hijo. Las que terminan en una
//! punta (cabeza, cada pata, la cola) son una parte cada una; las que unen
//! dos bifurcaciones (columna, pecho) forman juntas el torso.

use uv_core::SkinParts;

/// Parte de cada hueso y el nombre de cada parte. `parents[b]` es el padre
/// del hueso `b` (el peso del hueso `b` es el del segmento padre → `b`).
pub fn bone_parts(names: &[String], parents: &[Option<usize>]) -> (Vec<Option<usize>>, Vec<String>) {
    let n = parents.len();
    let mut children = vec![Vec::new(); n];
    for (b, parent) in parents.iter().enumerate() {
        if let Some(p) = parent.filter(|&p| p < n) {
            children[p].push(b);
        }
    }
    // Cadenas: desde cada hijo de una raíz o bifurcación
    let starts_chain = |b: usize| parents[b].is_some_and(|p| p < n && (parents[p].is_none() || children[p].len() != 1));
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for b in (0..n).filter(|&b| starts_chain(b)) {
        let mut chain = vec![b];
        let mut last = b;
        while children[last].len() == 1 {
            last = children[last][0];
            chain.push(last);
        }
        chains.push(chain);
    }

    let mut part_of = vec![None; n];
    let mut part_names: Vec<String> = Vec::new();
    let mut torso = None;
    for chain in &chains {
        let tip = children[*chain.last().expect("cadena no vacía")].is_empty();
        let part = if tip {
            part_names.push(chain_name(chain.iter().map(|&b| names[b].as_str()), names));
            part_names.len() - 1
        } else {
            *torso.get_or_insert_with(|| {
                part_names.push("Torso".to_string());
                part_names.len() - 1
            })
        };
        for &b in chain {
            part_of[b] = Some(part);
        }
    }
    // Nombres repetidos (patas sin lado): numerarlos
    let mut count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in &part_names {
        *count.entry(name.clone()).or_default() += 1;
    }
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let part_names = part_names
        .into_iter()
        .map(|name| {
            if count[&name] > 1 {
                let k = seen.entry(name.clone()).or_default();
                *k += 1;
                format!("{name} {k}")
            } else {
                name
            }
        })
        .collect();
    (part_of, part_names)
}

/// Nombre de una parte por los huesos de su cadena (nombres en inglés de las
/// plantillas y de Blender o Mixamo).
fn chain_name<'a>(bones: impl Iterator<Item = &'a str>, all: &[String]) -> String {
    let bones: Vec<String> = bones.map(|b| b.to_ascii_lowercase()).collect();
    let has = |words: &[&str]| bones.iter().any(|b| words.iter().any(|w| b.contains(w)));
    // Cuadrúpedo: el esqueleto tiene patas ("paw") o pezuñas
    let quadruped = all.iter().any(|b| {
        let b = b.to_ascii_lowercase();
        b.contains("paw") || b.contains("hoof")
    });
    let side = {
        let ends = |suffixes: &[&str]| bones.iter().any(|b| suffixes.iter().any(|s| b.ends_with(s)) || b.contains(suffixes[0]));
        if ends(&["left", "_l", ".l", "_fl", "_bl"]) || bones.iter().any(|b| b.starts_with("l_") || b.starts_with("left")) {
            Some(true)
        } else if ends(&["right", "_r", ".r", "_fr", "_br"]) || bones.iter().any(|b| b.starts_with("r_") || b.starts_with("right")) {
            Some(false)
        } else {
            None
        }
    };
    let front = has(&["_fl", "_fr", "front", "fore"]);
    let back = has(&["_bl", "_br", "back", "hind", "rear"]);
    let (noun, feminine) = if has(&["head", "neck", "skull"]) {
        ("Cabeza", true)
    } else if has(&["tail"]) {
        ("Cola", true)
    } else if has(&["wing"]) {
        ("Ala", true)
    } else if has(&["shoulder", "arm", "elbow", "wrist", "hand", "clavicle"]) || front {
        if quadruped { ("Pata delantera", true) } else { ("Brazo", false) }
    } else if has(&["hip", "thigh", "knee", "ankle", "foot", "leg", "shin", "calf"]) || back {
        if quadruped { ("Pata trasera", true) } else { ("Pierna", true) }
    } else {
        return capitalized(bones.first().map_or("Parte", |b| b.as_str()));
    };
    match (side, feminine) {
        (Some(true), true) => format!("{noun} izquierda"),
        (Some(true), false) => format!("{noun} izquierdo"),
        (Some(false), true) => format!("{noun} derecha"),
        (Some(false), false) => format!("{noun} derecho"),
        (None, _) => noun.to_string(),
    }
}

fn capitalized(name: &str) -> String {
    let words = name.replace(['_', '.'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => "Parte".to_string(),
    }
}

/// Parte de cada cara: la del hueso con más peso sumado en sus vértices.
/// Las partes sin caras se quitan. `None` si ninguna cara tiene peso.
pub fn face_parts(
    faces: &[Vec<usize>],
    weights: &[Vec<f64>],
    bone_part: &[Option<usize>],
    part_names: &[String],
) -> Option<SkinParts> {
    let mut face_part: Vec<Option<usize>> = faces
        .iter()
        .map(|face| {
            let mut sum = vec![0.0; bone_part.len()];
            for &v in face {
                for (b, w) in weights.get(v).map_or(&[][..], |w| &w[..]).iter().enumerate().take(sum.len()) {
                    sum[b] += w;
                }
            }
            (0..sum.len()).filter(|&b| bone_part[b].is_some() && sum[b] > 0.0).max_by(|&a, &b| sum[a].total_cmp(&sum[b])).and_then(|b| bone_part[b])
        })
        .collect();
    // Caras sin peso: a la parte más grande
    let mut sizes = vec![0usize; part_names.len()];
    for p in face_part.iter().flatten() {
        sizes[*p] += 1;
    }
    let largest = (0..sizes.len()).max_by_key(|&p| sizes[p])?;
    if sizes[largest] == 0 {
        return None;
    }
    for p in &mut face_part {
        p.get_or_insert(largest);
    }
    // Renumerar sin las partes vacías
    let mut new_index = vec![None; part_names.len()];
    let mut names = Vec::new();
    for (p, &size) in sizes.iter().enumerate() {
        if size > 0 {
            new_index[p] = Some(names.len());
            names.push(part_names[p].clone());
        }
    }
    Some(SkinParts {
        names,
        face_part: face_part.into_iter().map(|p| new_index[p.expect("todas asignadas")].expect("parte con caras")).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_of(bones: &[(&str, Option<usize>)]) -> (Vec<Option<usize>>, Vec<String>) {
        let names: Vec<String> = bones.iter().map(|b| b.0.to_string()).collect();
        let parents: Vec<Option<usize>> = bones.iter().map(|b| b.1).collect();
        bone_parts(&names, &parents)
    }

    #[test]
    fn human_template_parts() {
        let (part, names) = parts_of(&[
            ("pelvis", None),
            ("spine", Some(0)),
            ("chest", Some(1)),
            ("neck", Some(2)),
            ("head", Some(3)),
            ("shoulder_l", Some(2)),
            ("elbow_l", Some(5)),
            ("hand_l", Some(6)),
            ("shoulder_r", Some(2)),
            ("elbow_r", Some(8)),
            ("hand_r", Some(9)),
            ("hip_l", Some(0)),
            ("knee_l", Some(11)),
            ("hip_r", Some(0)),
            ("knee_r", Some(13)),
        ]);
        let name = |b: usize| names[part[b].unwrap()].as_str();
        assert_eq!(part[0], None, "la raíz no tiene segmento");
        assert_eq!(name(1), "Torso");
        assert_eq!(name(2), "Torso");
        assert_eq!(name(4), "Cabeza");
        assert_eq!(name(6), "Brazo izquierdo");
        assert_eq!(name(9), "Brazo derecho");
        assert_eq!(name(12), "Pierna izquierda");
        assert_eq!(name(14), "Pierna derecha");
        assert_eq!(names.len(), 6);
    }

    #[test]
    fn quadruped_template_parts() {
        let (part, names) = parts_of(&[
            ("hip", None),
            ("spine", Some(0)),
            ("chest", Some(1)),
            ("neck", Some(2)),
            ("head", Some(3)),
            ("tail_base", Some(0)),
            ("tail_end", Some(5)),
            ("shoulder_l", Some(2)),
            ("paw_fl", Some(7)),
            ("hip_r", Some(0)),
            ("paw_br", Some(9)),
        ]);
        let name = |b: usize| names[part[b].unwrap()].as_str();
        assert_eq!(name(6), "Cola");
        assert_eq!(name(8), "Pata delantera izquierda");
        assert_eq!(name(10), "Pata trasera derecha");
        assert_eq!(name(4), "Cabeza");
    }

    #[test]
    fn faces_follow_their_dominant_bone() {
        // Dos huesos en cadena desde la raíz: el 1 torso-punta, el 2 punta
        let (bone_part, names) = parts_of(&[("root", None), ("a", Some(0)), ("tail", Some(0))]);
        let weights = vec![vec![0.0, 1.0, 0.0], vec![0.0, 0.2, 0.8], vec![0.0, 0.0, 1.0], vec![0.0; 3]];
        let parts = face_parts(&[vec![0, 1], vec![1, 2], vec![3]], &weights, &bone_part, &names).unwrap();
        assert_eq!(parts.names[parts.face_part[1]], "Cola");
        assert_eq!(parts.names[parts.face_part[0]], "A");
        // Cara sin peso: va a la parte más grande
        assert_eq!(parts.face_part.len(), 3);
    }
}
