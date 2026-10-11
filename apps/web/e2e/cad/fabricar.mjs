// Fabricar: objetos, mallas del diseño y remallado.
//   node apps/web/e2e/cad/fabricar.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo fabricar              (lo mismo)

import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { BRIDGE, call, near, twoCubesStl, begin, body, accept, tab } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
  async "remallar: los modos con la retopología adentro"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // La caja pasa sola a malla al ir a Preparar
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    const tabs = await b.eval(`[...document.querySelectorAll("nav button[aria-label]")].map((t) => t.getAttribute("aria-label"))`);
    if (tabs.includes("Retopología") || !tabs.includes("Remallar")) throw new Error(`pestañas: ${tabs}`);
    await tab(b, "Remallar");
    const modes = await b.eval(`[...document.querySelectorAll('[aria-label="Modo de remallado"] [data-mode]')].map((x) => x.dataset.mode + (x.getAttribute("aria-checked") === "true" ? "*" : ""))`);
    if (modes.join() !== "retopology*,simplify,isotropic,voxel,quads,smooth") throw new Error(`modos: ${modes}`);
    // La retopología está adentro, con su botón
    if (!(await b.eval(`[...document.querySelectorAll("button")].some((x) => x.textContent.trim() === "Retopologizar")`))) throw new Error("sin el panel de retopología");
    await b.shot("remallar");
    // Todos los modos están; otro modo cambia el panel
    if (await b.eval(`document.querySelector('[aria-label="Modo de remallado"]').innerText.includes("pronto")`)) throw new Error("queda un modo marcado como pronto");
    await b.eval(`document.querySelector('[data-mode="quads"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-quads]")`))) throw new Error("sin el panel de A quads");
    if (await b.eval(`[...document.querySelectorAll("button")].some((x) => x.textContent.trim() === "Retopologizar")`)) throw new Error("el panel de retopología sigue a la vista");
  },

  async "remallar: simplificar con vista previa, aplicar y deshacer"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Triángulos del modelo en el backend (cabecera de get_mesh_data: vértices, índices…)
    const triangles = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="simplify"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-simplify]")`))) throw new Error("sin el panel de Simplificar");
    if (await b.eval(`!!document.querySelector("[data-remesh-pending]")`)) throw new Error("Simplificar sigue marcado como pendiente");
    const before = await triangles();
    if (!(before > 500)) throw new Error(`la esfera tiene pocos triángulos: ${before}`);

    // Vista previa: el modelo no cambia; el resultado se ve en alambre encima
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    near(after, before / 4, before * 0.05, "triángulos de la vista previa");
    if ((await triangles()) !== before) throw new Error("la vista previa cambió el modelo");
    if (!(await b.eval(`window.__viewer().hasPreviewOverlay`))) throw new Error("sin el alambre de la vista previa");
    if (!(await b.eval(`!!document.querySelector("[data-preview-view]")`))) throw new Error("sin el selector de la vista previa en el visor");
    await b.eval(`document.querySelector('[data-preview-view] [data-view="result"]').click()`);
    await sleep(300);
    if (await b.eval(`window.__viewer().hasPreviewOverlay`)) throw new Error("con Resultado no debería haber alambre encima");
    await b.shot("simplificar-vista-previa");

    // Aplicar: la malla cambia y la vista previa se va
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await triangles()) === before; t++) await sleep(250);
    const applied = await triangles();
    near(applied, after, 1, "triángulos aplicados");
    for (let t = 0; t < 20 && (await b.eval(`!!document.querySelector("[data-preview-view]")`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector("[data-preview-view]") || window.__viewer().hasPreviewOverlay`)) throw new Error("la vista previa sigue después de aplicar");
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("sin el resumen de lo aplicado");

    // Deshacer: vuelve la malla de antes
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await triangles()) !== before; t++) await sleep(250);
    if ((await triangles()) !== before) throw new Error(`deshacer no devolvió la malla: ${await triangles()} (antes ${before})`);
  },

  async "remallar: suavizar sin cambiar la conectividad, aplicar y deshacer"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Cabecera de get_mesh_data (vértices, índices…) y una huella de las posiciones
    const mesh = async () => {
      const buf = await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer();
      const bytes = new Uint8Array(buf);
      let h = 0;
      for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) | 0;
      return { triangles: new Uint32Array(buf, 0, 4)[1] / 3, hash: h };
    };
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="smooth"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-smooth]")`))) throw new Error("sin el panel de Suavizar");
    if (await b.eval(`!!document.querySelector("[data-remesh-pending]")`)) throw new Error("Suavizar sigue marcado como pendiente");
    const before = await mesh();

    // Vista previa: mismos triángulos y el modelo no cambia
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    if (after !== before.triangles) throw new Error(`suavizar cambió los triángulos: ${before.triangles} → ${after}`);
    if ((await mesh()).hash !== before.hash) throw new Error("la vista previa cambió el modelo");
    if (!(await b.eval(`window.__viewer().hasPreviewOverlay`))) throw new Error("sin el alambre de la vista previa");
    await b.shot("suavizar-vista-previa");

    // Cambiar una opción descarta la vista previa
    await b.eval(`[...document.querySelectorAll("[data-smooth] label")].find((x) => x.textContent.includes("Sin deslizar")).click()`);
    for (let t = 0; t < 20 && (await b.eval(`!!document.querySelector("[data-preview-view]")`)); t++) await sleep(250);
    if (await b.eval(`window.__viewer().hasPreviewOverlay`)) throw new Error("cambiar la opción no descartó la vista previa");

    // Aplicar: cambian las posiciones, no los triángulos
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await mesh()).hash === before.hash; t++) await sleep(250);
    const applied = await mesh();
    if (applied.hash === before.hash) throw new Error("aplicar no movió nada");
    if (applied.triangles !== before.triangles) throw new Error(`aplicar cambió los triángulos: ${applied.triangles}`);
    for (let t = 0; t < 20 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("sin el resumen de lo aplicado");

    // Deshacer: vuelve la malla de antes
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await mesh()).hash !== before.hash; t++) await sleep(250);
    if ((await mesh()).hash !== before.hash) throw new Error("deshacer no devolvió la malla");
  },

  async "remallar: isótropo con lado en mm y aristas vivas"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 30, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    const triangles = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector("[data-isotropic-broken]")`)) throw new Error("la caja no debería figurar rota");
    const before = await triangles();
    // Lado de 2 mm: la caja de 30 × 20 × 10 (2200 mm²) da ≈ 1270 triángulos
    await b.eval(`(() => { const i = document.querySelector("[data-isotropic] input[type=number]"); i.value = "2"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
    await sleep(300);
    const estimate = await b.eval(`parseInt(document.querySelector("[data-isotropic-estimate]").textContent.replace(/\\D/g, ""))`);
    near(estimate, 1270, 30, "triángulos estimados");
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 80 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    near(after, 1270, 400, "triángulos de la vista previa");
    // Con las aristas vivas, la caja sigue siendo caja (se aleja menos de una décima de mm)
    const stats = await call("remesh_preview", { params: { mode: "isotropic", edge_mm: 2, sharp_angle: 45, iterations: 5, thin_features: false } });
    if (!(stats.deviation.max_mm < 0.1)) throw new Error(`la caja se deformó: ${JSON.stringify(stats.deviation)}`);
    await b.shot("isotropo-vista-previa");
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 80 && (await triangles()) === before; t++) await sleep(250);
    near(await triangles(), after, 1, "triángulos aplicados");
    // (el campo del lado tiene el foco: Ctrl+Z sería su deshacer de texto)
    await b.eval(`document.activeElement?.blur()`);
    // Recién aplicado la app a veces sigue ocupada y no toma el atajo (inestable antes de E4)
    await sleep(3000);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await triangles()) !== before; t++) await sleep(250);
    if ((await triangles()) !== before) throw new Error(`deshacer no devolvió la malla: ${await triangles()} triángulos (antes ${before}, remallada ${after})`);
  },

  async "remallar: vóxeles une piezas que se cruzan y avisa si es pesado"(b) {
    // STL con dos cubos de 20 mm que se cruzan (dos cáscaras metidas una en otra)
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    mkdirSync(out, { recursive: true });
    const path = `${out}/dos-cubos.stl`;
    writeFileSync(path, twoCubesStl());
    await b.eval(`window.__nextPath = [${JSON.stringify(path)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(3000);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="voxel"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-voxel-grid]")`)); t++) await sleep(250);
    const grid = await b.eval(`document.querySelector("[data-voxel-grid]").textContent`);
    if (!/Vóxel de/.test(grid) || !/MB de memoria/.test(grid)) throw new Error(`sin la estimación de la grilla: ${grid}`);
    // Volumen de las dos cajas por separado: 16000; la unión, 15000
    // (en el puente de depuración tarda ~30 s: grilla, emparejado y normales)
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 360 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("no se aplicó");
    const analysis = await call("analyze_print3d");
    if (!analysis.is_closed) throw new Error("la superficie rehecha no es cerrada");
    near(analysis.volume, 15000, 300, "volumen de la unión");
    await b.shot("voxeles-aplicado");
    // Con el detalle al máximo, pregunta antes de correr
    await b.eval(`(() => { const s = [...document.querySelectorAll("[data-voxel] [role=slider]")][0]; s.focus(); for (let k = 0; k < 80; k++) s.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })); })()`);
    for (let t = 0; t < 20 && !(await b.eval(`/640/.test(document.querySelector("[data-voxel] [role=slider]")?.getAttribute("aria-valuenow") ?? "")`)); t++) await sleep(150);
    await sleep(500);
    // (≈ 12 millones de triángulos estimados para esta pieza)
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    // (antes de preguntar calcula la grilla del detalle elegido)
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Vóxeles muy finos")`)); t++) await sleep(250);
    if (!(await b.eval(`document.body.innerText.includes("Vóxeles muy finos")`))) throw new Error("no preguntó antes de una grilla pesada");
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
  },

  async "remallar: triángulos a quads y OBJ con quads"(b) {
    await begin(b);
    // Una caja: cada cara es un rectángulo en dos triángulos
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 30, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    const header = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="quads"]').click()`);
    await sleep(300);
    const before = await header();
    if (before[3] !== 0) throw new Error("la caja ya traía quads");
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-after]")`)); t++) await sleep(250);
    const after = await b.eval(`document.querySelector("[data-remesh-after]").textContent`);
    if (!/^6 quads/.test(after.trim())) throw new Error(`vista previa: ${after}`);
    await b.shot("quads-vista-previa");
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await header())[3] === 0; t++) await sleep(250);
    const applied = await header();
    // Mismos vértices y triángulos; 6 caras de 4 índices para el alambre
    if (applied[0] !== before[0] || applied[1] !== before[1]) throw new Error(`cambió la malla: ${before} → ${applied}`);
    if (applied[3] !== 24) throw new Error(`índices de quads: ${applied[3]}`);
    // OBJ: seis caras de cuatro vértices
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    mkdirSync(out, { recursive: true });
    const path = `${out}/caja-quads.obj`;
    await call("export_model", { config: { format: "obj", path } });
    const faces = readFileSync(path, "utf8").split("\n").filter((l) => l.startsWith("f "));
    if (faces.length !== 6 || !faces.every((l) => l.trim().split(/\s+/).length === 5)) throw new Error(`caras del OBJ: ${faces.join(" | ")}`);
    // Deshacer: sin quads
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await header())[3] !== 0; t++) await sleep(250);
    if ((await header())[3] !== 0) throw new Error("deshacer no quitó los quads");
  },

  async "remallar: isótropo traslada textura y pesos, o los descarta"(b) {
    const model = process.env.E2E_RIGGED ?? `${process.env.HOME}/Descargas/chilesaurus_animado.glb`;
    if (!existsSync(model)) return console.log(`  (sin ${model}: se salta)`);
    await b.eval(`window.__nextPath = [${JSON.stringify(model)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(4000);
    const header = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    await b.eval(`(() => { const i = document.querySelector("[data-isotropic] input[type=number]"); i.value = "12"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); i.blur(); })()`);
    await sleep(300);
    const before = await header();
    if (before[2] !== 1) throw new Error("el modelo no trae UV");
    await b.shot("isotropo-antes");

    // Aplicar pregunta qué trasladar: las dos casillas, marcadas
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-confirm-choices]")`)); t++) await sleep(250);
    const choices = await b.eval(`[...document.querySelectorAll("[data-choice]")].map((c) => c.dataset.choice + (c.querySelector("input")?.checked ? "*" : ""))`);
    if (choices.join() !== "texture*,rig*") throw new Error(`casillas: ${choices}`);
    await b.shot("isotropo-que-trasladar");
    await b.eval(`document.querySelector("[data-confirm]").click()`);
    for (let t = 0; t < 240 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    const baked = await header();
    if (baked[2] !== 1) throw new Error("la malla nueva quedó sin UV: no se horneó la textura");
    if (!(await call("remesh_info")).has_rig) throw new Error("los pesos no pasaron a la malla nueva");
    await sleep(1500);
    await b.shot("isotropo-horneado");

    // Deshacer y aplicar sin trasladar nada
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await header())[1] !== before[1]; t++) await sleep(250);
    await sleep(1000);
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-confirm-choices]")`)); t++) await sleep(250);
    await b.eval(`document.querySelectorAll("[data-choice] label").forEach((x) => x.click())`);
    await sleep(200);
    const unchecked = await b.eval(`[...document.querySelectorAll("[data-choice] input")].every((i) => !i.checked)`);
    if (!unchecked) throw new Error("no se pudieron destildar las casillas");
    await b.eval(`document.querySelector("[data-confirm]").click()`);
    for (let t = 0; t < 120 && (await header())[1] === before[1]; t++) await sleep(250);
    const plain = await header();
    if (plain[2] !== 0) throw new Error("sin trasladar, la malla nueva no debería tener UV");
    if ((await call("remesh_info")).has_rig) throw new Error("sin trasladar, los pesos deberían descartarse");
  },

  async "objetos: las piezas pasan solas a Fabricar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 51, name: "Cilindro aparte", kind: { type: "primitive", shape: { type: "cylinder", radius: 5, height: 10 }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 52;
    })`);
    await sleep(2500);
    const rows = () => b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    const row = (label) => `[...document.querySelector("[data-outliner]").querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(label)})`;
    // Sin botones: a Fabricar y la primera pieza ya tiene malla
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1500);
    let got = await rows();
    if (!got.includes("Malla generada") || got.indexOf("Malla generada") !== got.indexOf("Pieza 1") + 1) throw new Error(`sin malla bajo la pieza 1: ${JSON.stringify(got)}`);
    // Otra pieza: clic en el Outliner y pasa a ser el objeto activo
    await b.eval(`${row("Pieza 2")}.click()`);
    for (let t = 0; t < 40 && (await rows()).indexOf("Malla generada") !== (await rows()).indexOf("Pieza 2") + 1; t++) await sleep(250);
    got = await rows();
    if (got.indexOf("Malla generada") !== got.indexOf("Pieza 2") + 1) throw new Error(`la pieza 2 no quedó activa: ${JSON.stringify(got)}`);
    const objs = await b.eval(`JSON.stringify(window.__objects?.().map((o) => [o.name, !!o.source.hash, !!o.ui]))`);
    if (objs !== JSON.stringify([["Pieza 1", true, true], ["Pieza 2", true, false]])) throw new Error(`objetos: ${objs}`);
    // La pieza 1 se sigue viendo, en gris
    for (let t = 0; t < 20 && (await b.eval(`window.__viewer().ghostCount`)) !== 1; t++) await sleep(250);
    if ((await b.eval(`window.__viewer().ghostCount`)) !== 1) throw new Error("la pieza 1 no se ve en gris");
    await b.shot("objetos-fantasma");
    // Se cambia el diseño (cilindro más ancho) y al volver a Fabricar la malla se regenera sola
    const before = await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features.find((f) => f.id === 51).kind.shape.radius = 8; })`);
    await sleep(2000);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && (await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`)) === before; t++) await sleep(250);
    const after = await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`);
    if (after === before) throw new Error("la malla no se regeneró");
    await b.shot("objetos");
  },

  async "mallas: caja del diseño en Fabricar"(b) {
    // La malla soldada de una pieza trae una sola normal por vértice en las
    // aristas vivas: la marca por vértice hace que las caras se dibujen planas
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(2000);
    const info = JSON.parse(await b.eval(`JSON.stringify((() => { let out = null; window.__viewer().scene.traverse((o) => { const g = o.geometry; if (!out && o.isMesh && g?.attributes?.position?.count === 8) out = { crease: g.attributes.crease ? [...g.attributes.crease.array] : null, smooth: "AUTO_SMOOTH" in (o.material.defines ?? {}) }; }); return out; })())`));
    if (!info) throw new Error("no está la malla de la caja");
    if (JSON.stringify(info.crease) !== JSON.stringify(Array(8).fill(1))) throw new Error(`marcas de aristas vivas: ${JSON.stringify(info.crease)}`);
    if (!info.smooth) throw new Error("el material no tiene el suavizado automático");
    // Las unidades llegan: la grilla en mm y la cámara afuera de la caja
    if (!(await b.eval(`document.body.innerText.includes("Grilla: 1 mm")`))) throw new Error("la grilla no está en mm");
    if ((await b.eval(`window.__viewer().camera.position.length()`)) < 20) throw new Error("la cámara quedó dentro de la caja");
    await b.shot("mallas-caja-fabricar");
  },

  async "mallas: STL del diseño importado"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 61, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 62, name: "Cilindro", kind: { type: "primitive", shape: { type: "cylinder", radius: 8, height: 20 }, origin: [35, 0, -10], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 63, name: "Esfera", kind: { type: "primitive", shape: { type: "sphere", radius: 10 }, origin: [-35, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 64, name: "Cono", kind: { type: "primitive", shape: { type: "cone", r1: 9, r2: 3, height: 20 }, origin: [0, 35, -10], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 65;
    })`);
    await sleep(3000);
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/mallas-diseno.stl`;
    const r = await call("cad_export", { path, format: "stl" });
    if (r?.error) throw new Error(JSON.stringify(r));
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", ctrlKey: true }))`);
    for (let t = 0; t < 60 && !(await b.eval(`document.body.innerText.includes("mallas-diseno")`)); t++) await sleep(250);
    await sleep(2500);
    // Caja y tapas planas; la esfera y los costados curvos quedan suaves
    const share = await b.eval(`(() => { let out = null; window.__viewer().scene.traverse((o) => { const c = o.geometry?.attributes?.crease; if (!out && o.isMesh && c && o.geometry.attributes.position.count > 1000) out = c.array.reduce((s, x) => s + x, 0) / c.count; }); return out; })()`);
    if (share === null || !(share > 0.05 && share < 0.6)) throw new Error(`vértices en aristas vivas: ${share}`);
    await b.click(31, 276);
    await sleep(1500);
    await b.shot("mallas-stl-importado");
  },

  async "objetos: varios elegidos se exportan juntos"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 20, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 51, name: "Cilindro", kind: { type: "primitive", shape: { type: "cylinder", radius: 5, height: 10 }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 52;
    })`);
    await sleep(2500);
    const row = (label) => `[...document.querySelector("[data-outliner]").querySelectorAll("[data-outliner-row]")].find((x) => x.innerText.split("\\n")[0] === ${JSON.stringify(label)})`;
    const selected = () => b.eval(`JSON.stringify(window.__selectedObjects())`);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    // La pieza 2 activa (así las dos tienen malla) y la 1 sumada con Ctrl + clic
    await b.eval(`${row("Pieza 2")}.click()`);
    for (let t = 0; t < 40 && !(await b.eval(`window.__objects().find((o) => o.name === "Pieza 1")?.ui != null`)); t++) await sleep(250);
    await sleep(1000);
    await b.eval(`${row("Pieza 1")}.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }))`);
    await sleep(800);
    const ids = JSON.parse(await selected());
    if (ids.length !== 2) throw new Error(`elegidos: ${JSON.stringify(ids)}`);
    // El panel lista los dos con sus medidas
    for (let t = 0; t < 20 && (await b.eval(`document.querySelectorAll("[data-selected-object]").length`)) !== 2; t++) await sleep(250);
    for (let t = 0; t < 20 && (await b.eval(`[...document.querySelectorAll("[data-selected-object]")].some((e) => e.innerText.includes("…"))`)); t++) await sleep(250);
    const sizes = await b.eval(`[...document.querySelectorAll("[data-selected-object]")].map((e) => e.innerText).join(" | ")`);
    if (!sizes.includes("20 × 20 × 20 mm") || !sizes.includes("10 × 10 × 10 mm")) throw new Error(`medidas: ${sizes}`);
    // El de la Pieza 1 en el visor, con el acento
    if (!(await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].filter((e) => e.className.includes("bg-accent")).length === 2`)))
      throw new Error("el Outliner no marca los dos");
    await b.shot("objetos-elegidos");
    // Exportar juntos a 3MF, acomodados en la cama
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/juntos.3mf`;
    rmSync(path, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.clickText("Exportar juntos");
    for (let t = 0; t < 40 && !existsSync(path); t++) await sleep(250);
    if (!existsSync(path) || statSync(path).size < 500) throw new Error("no se exportó el 3MF");
    // Un clic sin Ctrl vuelve a uno solo
    await b.eval(`${row("Pieza 1")}.click()`);
    await sleep(1500);
    if (JSON.parse(await selected()).length !== 1) throw new Error(`sin Ctrl: ${await selected()}`);
  },

  async "visor principal: derecho desplaza o abre el menú, Ver todo encuadra"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    const rect = await b.eval(`(() => { const r = window.__viewer().canvas.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()`);
    const [cx, cy] = [rect[0] + rect[2] * 0.3, rect[1] + rect[3] * 0.6];
    const target = () => b.eval(`window.__viewer().controls.target.toArray()`);
    const menuOpen = () => b.eval(`!!document.querySelector('[role=menu]')`);
    // Clic derecho quieto: menú
    await b.click(cx, cy, { button: "right", buttons: 2, wait: 800 });
    if (!(await menuOpen())) throw new Error("el clic derecho no abrió el menú");
    await b.eval(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
    await sleep(400);
    if (await menuOpen()) await b.click(rect[0] + 5, rect[1] + 5, { wait: 400 });
    // Arrastrar con el derecho: desplaza y no abre el menú
    const t0 = await target();
    await b.mouse("mouseMoved", cx, cy, { buttons: 0 });
    await b.mouse("mousePressed", cx, cy, { button: "right", buttons: 2 });
    for (let i = 1; i <= 15; i++) {
      await b.mouse("mouseMoved", cx + i * 6, cy, { button: "right", buttons: 2 });
      await sleep(20);
    }
    await b.mouse("mouseReleased", cx + 90, cy, { button: "right", buttons: 0 });
    await sleep(500);
    const t1 = await target();
    if (!(Math.hypot(...t1.map((v, i) => v - t0[i])) > 1e-4)) throw new Error("el derecho no desplazó");
    if (await menuOpen()) throw new Error("arrastrar con el derecho abrió el menú");
    // Ver todo: el centro de giro vuelve al centro de la caja y entra entera
    await b.eval(`window.__viewer().resetView()`);
    await sleep(500);
    const t2 = await target();
    if (Math.hypot(...t2.map((v, i) => v - t0[i])) > Math.hypot(...t1.map((v, i) => v - t0[i]))) throw new Error(`Ver todo no volvió: ${t0} ${t2}`);
  },

  async "visor: la misma cámara en Diseñar y en Fabricar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    const main = () => b.eval(`JSON.parse(JSON.stringify(window.__viewer().getCameraPose()))`);
    const cadPose = () => b.eval(`JSON.parse(JSON.stringify(window.__cadViewer.cameraPose()))`);
    const close = (a, c, what) => {
      for (const k of ["position", "target"]) {
        const d = Math.hypot(...a[k].map((v, i) => v - c[k][i]));
        const size = Math.hypot(...a.position.map((v, i) => v - a.target[i]));
        if (d > 1e-3 * size) throw new Error(`${what}: ${k} ${JSON.stringify(a[k])} contra ${JSON.stringify(c[k])}`);
      }
    };
    // Mirar desde arriba en Fabricar y pasar a Diseñar: la misma vista
    await b.eval(`window.__viewer().setView("top")`);
    await sleep(800);
    const fromMain = await main();
    await b.clickText("Diseñar");
    await sleep(2000);
    close(await cadPose(), fromMain, "al entrar a Diseñar");
    // De frente en Diseñar y vuelta a Fabricar: la misma vista allá
    await b.eval(`window.__cadViewer.lookFrom([0, -1, 0])`);
    await sleep(800);
    const fromCad = await cadPose();
    await b.clickText("Fabricar");
    await sleep(2500);
    close(await main(), fromCad, "al volver a Fabricar");
  },

  async "objetos: el remallado se rehace si cambia el diseño"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Desvío relativo del largo de las aristas: una malla isótropa lo tiene chico
    const spread = async () => {
      const buf = await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer();
      const [nv, ni] = new Uint32Array(buf, 0, 4);
      const p = new Float32Array(buf, 16, nv * 3);
      const uvs = new Uint32Array(buf, 0, 4)[2] ? nv * 2 : 0;
      const idx = new Uint32Array(buf, 16 + 4 * (nv * 6 + uvs), ni);
      const lengths = [];
      for (let t = 0; t < ni; t += 3)
        for (let k = 0; k < 3; k++) {
          const [a, c] = [idx[t + k], idx[t + ((k + 1) % 3)]];
          lengths.push(Math.hypot(p[3 * a] - p[3 * c], p[3 * a + 1] - p[3 * c + 1], p[3 * a + 2] - p[3 * c + 2]));
        }
      const mean = lengths.reduce((x, y) => x + y, 0) / lengths.length;
      return Math.sqrt(lengths.reduce((x, y) => x + (y - mean) ** 2, 0) / lengths.length) / mean;
    };
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__objects?.()[0]?.source.hash`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    const raw = await spread();
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 120 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    const even = await spread();
    if (!(even < 0.25 && even < raw)) throw new Error(`no quedó isótropa: desvío ${even} (antes ${raw})`);
    const extent0 = (await call("remesh_info")).extent_mm;
    near(extent0, 40, 1, "tamaño");

    // El diseño cambia: la bola el doble de grande
    const hash = await b.eval(`window.__objects()[0].source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features.find((f) => f.id === 50).kind.shape.radius = 40; })`);
    await sleep(2000);
    await b.clickText("Preparar");
    for (let t = 0; t < 60 && (await b.eval(`window.__objects()[0].source.hash`)) === hash; t++) await sleep(250);
    for (let t = 0; t < 120 && !(await b.eval(`/El diseño cambió/.test(document.body.innerText)`)); t++) await sleep(250);
    const status = await b.eval(`document.body.innerText.match(/El diseño cambió[^\\n]*/)?.[0] ?? ""`);
    if (!status.includes("1 modificación")) throw new Error(`aviso: ${status}`);
    // La malla nueva es la bola grande, otra vez isótropa (el lado automático siguió al tamaño)
    near((await call("remesh_info")).extent_mm, 80, 2, "tamaño rehecho");
    const again = await spread();
    if (!(again < 0.25)) throw new Error(`el remallado no se rehízo: desvío ${again}`);
    // La barra del visor cuenta la malla rehecha (no la de antes del cambio)
    const faces = new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    const shown = await b.eval(`parseInt(document.body.innerText.match(/Caras:\\s*([\\d.]+)/)?.[1].replace(/\\./g, "") ?? "0")`);
    if (shown !== faces) throw new Error(`la barra dice ${shown} caras, la malla tiene ${faces}`);
    const got = await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    if (!got.some((r) => r.startsWith("Remallado isótropo (automático)"))) throw new Error(`árbol del objeto: ${JSON.stringify(got)}`);
    await b.shot("objetos-remallado-rehecho");
  },

  async "objetos: lo hecho sobre la malla se rehace si cambia el diseño"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__objects?.()[0]?.source.hash`)); t++) await sleep(250);
    await sleep(1000);
    const dims = async () => (await call("analyze_print3d")).dimensions;
    const d0 = await dims();
    if (!(await call("analyze_print3d")).is_closed) throw new Error("la malla de la pieza no quedó cerrada");
    // Escalar ×2 desde Fabricar
    await b.clickText("Analizar para impresión");
    await sleep(1000);
    await b.eval(`(() => { const i = [...document.querySelectorAll("label")].find((l) => l.textContent.trim().startsWith("×"))?.querySelector("input"); i.value = "2"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
    await b.clickText("Escalar");
    await sleep(2000);
    const d1 = await dims();
    near(d1[0], 2 * d0[0], 1e-3 * d0[0], "ancho escalado");
    // El diseño cambia: la caja el doble de ancha
    const hash = await b.eval(`window.__objects()[0].source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features[0].kind.shape.dx = 40; })`);
    await sleep(2000);
    await b.clickText("Fabricar");
    for (let t = 0; t < 60 && (await b.eval(`window.__objects()[0].source.hash`)) === hash; t++) await sleep(250);
    await sleep(2000);
    // La malla nueva sale ya escalada: el ancho es 2 × 2 y lo demás 2 ×
    const d2 = await dims();
    near(d2[0], 4 * d0[0], 1e-3 * d0[0], "ancho rehecho");
    near(d2[1], 2 * d0[1], 1e-3 * d0[1], "alto rehecho");
    // El panel muestra el análisis de la malla nueva
    const shown = await b.eval(`document.body.innerText.match(/Dimensiones\\s*([\\d.]+)/)?.[1]`);
    near(parseFloat(shown), d2[0], 0.1, "ancho en el panel");
    const status = await b.eval(`document.body.innerText.match(/El diseño cambió[^\\n]*/)?.[0] ?? ""`);
    if (!status.includes("1 modificación")) throw new Error(`aviso: ${status}`);
    // En el Outliner, la pieza con su malla y lo hecho sobre ella
    const got = await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    const at = got.indexOf("Pieza 1");
    if (got[at + 1] !== "Malla generada" || got[at + 2] !== "Escalar para fabricar") throw new Error(`árbol del objeto: ${JSON.stringify(got.slice(at))}`);
    await b.shot("objetos-rehecho");
  },
};

siEsPrincipal(import.meta.url, escenarios);
