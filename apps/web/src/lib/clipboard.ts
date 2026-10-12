// Copiar texto al portapapeles del sistema (sin plugin de Tauri).

/**
 * Copia `text`; dice si pudo. Prueba la API del navegador y, si no está o
 * falla (WebKitGTK fuera de contexto seguro), un área de texto oculta con
 * `execCommand("copy")`. Llamar desde un clic (gesto del usuario).
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    return ok;
  }
}
