/* @refresh reload */
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

render(() => <App />, root);
