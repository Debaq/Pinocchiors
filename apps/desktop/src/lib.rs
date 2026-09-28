//! Pinocchio App - Aplicación Tauri para auto-rigging

pub mod animation;
pub mod commands;
pub mod placement;
pub mod project;
pub mod state;
pub mod structure;

pub use commands::*;
pub use state::{AppState, SkeletonType};

use tauri::Manager;

/// Ejecuta la aplicación Tauri
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Inicializar estado
            app.manage(AppState::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Import/Export multi-formato
            commands::get_supported_formats,
            commands::import_model,
            commands::export_model,
            commands::get_mesh_data,
            // Orientación: piso, frente, origen
            placement::get_placement_info,
            placement::apply_placement,
            // Estructura del archivo de origen
            structure::get_scene_structure,
            structure::get_scene_materials,
            structure::get_scene_texture,
            // Esqueletos
            commands::list_skeleton_presets,
            commands::select_skeleton,
            commands::remove_object,
            project::save_project,
            project::open_project,
            project::revert_to_original,
            project::recovery_project_path,
            project::recovery_info,
            commands::get_body_plan,
            commands::select_body_plan,
            commands::get_skeleton_data,
            // Transformación de esqueleto
            commands::transform_skeleton,
            commands::move_bone,
            commands::auto_fit_skeleton,
            commands::center_bones,
            // Autorig
            commands::run_autorig,
            commands::get_weights_data,
            commands::set_vertex_weights,
            commands::get_weight_mirror,
            // Retopología
            commands::run_retopology,
            commands::get_quad_mesh_data,
            commands::set_active_mesh,
            commands::get_active_mesh,
            // UV / Piel
            commands::get_uv_info,
            commands::run_uv_unwrap,
            commands::restore_transferred_uvs,
            commands::get_uv_texture,
            commands::get_uv_layout,
            // Reparación
            commands::analyze_mesh,
            commands::repair_mesh,
            commands::undo_repair,
            commands::get_repair_diagnostics,
            // Impresión 3D
            commands::analyze_print3d,
            commands::scale_mesh_for_print,
            commands::undo_print_scale,
            commands::subdivide_mesh,
            commands::export_print3d_piece,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
