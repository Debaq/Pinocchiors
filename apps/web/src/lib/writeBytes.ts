// Guardar archivos binarios desde la interfaz sin pasarlos por JSON (como lista
// de números pesaban 4 veces más y había que armarla y leerla entera).

import { invoke } from "@tauri-apps/api/core";

/** Escribe `bytes` en `path` tal cual; devuelve cuántos bytes se escribieron */
export function writeBytes(path: string, bytes: Uint8Array): Promise<number> {
  return invoke<number>("write_render_image", bytes, { headers: { path: encodeURIComponent(path) } });
}

/** Bytes de un texto en base64 */
export function base64Bytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Bytes de un `data:` URL en base64 */
export function dataUrlBytes(url: string): Uint8Array {
  return base64Bytes(url.slice(url.indexOf(",") + 1));
}
