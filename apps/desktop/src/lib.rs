//! Pinocchio App - Aplicación Tauri para auto-rigging

pub mod animation;
pub mod body_parts;
pub mod cad;
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
            #[cfg(target_os = "linux")]
            enable_camera(app);
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
            commands::list_template_shapes,
            commands::new_body_plan,
            commands::select_body_plan,
            commands::get_skeleton_data,
            // Transformación de esqueleto
            commands::transform_skeleton,
            commands::move_bone,
            commands::set_bone_positions,
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
            scan_cloud::scan_cloud_add,
            scan_cloud::scan_cloud_take_set,
            scan_cloud::scan_cloud_take_align,
            scan_cloud::scan_cloud_fuse,
            scan_cloud::scan_cloud_reference,
            scan_cloud::scan_cloud_align_reference,
            scan_cloud::scan_cloud_compare,
            // CAD paramétrico y escaneo → CAD
            cad::cad_status,
            cad::cad_new,
            cad::cad_close,
            cad::cad_get_document,
            cad::cad_set_document,
            cad::cad_preview,
            cad::cad_evaluate,
            cad::cad_solve_sketch,
            cad::cad_eval_expr,
            cad::cad_mesh,
            cad::cad_tool_mesh,
            cad::cad_face_ref,
            cad::cad_edge_ref,
            cad::cad_face_info,
            cad::cad_resolve_refs,
            cad::cad_measure,
            cad::cad_parts_at,
            cad::cad_drawing,
            cad::cad_assembly_mesh,
            cad::cad_assembly_faces,
            cad::cad_assembly_connector,
            cad::cad_assembly_interference,
            cad::cad_deviation,
            cad::cad_write_text,
            cad::cad_write_pdf,
            cad::cad_export,
            cad::cad_import_step,
            cad::cad_to_model,
            cad::cad_scan_pick,
            cad::cad_scan_detect,
            cad::cad_scan_slice,
            cad::cad_scan_add,
            cad::cad_mm_per_unit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// WebKitGTK trae la cámara apagada y niega los permisos que nadie atiende:
/// se enciende y se acepta el pedido de la cámara (captura de movimiento)
#[cfg(target_os = "linux")]
fn enable_camera(app: &tauri::App) {
    use webkit2gtk::glib::prelude::*;
    use webkit2gtk::{PermissionRequestExt, SettingsExt, UserMediaPermissionRequest, WebViewExt};
    for window in app.webview_windows().values() {
        let _ = window.with_webview(|webview| {
            let view = webview.inner();
            if let Some(settings) = WebViewExt::settings(&view) {
                settings.set_enable_media_stream(true);
            }
            view.connect_permission_request(|_, request| {
                if request.is::<UserMediaPermissionRequest>() {
                    request.allow();
                    true
                } else {
                    false
                }
            });
        });
    }
}
