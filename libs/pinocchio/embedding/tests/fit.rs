mod characters;

use characters::Character;
use pinocchio_embedding::{fit_skeleton, FitOptions};
use pinocchio_skeleton::{BodyPlan, HumanSkeleton, QuadSkeleton, Skeleton};

/// Error de cada articulación con posición real conocida, en fracción del
/// tamaño del personaje.
fn fit_errors<S: Skeleton>(character: Character, template: &S) -> Vec<(&'static str, f64)> {
    let mesh = characters::mesh(&character.capsules, 0.025);
    let size = mesh.bounding_box().longest_axis_length();
    let fit = fit_skeleton(&mesh, template, &FitOptions::default()).expect("ajuste");
    println!("{} extremidades, calidad {:.2}", fit.extremities.len(), fit.quality);
    println!("  giro {:?}", fit.rotation);
    for (b, e) in fit.bone_extremity.iter().enumerate() {
        if let Some(e) = e {
            println!("    {} → extremidad {e}", template.bones()[b].name);
        }
    }
    for e in &fit.extremities {
        println!("    punta ({:+.2}, {:+.2}, {:+.2}) sobresale {:.2}", e.tip.x(), e.tip.y(), e.tip.z(), e.length);
    }
    character
        .joints
        .iter()
        .map(|&(name, truth)| {
            let b = template.bones().iter().position(|b| b.name == name).expect("hueso del preset");
            let p = fit.skeleton.bones()[b].position;
            let e = ((p.x() - truth[0]).powi(2) + (p.y() - truth[1]).powi(2) + (p.z() - truth[2]).powi(2)).sqrt() / size;
            println!("  {name:>10}: {e:.3}  ({:+.2}, {:+.2}, {:+.2})", p.x(), p.y(), p.z());
            (name, e)
        })
        .collect()
}

fn assert_errors(errors: &[(&str, f64)], tolerance: f64) {
    let bad: Vec<_> = errors.iter().filter(|(_, e)| *e > tolerance).collect();
    assert!(bad.is_empty(), "articulaciones lejos de su lugar: {bad:?}");
}

#[test]
fn human_fits() {
    assert_errors(&fit_errors(characters::human(), &HumanSkeleton::new()), 0.06);
}

#[test]
fn elephant_fits() {
    assert_errors(&fit_errors(characters::elephant(), &QuadSkeleton::new()), 0.06);
}

#[test]
fn turned_elephant_fits() {
    assert_errors(&fit_errors(characters::elephant().turned(), &QuadSkeleton::new()), 0.06);
}

#[test]
fn z_up_human_fits() {
    assert_errors(&fit_errors(characters::human().z_up(), &HumanSkeleton::new()), 0.06);
}

#[test]
fn elephant_template_puts_the_trunk_in_the_trunk() {
    let mut character = characters::elephant();
    // Con la plantilla de elefante la cabeza no es hoja: se mide la trompa
    character.joints.retain(|(name, _)| *name != "head");
    character.joints.push(("trunk_tip", [0.0, 0.35, 1.35]));
    character.joints.push(("tail_tip", [0.0, 0.75, -1.15]));
    character.joints.retain(|(name, _)| *name != "tail_end");
    assert_errors(&fit_errors(character, &BodyPlan::variant("elephant").unwrap().build()), 0.06);
}

#[test]
fn octopus_arms_find_their_tentacles() {
    assert_errors(&fit_errors(characters::octopus(), &BodyPlan::variant("octopus").unwrap().build()), 0.06);
}

#[test]
fn fish_fits() {
    assert_errors(&fit_errors(characters::fish(), &BodyPlan::variant("fish").unwrap().build()), 0.08);
}
