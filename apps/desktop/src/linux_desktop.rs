//! Acceso directo en Linux: al abrir la app se revisa que exista
//! `~/.local/share/applications/pinocchio.desktop` con su ícono y que apunte
//! al ejecutable actual (binario suelto o AppImage). Si falta o quedó
//! apuntando a otra versión, se reescribe.

use std::path::{Path, PathBuf};

/// Nombre de la ventana para el escritorio (WM_CLASS en X11, app_id en
/// Wayland). Coincide con el nombre del `.desktop` para que el dock muestre
/// el ícono correcto aunque el binario cambie de nombre en cada versión.
pub const APP_ID: &str = "pinocchio";

const ICON_PNG: &[u8] = include_bytes!("../icons/128x128@2x.png");

/// Fija el nombre del programa antes de que GTK cree ventanas.
pub fn set_program_name() {
    webkit2gtk::glib::set_prgname(Some(APP_ID));
    webkit2gtk::glib::set_application_name("Pinocchio");
}

/// Crea o actualiza el `.desktop` y el ícono. Los errores solo se reportan.
pub fn ensure_desktop_entry() {
    // En desarrollo no se toca: apuntaría al binario de target/debug
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(e) = install() {
        eprintln!("[desktop] no se pudo crear el acceso directo: {e}");
    }
}

fn install() -> std::io::Result<()> {
    let Some(data) = data_home() else {
        return Ok(());
    };
    let exe = executable()?;

    let icon = data.join("icons/hicolor/256x256/apps").join(format!("{APP_ID}.png"));
    write_if_changed(&icon, ICON_PNG)?;

    let entry = desktop_entry(&exe, &icon);
    let file = data.join("applications").join(format!("{APP_ID}.desktop"));
    write_if_changed(&file, entry.as_bytes())
}

/// `$XDG_DATA_HOME` o `~/.local/share`.
fn data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")),
    }
}

/// Dentro de un AppImage el ejecutable vive en un montaje temporal; la ruta
/// estable es la del propio `.AppImage`.
fn executable() -> std::io::Result<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => std::env::current_exe(),
    }
}

fn desktop_entry(exe: &Path, icon: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Pinocchio\n\
         Comment=Modelado, rigging y animación 3D\n\
         Exec={}\n\
         Icon={}\n\
         Terminal=false\n\
         Categories=Graphics;3DGraphics;\n\
         StartupWMClass={APP_ID}\n\
         StartupNotify=true\n",
        quote_exec(&exe.to_string_lossy()),
        icon.display(),
    )
}

/// Comillas para el campo `Exec` según la especificación de `.desktop`.
fn quote_exec(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 2);
    out.push('"');
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn write_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_con_espacios_y_especiales() {
        assert_eq!(quote_exec("/home/a b/Pinocchio"), "\"/home/a b/Pinocchio\"");
        assert_eq!(quote_exec("/x/$y\"%"), "\"/x/\\$y\\\"%%\"");
    }

    #[test]
    fn entrada_apunta_al_ejecutable_y_al_icono() {
        let entry = desktop_entry(Path::new("/opt/Pinocchio"), Path::new("/i/pinocchio.png"));
        assert!(entry.contains("Exec=\"/opt/Pinocchio\"\n"));
        assert!(entry.contains("Icon=/i/pinocchio.png\n"));
        assert!(entry.contains("StartupWMClass=pinocchio\n"));
    }
}
