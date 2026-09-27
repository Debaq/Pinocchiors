//! Pinocchio App - Aplicación Tauri para auto-rigging

pub mod commands;
pub mod state;

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
            // Esqueletos
            commands::list_skeleton_presets,
            commands::select_skeleton,
            commands::get_skeleton_data,
            // Transformación de esqueleto
            commands::transform_skeleton,
            commands::move_bone,
            commands::auto_fit_skeleton,
            // Autorig
            commands::run_autorig,
            commands::get_weights_data,
            // Retopología
            commands::run_retopology,
            commands::get_quad_mesh_data,
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
