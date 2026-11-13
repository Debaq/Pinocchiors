//! Pinocchio en Android: lo que necesita código nativo (Kotlin).
//!
//! - Compartir un archivo con el menú de Android (WhatsApp, correo, Drive, el
//!   laminador…), con un `FileProvider` declarado en el manifiesto de este plugin
//!   (se une al de la app al compilar: no hace falta versionar `gen/android`).
//!
//! En escritorio el plugin no hace nada.

use tauri::{
    Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.pinocchio.android";

/// Acceso al lado Kotlin
pub struct PinocchioAndroid<R: Runtime>(#[allow(dead_code)] Option<tauri::plugin::PluginHandle<R>>);

#[cfg(target_os = "android")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SharePayload<'a> {
    path: &'a str,
    mime: &'a str,
    title: &'a str,
}

impl<R: Runtime> PinocchioAndroid<R> {
    /// Abre el menú de compartir de Android con este archivo. Tiene que estar en la
    /// carpeta de caché de la app (la única que expone el `FileProvider`).
    #[allow(unused_variables)]
    pub fn share_file(&self, path: &str, mime: &str, title: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            let handle = self.0.as_ref().ok_or("Compartir no está disponible")?;
            handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("shareFile", SharePayload { path, mime, title })
                .map(|_| ())
                .map_err(|e| format!("No se pudo compartir: {e}"))
        }
        #[cfg(not(target_os = "android"))]
        Err("Compartir solo está en Android".into())
    }
}

/// Acceso al plugin desde cualquier `Manager` (`app.pinocchio_android()`)
pub trait PinocchioAndroidExt<R: Runtime> {
    fn pinocchio_android(&self) -> &PinocchioAndroid<R>;
}

impl<R: Runtime, T: Manager<R>> PinocchioAndroidExt<R> for T {
    fn pinocchio_android(&self) -> &PinocchioAndroid<R> {
        self.state::<PinocchioAndroid<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("pinocchio-android")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let handle = Some(api.register_android_plugin(PLUGIN_IDENTIFIER, "PinocchioPlugin")?);
            #[cfg(not(target_os = "android"))]
            let handle = {
                let _ = api;
                None
            };
            app.manage(PinocchioAndroid::<R>(handle));
            Ok(())
        })
        .build()
}
