//! Prueba de carga de modelo GLB

use pinocchio_core::mesh::load_glb;
use pinocchio_core::skeleton::{HumanSkeleton, Skeleton};
use pinocchio_core::{autorig, PinocchioConfig};

fn main() {
    // Ruta desde la raíz del workspace (donde se ejecuta cargo run)
    let path = "modelo.glb";

    println!("=== Prueba de Pinocchio-RS ===\n");

    // 1. Cargar el modelo GLB
    println!("Cargando modelo: {}", path);
    let mesh = match load_glb(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error cargando GLB: {}", e);
            return;
        }
    };

    println!("Modelo cargado:");
    println!("  - Vértices: {}", mesh.num_vertices());
    println!("  - Caras: {}", mesh.num_faces());
    println!("  - Aristas: {}", mesh.num_edges());

    let bbox = mesh.bounding_box();
    println!("  - Bounding box:");
    println!("    Min: ({:.3}, {:.3}, {:.3})", bbox.min.x(), bbox.min.y(), bbox.min.z());
    println!("    Max: ({:.3}, {:.3}, {:.3})", bbox.max.x(), bbox.max.y(), bbox.max.z());
    println!("    Tamaño: ({:.3}, {:.3}, {:.3})", bbox.size().x(), bbox.size().y(), bbox.size().z());

    // 2. Crear esqueleto humano
    println!("\nCreando esqueleto humanoide...");
    let skeleton = HumanSkeleton::new();
    println!("  - Huesos: {}", skeleton.bones().len());

    // 3. Intentar auto-rigging (con config rápida para prueba)
    println!("\nIntentando auto-rigging (config rápida)...");
    let config = PinocchioConfig::fast();

    match autorig(&mesh, &skeleton, Some(config)) {
        Ok(output) => {
            println!("\n¡Auto-rigging completado!");
            println!("  - Vértices procesados: {}", output.stats.num_vertices);
            println!("  - Huesos: {}", output.stats.num_bones);
            println!("  - Esferas mediales: {}", output.stats.num_medial_spheres);
            println!("  - Calidad embedding: {:.2}%", output.stats.embedding_quality * 100.0);
            println!("  - Influencias promedio/vértice: {:.2}", output.stats.avg_influences_per_vertex);

            // Mostrar algunos pesos de ejemplo
            println!("\nPesos de los primeros 5 vértices:");
            for i in 0..5.min(output.stats.num_vertices) {
                let dominant = output.get_dominant_bones(i, 3);
                print!("  Vértice {}: ", i);
                for (bone_idx, weight) in &dominant {
                    print!("hueso{}={:.2} ", bone_idx, weight);
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("Error en auto-rigging: {}", e);
        }
    }

    println!("\n=== Prueba completada ===");
}
