//! Pinocchio App - Aplicación Tauri para auto-rigging

pub mod animation;
pub mod body_parts;
pub mod bvh;
pub mod imported_rig;
pub mod commands;
pub mod placement;
pub mod project;
pub mod scan_cloud;
pub mod scanner;
pub mod state;
pub mod structure;
pub mod textures;

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
            app.manage(textures::TextureWatch::default());
            app.manage(scanner::ScannerHandle::default());
            app.manage(scan_cloud::CloudEditor::default());
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
            // Editar texturas en otras aplicaciones
            textures::export_texture,
            textures::import_texture,
            textures::edit_texture_externally,
            textures::stop_texture_watch,
            // Esqueletos
            commands::list_skeleton_presets,
            commands::select_skeleton,
            commands::apply_rest_pose,
            commands::set_skeleton_bones,
            commands::write_text_file,
            commands::remove_object,
            commands::remove_scene_node,
            commands::set_scene_node_transform,
            commands::undo_scene_edit,
            project::save_project,
            project::new_project,
            project::open_project,
            project::revert_to_original,
            project::recovery_project_path,
            project::recovery_info,
            project::project_changed,
            project::clear_recovery,
            project::take_snapshot,
            project::swap_snapshot,
            project::drop_snapshot,
            project::clear_snapshots,
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
            commands::unwrap_original_mesh,
            commands::undo_unwrap_original,
            commands::restore_transferred_uvs,
            commands::get_checker_texture,
            commands::get_skin_materials,
            commands::get_skin_texture,
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
            // Escáneres 3D (Orizon3D)
            scanner::scanner_connect,
            scanner::scanner_disconnect,
            scanner::scanner_status,
            scanner::scanner_set_settings,
            scanner::scanner_set_gain,
            scanner::scanner_preview,
            scanner::scanner_scan,
            scanner::scanner_measure,
            scanner::scanner_record,
            scanner::scanner_create_model,
            scan_cloud::scan_cloud_take,
            scan_cloud::scan_cloud_import,
            scan_cloud::scan_cloud_export,
            scan_cloud::scan_cloud_info,
            scan_cloud::scan_cloud_data,
            scan_cloud::scan_cloud_edit,
            scan_cloud::scan_cloud_history,
            scan_cloud::scan_cloud_discard,
            scan_cloud::scan_cloud_create_model,
            scan_cloud::scan_cloud_merge,
            scan_cloud::scan_cloud_reference,
            scan_cloud::scan_cloud_align_reference,
            scan_cloud::scan_cloud_compare,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
