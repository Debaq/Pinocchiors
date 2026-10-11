// Pruebas de punta a punta del espacio Diseñar con el backend CAD real, sin
// Tauri: el arnés (cad-harness.html) manda los comandos al puente HTTP.
// Los escenarios están repartidos por tema en e2e/cad/*.mjs; el ejecutor
// (e2e/cad/_ejecutor.mjs) los corre con un Chromium por trabajador.
//
//   cargo build -p pinocchio-app --example cad_http     # backend (lo levanta el ejecutor si no está)
//   node apps/web/e2e/cad.mjs                            # todo (Vite y cad_http se levantan solos)
//   node apps/web/e2e/cad.mjs -j 3                       # 3 trabajadores, cada uno con su backend
//   node apps/web/e2e/cad.mjs --grupo plano2d            # un grupo (= node apps/web/e2e/cad/plano2d.mjs)
//   node apps/web/e2e/cad.mjs redondeo                   # los que tengan «redondeo» en el nombre
//   node apps/web/e2e/cad.mjs --exacto "chapa: alivios"  # uno solo
//   node apps/web/e2e/cad.mjs --lista                    # qué hay en cada grupo
//
// Capturas en $E2E_OUT (por defecto /tmp/pinocchio-e2e), donde también queda
// tiempos.json: con -j los escenarios más largos se reparten primero.

import { ejecutar } from "./cad/_ejecutor.mjs";

await ejecutar().catch((e) => {
  console.error(`✗ ${e.message}`);
  process.exit(1);
});
