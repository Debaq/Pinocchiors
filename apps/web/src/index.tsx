/* @refresh reload */
import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { App } from "./App";
import "./styles/app.css";
import { applyTheme, storedTheme } from "./lib/theme";
import { installWheelNumbers } from "./lib/wheelNumber";
import { applyLocale, t } from "./lib/i18n";

// Antes del primer cuadro, para no pasar por el tema oscuro al abrir en claro
applyTheme(storedTheme());
applyLocale();

// El splash muestra cada paso del arranque (sin Tauri no hay a quién avisar)
const bootStep = (step: string) => void invoke("boot_step", { step }).catch(() => {});
bootStep("ui");

const root = document.getElementById("root");

if (!root) {
  throw new Error("Root element not found");
}

// Sin el menú del navegador (Recargar, Inspeccionar…): cada zona pone el suyo.
// En campos de texto queda el nativo para copiar y pegar
document.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement | null;
  if (!target?.closest("input, textarea, [contenteditable='true']")) e.preventDefault();
});

// El pellizco del touchpad llega como rueda con Ctrl (o gesto de Safari) y el
// webview lo usa para acercar toda la aplicación; las vistas lo manejan solas
window.addEventListener("wheel", (e) => {
  if (e.ctrlKey) e.preventDefault();
}, { passive: false, capture: true });
for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
  document.addEventListener(type, (e) => e.preventDefault(), { passive: false });
}
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && (e.key === "+" || e.key === "-" || e.key === "=" || e.key === "0") && !e.defaultPrevented) {
    // Sin zoom de página por teclado; los atajos propios corren antes y ya lo marcan
    e.preventDefault();
  }
});

// Rueda del mouse sobre un campo numérico: sube o baja el valor
installWheelNumbers();

// Un error de la interfaz no deja la ventana en blanco: se muestra y se puede
// recargar (el trabajo del backend sigue ahí y la recuperación también)
render(
  () => (
    <ErrorBoundary
      fallback={(error, reset) => (
        <div style={{ padding: "24px", "font-family": "sans-serif", color: "var(--color-text, #ddd)", background: "var(--color-bg, #1e1e1e)", height: "100vh" }}>
          <h2 style={{ "font-size": "16px", margin: "0 0 8px" }}>{t("La interfaz tuvo un error")}</h2>
          <pre style={{ "white-space": "pre-wrap", "font-size": "12px", opacity: 0.8 }}>{`${error?.name ?? "Error"}: ${error?.message ?? error}\n\n${error?.stack ?? ""}`}</pre>
          <button style={{ "margin-top": "12px", padding: "6px 12px" }} onClick={reset}>
            {t("Reintentar")}
          </button>{" "}
          <button style={{ "margin-top": "12px", padding: "6px 12px" }} onClick={() => location.reload()}>
            {t("Recargar la interfaz")}
          </button>
        </div>
      )}
    >
      <App />
    </ErrorBoundary>
  ),
  root
);

// La interfaz ya está armada: se cierra el splash (otra ventana) y aparece
// esta. Con setTimeout y no requestAnimationFrame, que no corre mientras la
// ventana está oculta. Sin Tauri (vista previa) no hay nada que avisar
setTimeout(() => {
  bootStep("app");
  void invoke("app_ready").catch(() => {});
});
