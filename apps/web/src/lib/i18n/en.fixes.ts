import type { Catalog } from "../i18n";

/**
 * Va al final de en.ts: palabras españolas con más de un sentido en inglés.
 * Las claves con `contexto|texto` son para `tx(contexto, texto)`; las sin
 * contexto fijan la traducción general cuando dos zonas discrepaban
 */
export const enFixes: Catalog = {
  "estado|Listo": "Ready",
  "dirección|Arriba": "Up",
  "silencio|Activar": "Unmute",
  "cad|Corte": "Split",
  "curva|Automática": "Auto clamped",
  "simetría|Simetría": "Symmetry",
  "restricción|Simetría": "Symmetric",
  "confirmar|Seguir": "Continue",
  "Planta": "Plantigrade",
  "Arriba": "Top",
  "Listo": "Done",
};
