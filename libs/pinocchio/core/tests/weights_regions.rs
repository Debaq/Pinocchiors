//! Pesos por región en personajes sintéticos: cada zona del cuerpo debe
//! quedar dominada por los huesos que la mueven. El elefante tiene un tronco
//! grueso: el lomo y la panza son de la columna, no de las patas o el cuello.

#[path = "../../embedding/tests/characters/mod.rs"]
mod characters;

use pinocchio_core::math::Vector3;
use pinocchio_core::skeleton::{HumanSkeleton, QuadSkeleton, Skeleton};
use pinocchio_core::{autorig, PinocchioConfig, PinocchioOutput};

type Region = (&'static str, fn(&Vector3) -> bool, &'static [&'static str]);

/// Fracción de vértices de cada región dominados por un hueso permitido.
fn region_scores<S: Skeleton>(mesh: &pinocchio_core::mesh::Mesh, out: &PinocchioOutput, skeleton: &S, regions: &[Region]) -> Vec<(&'static str, f64)> {
    regions
        .iter()
        .map(|(label, inside, allowed)| {
            let allowed: Vec<usize> = allowed
                .iter()
                .map(|n| skeleton.bones().iter().position(|b| b.name == *n).expect("hueso"))
                .collect();
            let (mut total, mut ok) = (0usize, 0usize);
            let mut wrong: std::collections::HashMap<usize, usize> = Default::default();
            for (v, vertex) in mesh.vertices.iter().enumerate() {
                if !inside(&vertex.position) {
                    continue;
                }
                total += 1;
                let dominant = out.get_dominant_bones(v, 1)[0].0;
                if allowed.contains(&dominant) {
                    ok += 1;
                } else {
                    *wrong.entry(dominant).or_default() += 1;
                }
            }
            assert!(total > 20, "{label}: región vacía");
            let ratio = ok as f64 / total as f64;
            let mut wrong: Vec<_> = wrong.into_iter().collect();
            wrong.sort_by_key(|w| std::cmp::Reverse(w.1));
            let wrong: Vec<String> = wrong.iter().take(3).map(|(b, n)| format!("{}:{n}", skeleton.bones()[*b].name)).collect();
            println!("  {label:>16}: {:5.1} %  (otros: {})", 100.0 * ratio, wrong.join(", "));
            (*label, ratio)
        })
        .collect()
}

#[test]
fn thick_body_belongs_to_the_spine() {
    let character = characters::elephant();
    let mesh = characters::mesh(&character.capsules, 0.03);
    let skeleton = QuadSkeleton::new();
    let config = PinocchioConfig { verify_mesh_integrity: false, ..Default::default() };
    let out = autorig(&mesh, &skeleton, Some(config)).expect("autorig");
    let regions: [Region; 8] = [
        // Entre la cadera (z ≈ -0,48) y el pecho (z ≈ 0,37), con margen
        ("lomo", |p| p.y() > 1.55 && p.z() > -0.3 && p.z() < 0.2, &["spine", "chest"]),
        // La cruz, sobre la articulación del pecho: el cuello arranca ahí
        ("cruz", |p| p.y() > 1.55 && p.z() >= 0.2 && p.z() < 0.5, &["chest", "neck"]),
        ("panza", |p| p.y() < 0.85 && p.y() > 0.7 && p.x().abs() < 0.08 && p.z().abs() < 0.2, &["spine", "chest"]),
        // Axilas: la franja de la panza pegada a las patas puede compartir
        ("panza (axilas)", |p| p.y() < 0.85 && p.y() > 0.7 && p.x().abs() < 0.14 && p.z().abs() < 0.3, &["spine", "chest", "elbow_l", "elbow_r", "knee_l", "knee_r"]),
        ("flanco", |p| p.x().abs() > 0.4 && p.y() > 1.1 && p.z().abs() < 0.2, &["spine", "chest"]),
        ("pata del. izq.", |p| p.x() < -0.1 && p.y() < 0.45 && p.z() > 0.2, &["elbow_l", "paw_fl"]),
        ("pata tras. der.", |p| p.x() > 0.1 && p.y() < 0.45 && p.z() < -0.2, &["knee_r", "paw_br"]),
        ("cabeza", |p| p.z() > 0.85 && p.y() > 1.2, &["neck", "head"]),
    ];
    let scores = region_scores(&mesh, &out, &skeleton, &regions);
    for (label, ratio) in scores {
        assert!(ratio > 0.8, "{label}: solo {:.0} % con el hueso esperado", 100.0 * ratio);
    }
}

#[test]
fn human_regions_keep_their_bones() {
    let character = characters::human();
    let mesh = characters::mesh(&character.capsules, 0.02);
    let skeleton = HumanSkeleton::new();
    let config = PinocchioConfig { verify_mesh_integrity: false, ..Default::default() };
    let out = autorig(&mesh, &skeleton, Some(config)).expect("autorig");
    let regions: [Region; 5] = [
        ("pecho", |p| p.y() > 1.15 && p.y() < 1.3 && p.x().abs() < 0.1 && p.z() > 0.1, &["spine", "chest"]),
        ("brazo izq.", |p| p.x() < -0.28 && p.x() > -0.4 && p.y() > 1.2, &["elbow_l"]),
        ("antebrazo der.", |p| p.x() > 0.52 && p.x() < 0.62, &["wrist_r"]),
        ("muslo izq.", |p| p.x() < -0.03 && p.y() > 0.6 && p.y() < 0.75, &["knee_l"]),
        ("canilla der.", |p| p.x() > 0.03 && p.y() > 0.2 && p.y() < 0.35, &["ankle_r"]),
    ];
    let scores = region_scores(&mesh, &out, &skeleton, &regions);
    for (label, ratio) in scores {
        assert!(ratio > 0.85, "{label}: solo {:.0} % con el hueso esperado", 100.0 * ratio);
    }
}
