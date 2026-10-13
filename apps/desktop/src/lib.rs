//! Pinocchio App - Aplicación Tauri para auto-rigging

pub mod animation;
pub mod body_parts;
pub mod cad;
pub mod bvh;
pub mod hdri;
pub mod imported_rig;
pub mod mesh_slot;
#[cfg(target_os = "linux")]
pub mod linux_desktop;
pub mod commands;
pub mod placement;
pub mod print_prep;
pub mod project;
pub mod ray_render;
pub mod remesh;
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
    #[cfg(target_os = "linux")]
    {
        linux_desktop::set_program_name();
        linux_desktop::ensure_desktop_entry();
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Inicializar estado
            app.manage(AppState::new());
            app.manage(textures::TextureWatch::default());
            app.manage(scanner::ScannerHandle::default());
            app.manage(ray_render::RayRender::default());
            app.manage(scan_cloud::CloudEditor::default());
            #[cfg(target_os = "linux")]
            enable_camera(app);
            #[cfg(target_os = "linux")]
            lock_page_zoom(app);
            // Si la interfaz nunca avisa (error al cargar), igual se muestra
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(20));
                show_main_window(&handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_ready,
            // Import/Export multi-formato
            commands::get_supported_formats,
            commands::import_model,
            commands::export_model,
            commands::write_render_image,
            ray_render::ray_render_start,
            ray_render::ray_render_animation_start,
            ray_render::ray_render_status,
            ray_render::ray_render_image,
            ray_render::ray_render_cancel,
            commands::get_mesh_data,
            commands::get_mesh_preview,
            commands::set_mesh_build,
            commands::mesh_edit_ready,
            // Orientación: piso, frente, origen
            placement::get_placement_info,
            placement::apply_placement,
            // Estructura del archivo de origen
            structure::get_scene_structure,
            structure::get_scene_materials,
            structure::set_scene_material,
            structure::set_skin_material,
            structure::add_scene_material,
            structure::remove_scene_material,
            structure::assign_scene_material,
            structure::assign_scene_material_faces,
            structure::get_scene_texture,
            // Editar texturas en otras aplicaciones
            textures::export_texture,
            textures::import_texture,
            textures::clear_texture,
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
            project::object_activate,
            project::object_remove,
            project::object_adopt,
            project::object_mesh_data,
            project::objects_info,
            project::export_objects,
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
            remesh::remesh_preview,
            remesh::get_remesh_preview_data,
            remesh::remesh_discard,
            remesh::remesh_apply,
            remesh::remesh_info,
            remesh::remesh_voxel_grid,
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
            print_prep::get_print_presets,
            print_prep::get_fab_tolerances,
            print_prep::export_calibration_model,
            print_prep::solve_print_calibration,
            print_prep::check_print_readiness,
            print_prep::suggest_print_orientation,
            print_prep::add_print_joints,
            print_prep::estimate_print_material,
            print_prep::export_print_plates,
            print_prep::preview_print_plates,
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
            cad::cad_sketch_suggest,
            cad::cad_gear_info,
            cad::cad_gear_design,
            cad::cad_gear_block,
            cad::cad_sketch_define,
            cad::cad_vertex_spec,
            cad::cad_eval_expr,
            cad::cad_mesh,
            cad::cad_tool_mesh,
            cad::cad_face_ref,
            cad::cad_edge_ref,
            cad::cad_face_info,
            cad::cad_face_edges,
            cad::cad_feature_faces,
            cad::cad_resolve_refs,
            cad::cad_sketch_x,
            cad::cad_measure,
            cad::cad_parts_at,
            cad::cad_drawing,
            cad::cad_assembly_mesh,
            cad::cad_assembly_faces,
            cad::cad_assembly_connector,
            cad::cad_assembly_interference,
            cad::cad_assembly_clearance,
            cad::cad_assembly_suggest,
            cad::cad_deviation,
            cad::cad_write_text,
            cad::cad_write_bytes,
            cad::cad_write_pdf,
            cad::cad_export,
            cad::cad_project_edge,
            cad::cad_project_face,
            cad::cad_project_model,
            cad::cad_project_sketch,
            cad::cad_compare,
            cad::cad_edge_seat,
            cad::cad_flat_pattern,
            cad::cad_import_step,
            cad::cad_part_to_model,
            cad::cad_scan_pick,
            cad::cad_scan_detect,
            cad::cad_scan_slice,
            cad::cad_scan_add,
            cad::cad_scan_use,
            cad::cad_scan_outline,
            cad::cad_mm_per_unit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// La interfaz ya se dibujó: se cierra el splash y aparece la ventana principal
#[tauri::command]
fn app_ready(app: tauri::AppHandle) {
    show_main_window(&app);
}

/// Una sola vez: las siguientes llamadas no encuentran el splash y no hacen nada
fn show_main_window(app: &tauri::AppHandle) {
    let Some(splash) = app.get_webview_window("splash") else {
        return;
    };
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    let _ = splash.close();
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

/// El pellizco del touchpad y Ctrl + rueda hacen zoom de toda la página en
/// WebKitGTK: se vuelve siempre a 100 %
#[cfg(target_os = "linux")]
#[allow(deprecated)]
fn lock_page_zoom(app: &tauri::App) {
    use gtk::prelude::WidgetExt;
    use webkit2gtk::glib::prelude::*;
    use webkit2gtk::WebViewExt;
    for window in app.webview_windows().values() {
        let _ = window.with_webview(|webview| {
            let view = webview.inner();
            // El pellizco del touchpad llega como evento de GDK y WebKit lo
            // aplica a la escala de la página: se corta y se reenvía a la web
            // como rueda con Ctrl, que es lo que entienden los visores
            let last = std::rc::Rc::new(std::cell::Cell::new(1.0_f64));
            view.connect_event(move |view, event| {
                let Some(pinch) = event.downcast_ref::<gtk::gdk::EventTouchpadPinch>() else {
                    return webkit2gtk::glib::Propagation::Proceed;
                };
                let raw: &gtk::gdk::ffi::GdkEventTouchpadPinch = pinch.as_ref();
                // fase 0 = comienzo: la escala vuelve a partir de 1
                if raw.phase == 0 {
                    last.set(1.0);
                }
                let scale = pinch.scale();
                if raw.phase == 1 && scale > 0.0 && last.get() > 0.0 {
                    let delta = -(scale / last.get()).ln() * 1000.0;
                    let (x, y) = pinch.position();
                    let js = format!(
                        "(()=>{{const t=document.elementFromPoint({x},{y})||document.body;\
                         t.dispatchEvent(new WheelEvent('wheel',{{bubbles:true,cancelable:true,ctrlKey:true,\
                         clientX:{x},clientY:{y},deltaY:{delta}}}));}})()"
                    );
                    view.run_javascript(&js, None::<&gtk::gio::Cancellable>, |_| {});
                }
                last.set(scale);
                webkit2gtk::glib::Propagation::Stop
            });
            view.connect_notify_local(Some("zoom-level"), |view, _| {
                if (view.zoom_level() - 1.0).abs() > f64::EPSILON {
                    view.set_zoom_level(1.0);
                }
            });
        });
    }
}
