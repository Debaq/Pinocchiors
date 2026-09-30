/* @refresh reload */
import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import { App } from "./App";
import "./styles/app.css";
import { applyTheme, storedTheme } from "./lib/theme";

// Antes del primer cuadro, para no pasar por el tema oscuro al abrir en claro
applyTheme(storedTheme());

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

// Un error de la interfaz no deja la ventana en blanco: se muestra y se puede
// recargar (el trabajo del backend sigue ahí y la recuperación también)
render(
  () => (
    <ErrorBoundary
      fallback={(error, reset) => (
        <div style={{ padding: "24px", "font-family": "sans-serif", color: "var(--color-text, #ddd)", background: "var(--color-bg, #1e1e1e)", height: "100vh" }}>
          <h2 style={{ "font-size": "16px", margin: "0 0 8px" }}>La interfaz tuvo un error</h2>
          <pre style={{ "white-space": "pre-wrap", "font-size": "12px", opacity: 0.8 }}>{String(error?.stack ?? error)}</pre>
          <button style={{ "margin-top": "12px", padding: "6px 12px" }} onClick={reset}>
            Reintentar
          </button>{" "}
          <button style={{ "margin-top": "12px", padding: "6px 12px" }} onClick={() => location.reload()}>
            Recargar la interfaz
          </button>
        </div>
      )}
    >
      <App />
    </ErrorBoundary>
  ),
  root
);
