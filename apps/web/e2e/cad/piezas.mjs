// Piezas, materiales, ensamble, configuraciones, versiones y parámetros.
//   node apps/web/e2e/cad/piezas.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo piezas              (lo mismo)

import { existsSync, readFileSync, rmSync, statSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { BRIDGE, call, near, begin, evaluate, sketchOn, body, accept, tab, setInput } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
  async "piezas: nueva, color, ocultar y exportar"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const parts = async () => (await evaluate()).parts;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Cilindro como pieza nueva, al lado
    await b.clickText("Cilindro");
    await sleep(1500);
    await b.eval(`__cadStore.commit((d) => { const k = d.features.at(-1).kind; k.op = "new"; k.origin = [40, 0, -10]; })`);
    await sleep(1500);
    if ((await b.eval(`__cadStore.tool()?.op`)) !== "new") throw new Error("la herramienta no es de pieza nueva");
    await accept(b);
    let ps = await parts();
    if (ps.length !== 2 || ps[0].name !== "Pieza 1" || ps[1].name !== "Pieza 2") throw new Error(`piezas: ${JSON.stringify(ps.map((p) => p.name))}`);
    near(ps[0].volume, 8000, 1e-6, "caja");
    // Una arista de la caja: el redondeo solo cambia la caja
    const cyl = ps[1].volume;
    await b.click(...(await at([0, -10, 10])), { wait: 800 });
    await b.clickText("Redondeo");
    await sleep(1500);
    await accept(b);
    ps = await parts();
    near(ps[1].volume, cyl, 1e-9, "el cilindro no se tocó");
    if (!(ps[0].volume < 8000)) throw new Error("no se redondeó la caja");
    // Color y nombre de la segunda (no recalculan)
    const version = await b.eval(`__cadStore.result().version`);
    await tab(b, "Pieza");
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Color de Pieza 2"]'); i.value = "#ff0000"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(800);
    await b.eval(`document.querySelector('[data-part="${ps[1].id.feature}:0"] span.truncate').dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`);
    await sleep(400);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nombre de la pieza"]'); i.value = "Eje"; i.blur(); })()`);
    await sleep(1200);
    const props = (await call("cad_get_document")).parts;
    if (props?.[0]?.color !== "#ff0000" || props[0].name !== "Eje") throw new Error(`propiedades: ${JSON.stringify(props)}`);
    if ((await b.eval(`__cadStore.result().version`)) !== version) throw new Error("cambiar color o nombre recalculó");
    if ((await parts())[1].name !== "Eje") throw new Error("no cambió el nombre");
    // Ocultar la caja: no se puede elegir donde estaba
    await b.eval(`document.querySelector('[aria-label="Ocultar Pieza 1"]').click()`);
    await sleep(1000);
    const [x, y] = await at([3, 3, 10]);
    const hit = await b.eval(`window.__cadViewer.pick(${x}, ${y}, { faces: true })?.kind ?? null`);
    if (hit === "face") throw new Error("se eligió una cara de la pieza oculta");
    await b.shot("piezas");
    // Exportar 3MF: dos objetos con nombre; y solo una pieza
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/piezas.3mf`;
    rmSync(path, { force: true });
    await call("cad_export", { path, format: "3mf" });
    const { execSync } = await import("node:child_process");
    const model = execSync(`unzip -p ${path} 3D/3dmodel.model`).toString();
    const objects = (model.match(/<object /g) ?? []).length;
    if (objects !== 2 || !model.includes('name="Eje"')) throw new Error(`3MF: ${objects} objetos`);
    // STEP: cada pieza con su nombre y el color elegido (rojo puro = color predefinido)
    const step = `${out}/piezas.step`;
    rmSync(step, { force: true });
    await call("cad_export", { path: step, format: "step" });
    const text = readFileSync(step, "latin1");
    if (!text.includes("'Eje'") || !text.includes("'Pieza 1'")) throw new Error("STEP sin los nombres de las piezas");
    if (!/DRAUGHTING_PRE_DEFINED_COLOUR\('red'\)|COLOUR_RGB/.test(text)) throw new Error("STEP sin el color");
    const one = `${out}/eje.stl`;
    rmSync(one, { force: true });
    await call("cad_export", { path: one, format: "stl", part: ps[1].id });
    if (!existsSync(one)) throw new Error("no se exportó la pieza sola");
    // Desde el área Exportar: elegir la pieza y exportarla como STL
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Eje");
    await sleep(400);
    await b.clickText("STL");
    const ui = `${out}/eje-ui.stl`;
    rmSync(ui, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(ui)}`);
    await b.clickText("Exportar STL");
    await sleep(2000);
    if (!existsSync(ui) || statSync(ui).size !== statSync(one).size) throw new Error("Exportar no sacó la pieza sola");
  },

  async "piezas: booleana, corte que separa y borrar"(b) {
    const parts = async () => (await evaluate()).parts;
    /** Activa la caja de piezas con ese título y hace clic en el visor en `p` (mm) */
    const tick = async (list, p) => {
      await b.eval(`document.querySelector('[aria-label=${JSON.stringify(list)}] [data-parts-box]').click()`);
      await sleep(1500);
      await b.click(...(await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`)), { wait: 800 });
    };
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un cilindro como pieza aparte que se mete en la caja
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    doc.features.push({ id: box.id + 1, name: "Cilindro 1", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 40 }, origin: [10, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = box.id + 2;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    if ((await parts()).length !== 2) throw new Error("no hay dos piezas");
    // Booleana: restar el cilindro de la caja
    await b.clickText("Booleana");
    await sleep(1500);
    await tick("Piezas que quedan", [-5, -10, 0]);
    await sleep(800);
    await tick("Piezas que restan", [10, -4, 16]);
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    await accept(b);
    let ps = await parts();
    if (ps.length !== 1) throw new Error(`booleana: ${ps.length} piezas`);
    near(ps[0].volume, 8000 - (Math.PI * 16 * 20) / 2, 0.5, "caja menos medio cilindro");
    // Un corte fino por el medio deja dos sólidos sueltos en una pieza: separarlos
    const d2 = await call("cad_get_document");
    d2.features.push({ id: d2.next_id, name: "Ranura", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 2, dy: 30, dz: 30, centered: true, centered_z: true }, origin: [-3, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
    d2.next_id += 1;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    // El corte parte la pieza en dos: cada pedazo queda como pieza, sin Separar
    ps = await parts();
    if (ps.length !== 2) throw new Error(`la ranura no separó las piezas: ${ps.length}`);
    // Borrar la segunda desde la lista de piezas
    const total = ps[0].volume + ps[1].volume;
    await tab(b, "Pieza");
    await b.eval(`document.querySelector('[aria-label="Borrar Pieza 2"]').click()`);
    await sleep(1500);
    await accept(b);
    ps = await parts();
    if (ps.length !== 1) throw new Error(`borrar: ${ps.length} piezas`);
    if (!(ps[0].volume < total)) throw new Error("no se borró");
    const kinds = (await call("cad_get_document")).features.map((f) => f.kind.type);
    if (kinds.join(",") !== "primitive,primitive,boolean,primitive,delete_parts") throw new Error(`historial: ${kinds}`);
  },

  async "material por pieza y masa total"(b) {
    // Sin raíz: la sección Sólido (la última con ese rótulo; la de la pieza va antes)
    const line = (label, root = "document") =>
      b.eval(`(() => { const e = [...${root}.querySelectorAll("span")].filter((x) => x.textContent === ${JSON.stringify(label)}).at(-1); return e?.nextElementSibling?.textContent.trim(); })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    // Cubo de 10 aparte: 1 cm³
    doc.features.push({ id: box.id + 1, name: "Cubo", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 10, dy: 10, dz: 10, centered: true, centered_z: true }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = box.id + 2;
    doc.material = { name: "PLA", density: 1240 };
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    // 8 cm³ + 1 cm³ de PLA
    await tab(b, "Inspección");
    if ((await line("Masa")) !== "11,16 g") throw new Error(`masa con PLA: ${await line("Masa")}`);
    // La segunda de acero: 8 × 1,24 + 1 × 7,85
    await tab(b, "Pieza");
    await b.eval(`[...document.querySelectorAll("[data-part] span.truncate")].find((s) => s.textContent === "Pieza 2").click()`);
    await sleep(800);
    const panel = `document.querySelector('[aria-label="Datos de Pieza 2"]')`;
    const trigger = await b.eval(`(() => { const e = [...${panel}.querySelectorAll("button")].find((x) => x.textContent.includes("El del diseño")); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.click(...trigger, { wait: 600 });
    await b.clickText("Acero (7850 kg/m³)");
    await sleep(1500);
    if ((await line("Masa", panel)) !== "7,85 g (Acero)") throw new Error(`masa de la pieza: ${await line("Masa", panel)}`);
    await tab(b, "Inspección");
    if ((await line("Masa")) !== "17,77 g") throw new Error(`masa total: ${await line("Masa")}`);
    // Con densidades distintas no hay una inercia del cuerpo
    if ((await line("Inercia")) !== undefined) throw new Error("mostró la inercia con materiales distintos");
    // El centro de masa se corre hacia el cubo de acero: 40 × 7,85 / 17,77
    const c = await line("Centro de masa");
    if (!c?.startsWith("17,67")) throw new Error(`centro: ${c}`);
  },

  async "ensamble: instancias, bisagra, grados libres y choques"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un cilindro como pieza aparte
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id, name: "Eje", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 30 }, origin: [60, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    // El ensamble va en su pestaña: «+» → Nuevo ensamble
    await b.eval(`document.querySelector("[data-tab-add]").click()`);
    await sleep(300);
    await b.clickText("Nuevo ensamble");
    await sleep(1500);
    if (!(await b.eval(`window.__cadUi.assemblyMode()`))) throw new Error("la pestaña de ensamble no está en modo ensamble");
    // Insertar las dos piezas (la primera queda fija)
    await b.clickText("Insertar");
    await sleep(1500);
    await b.eval(`(() => { const parts = window.__cadStore.result().parts; window.__p2 = parts[1].id; })()`);
    await b.eval(`window.__cadStore.editAssembly((a) => { a.instances.push({ id: a.next_id, part: window.__p2, name: "Eje", position: [40, 0, 0], rotation: [0, 0, 0], fixed: false }); a.next_id += 1; })`);
    await sleep(2000);
    if ((await b.eval(`document.querySelectorAll("[data-instance]").length`)) !== 2) throw new Error("no hay dos instancias");
    // Conectores: la tapa de arriba de la caja y el costado del cilindro
    const faces = await call("cad_assembly_faces");
    const conn = async (inst, test) => {
      const [, a, z] = faces.find((f) => f[0] === inst);
      for (let k = a; k < z; k++) {
        const c = await call("cad_assembly_connector", { face: k });
        if (test(c)) return c;
      }
      throw new Error(`sin conector en la instancia ${inst}`);
    };
    const asm = (await call("cad_get_document")).assembly;
    const [i1, i2] = asm.instances.map((i) => i.id);
    const top = await conn(i1, (c) => c.z[2] > 0.99 && Math.abs(c.origin[2] - 10) < 1e-6);
    const side = await conn(i2, (c) => Math.abs(c.z[2]) > 0.99 && Math.abs(c.origin[0] - 60) < 1e-6 && Math.abs(c.origin[2] - 15) < 1e-6);
    await b.eval(`window.__cadStore.editAssembly((a) => { a.mates.push({ id: a.next_id, name: "Bisagra 1", kind: "revolute", a: ${JSON.stringify(top)}, b: ${JSON.stringify(side)}, flip: false }); a.next_id += 1; })`);
    await sleep(2500);
    let sol = (await evaluate()).assembly.solution;
    if (!sol.converged || sol.dof !== 1) throw new Error(`bisagra: ${JSON.stringify(sol)}`);
    const status = await b.eval(`document.querySelector('[aria-label="Estado del ensamble"]').textContent`);
    if (!status.includes("1 grado libre")) throw new Error(`estado: ${status}`);
    // El eje queda clavado en el centro de la tapa: se pisa con la caja
    await b.clickText("Revisar choques");
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-hit]")`)); t++) await sleep(500);
    if (!(await b.eval(`!!document.querySelector("[data-hit]")`))) throw new Error("no detectó el choque");
    await b.shot("ensamble");
    // Lista de materiales: las dos piezas, una de cada una
    const bom = await b.eval(`[...document.querySelectorAll("[data-bom-row]")].map((r) => r.children[0].textContent + "×" + r.children[1].textContent)`);
    if (bom.length !== 2 || !bom.every((r) => r.endsWith("×1"))) throw new Error(`lista: ${bom}`);
    // Con el ángulo impuesto ya no queda nada libre
    await b.eval(`window.__cadStore.editAssembly((a) => { a.mates[0].angle = 45; })`);
    await sleep(2000);
    sol = (await evaluate()).assembly.solution;
    if (sol.dof !== 0) throw new Error(`con ángulo: ${JSON.stringify(sol)}`);
    // Exportar el ensamble (desde su pestaña): cada instancia en su lugar, con su nombre
    const names = (await call("cad_get_document")).assembly.instances.map((i) => i.name);
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Ensamble (2 instancias)");
    await sleep(400);
    for (const [fmt, label] of [["step", "Exportar STEP"], ["3mf", "Exportar 3MF"]]) {
      await b.clickText(fmt.toUpperCase());
      const path = `${out}/ensamble.${fmt}`;
      rmSync(path, { force: true });
      await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
      await b.clickText(label);
      await sleep(2500);
      if (!existsSync(path)) throw new Error(`no se exportó ${path}`);
      const { execSync } = await import("node:child_process");
      const text = fmt === "step" ? readFileSync(path, "latin1") : execSync(`unzip -p ${path} 3D/3dmodel.model`).toString();
      if (!names.every((n) => text.includes(n))) throw new Error(`${fmt} sin los nombres ${names}`);
      if (fmt === "3mf" && (text.match(/<object /g) ?? []).length !== 2) throw new Error("3MF sin un objeto por instancia");
    }
    // Volver al diseño (su pestaña): sin ensamble adentro
    await b.eval(`document.querySelector('[data-tab="0"]').click()`);
    await sleep(1500);
    if (await b.eval(`window.__cadUi.assemblyMode()`)) throw new Error("no volvió al diseño");
    if ((await call("cad_get_document")).assembly) throw new Error("el diseño quedó con ensamble adentro");
  },

  async "ensamble: animar una unión en GLB y vista explosionada"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const A = doc.next_id++;
    const B = doc.next_id++;
    doc.features.push({ id: A, name: "Caja", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 20, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.features.push({ id: B, name: "Eje", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 30 }, origin: [60, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.assembly = {
      instances: [
        { id: 1, part: { feature: A, index: 0 }, name: "Caja", position: [0, 0, 0], rotation: [0, 0, 0], fixed: true },
        { id: 2, part: { feature: B, index: 0 }, name: "Eje", position: [40, 0, 0], rotation: [0, 0, 0], fixed: false },
      ],
      mates: [],
      next_id: 3,
    };
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const faces = await call("cad_assembly_faces");
    const conn = async (inst, test) => {
      const [, a, z] = faces.find((f) => f[0] === inst);
      for (let k = a; k < z; k++) {
        const c = await call("cad_assembly_connector", { face: k });
        if (test(c)) return c;
      }
      throw new Error(`sin conector en la instancia ${inst}`);
    };
    const top = await conn(1, (c) => c.z[2] > 0.99 && Math.abs(c.origin[2] - 10) < 1e-6);
    const side = await conn(2, (c) => Math.abs(c.z[2]) > 0.99 && Math.abs(c.origin[0] - 60) < 1e-6 && Math.abs(c.origin[2] - 15) < 1e-6);
    const d2 = await call("cad_get_document");
    d2.assembly.mates.push({ id: 3, name: "Bisagra 1", kind: "revolute", a: top, b: side, flip: false });
    d2.assembly.next_id = 4;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    // GLB animado: 12 cuadros de 0 a 90°; dos nodos con traslación y giro cada uno
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/ensamble_animado.glb`;
    rmSync(path, { force: true });
    await call("cad_export_assembly_animation", { path, mate: 3, var: "angle", from: 0, to: 90, frames: 12, fps: 24 });
    if (!existsSync(path)) throw new Error("no se exportó el GLB animado");
    const buf = readFileSync(path);
    const jsonLen = buf.readUInt32LE(12);
    const gltf = JSON.parse(buf.subarray(20, 20 + jsonLen).toString("utf8"));
    if (gltf.animations?.length !== 1) throw new Error(`animaciones: ${gltf.animations?.length}`);
    const ch = gltf.animations[0].channels;
    if (ch.length !== 4) throw new Error(`canales: ${ch.length}`);
    const paths = ch.map((c) => c.target.path).sort().join(",");
    if (paths !== "rotation,rotation,translation,translation") throw new Error(`rutas: ${paths}`);
    // 12 cuadros por canal, a 24 cuadros por segundo
    const sampler = gltf.animations[0].samplers[ch[0].sampler];
    if (gltf.accessors[sampler.input].count !== 12) throw new Error(`cuadros: ${gltf.accessors[sampler.input].count}`);
    // Una unión que no existe da error (el puente devuelve el texto del error)
    const bad = await fetch(BRIDGE + "cad_export_assembly_animation", { method: "POST", body: JSON.stringify({ path, mate: 999, var: "angle", from: 0, to: 1, frames: 3 }) });
    if (bad.ok) throw new Error("animó una unión que no existe");
    // Vista explosionada: otra malla con las mismas caras
    const mesh = async (explode) => Buffer.from(await (await fetch(BRIDGE + "cad_assembly_mesh", { method: "POST", body: JSON.stringify({ explode }) })).arrayBuffer());
    const [flat, blown] = [await mesh(0), await mesh(1)];
    if (flat.length !== blown.length) throw new Error("la vista explosionada cambió la cantidad de datos");
    if (flat.equals(blown)) throw new Error("la vista explosionada no cambia la malla");
  },

  async "caja centrada y exportar el diseño"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    let r = await body();
    near(r.bbox_min[0], -10, 1e-6, "caja centrada en X");
    near(r.bbox_min[1], -10, 1e-6, "caja centrada en Y");
    near(r.bbox_min[2], -10, 1e-6, "centrada en Z: el centro en el origen");
    await accept(b);
    // Exportar del encabezado: con un diseño y sin modelo, exporta el sólido
    await b.clickContains("Exportar");
    await sleep(800);
    if (!(await b.eval(`document.body.innerText.includes("Exportar diseño")`))) throw new Error("no abrió la exportación del diseño");
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    for (const [fmt, label] of [["step", "Exportar STEP"], ["stl", "Exportar STL"]]) {
      await b.clickText(fmt.toUpperCase());
      const path = `${out}/caja.${fmt}`;
      rmSync(path, { force: true });
      await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
      await b.clickText(label);
      await sleep(2000);
      if (!existsSync(path)) throw new Error(`no se escribió ${path}`);
      const head = readFileSync(path).subarray(0, 80).toString("latin1");
      if (fmt === "step" && !head.startsWith("ISO-10303-21")) throw new Error(`STEP raro: ${head}`);
      if (fmt === "stl" && statSync(path).size !== 84 + 12 * 50) throw new Error(`STL de ${statSync(path).size} bytes`);
    }
  },

  async "configuraciones: variante, valores y exportar todas"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Ancho por parámetro y una variante «Chica»
    const doc = await call("cad_get_document");
    doc.parameters = [{ name: "ancho", expr: "20" }];
    doc.bindings = { [`${doc.features[0].id}.kind.shape.dx`]: "ancho" };
    doc.configurations = [{ name: "Chica", values: { ancho: "10" }, suppressed: [] }];
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    near((await body()).volume, 8000, 1e-6, "base");
    await b.eval(`document.querySelector('[aria-label="Usar Chica"]').click()`);
    await sleep(2000);
    near((await body()).volume, 4000, 1e-6, "variante chica");
    // Cambiar su valor desde el panel
    await b.eval(`(() => { const i = document.querySelector('[aria-label="ancho en Chica"]'); i.value = "5"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(2000);
    near((await body()).volume, 2000, 1e-6, "variante con 5");
    if ((await call("cad_get_document")).parameters[0].expr !== "20") throw new Error("cambió lo de base");
    await b.shot("configuraciones");
    // Exportar todas: un STL por variante
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Todas las configuraciones (2 archivos)");
    await sleep(400);
    await b.clickText("STL");
    for (const f of ["caja-base.stl", "caja-Chica.stl"]) rmSync(`${out}/${f}`, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(`${out}/caja.stl`)}`);
    await b.clickText("Exportar STL");
    await sleep(5000);
    for (const f of ["caja-base.stl", "caja-Chica.stl"]) if (!existsSync(`${out}/${f}`)) throw new Error(`no se exportó ${f}`);
    // Vuelve a la variante que estaba
    if ((await call("cad_get_document")).active_configuration !== 0) throw new Error("no volvió a la variante elegida");
  },

  async "versiones: guardar, ver, comparar y restaurar"(b) {
    const vol = async () => (await body()).volume;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    near(await vol(), 8000, 1e-6, "caja");
    // Guardar «v1» desde el panel
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nota de la versión"]'); i.value = "enviada a imprimir"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
    await b.clickText("Guardar versión");
    await sleep(1000);
    const saved = (await call("cad_get_document")).versions;
    if (saved?.length !== 1 || saved[0].name !== "v1" || saved[0].note !== "enviada a imprimir") throw new Error(`versiones: ${JSON.stringify(saved?.map((v) => [v.name, v.note]))}`);
    if (saved[0].document.versions) throw new Error("la versión guarda versiones adentro");
    // Más ancha: 30 × 20 × 20
    await b.eval(`window.__cadStore.commit((d) => (d.features[0].kind.shape.dx = 30))`);
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "más ancha");
    // Ver v1 en solo lectura: el visor muestra la vieja y no se puede editar
    await b.eval(`document.querySelector('[aria-label="Ver v1"]').click()`);
    await sleep(1500);
    near(await vol(), 8000, 1e-6, "viendo v1");
    if (!(await b.eval(`!!document.querySelector("[data-viewing]")`))) throw new Error("sin aviso de solo lectura");
    await b.eval(`window.__cadStore.commit((d) => (d.features[0].kind.shape.dx = 50))`);
    await sleep(500);
    if ((await call("cad_get_document")).features[0].kind.shape.dx !== 30) throw new Error("se pudo editar viendo una versión");
    await b.shot("version_viendo");
    await b.clickText("Volver al diseño");
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "de vuelta");
    // Comparar: se agregaron 4000 mm³ y no se quitó nada
    await b.eval(`document.querySelector('[aria-label="Comparar con v1"]').click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadStore.comparison()`)); t++) await sleep(250);
    const c = await b.eval(`(() => { const c = window.__cadStore.comparison(); return { added: c.added, removed: c.removed, mesh: !!c.addedMesh, none: !c.removedMesh }; })()`);
    near(c.added, 4000, 1e-6, "agregado");
    near(c.removed, 0, 1e-6, "quitado");
    if (!c.mesh || !c.none) throw new Error(`mallas: ${JSON.stringify(c)}`);
    await b.shot("version_comparar");
    // Restaurar: vuelve a 20 de ancho, la versión sigue y se puede deshacer
    await b.eval(`document.querySelector('[aria-label="Restaurar v1"]').click()`);
    await sleep(1500);
    near(await vol(), 8000, 1e-6, "restaurada");
    if ((await call("cad_get_document")).versions?.length !== 1) throw new Error("restaurar borró las versiones");
    await b.eval(`window.__cadStore.undo()`);
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "deshacer restaurar");
  },

  async "piezas estándar: tornillo en un agujero y tuerca roscada"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    const last = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1)))`);
    await begin(b);
    // Placa de 40×40×10 con un agujero pasante de 6,6 (M6)
    const doc = await call("cad_get_document");
    const prim = (shape, origin, op) => ({ id: doc.next_id++, name: shape.type, suppressed: false, kind: { type: "primitive", shape, origin, z: [0, 0, 1], x: [1, 0, 0], op } });
    doc.features.push(prim({ type: "box", dx: 40, dy: 40, dz: 10, centered: true }, [0, 0, 0], "join"));
    doc.features.push(prim({ type: "cylinder", radius: 3.3, height: 20 }, [0, 0, -5], "cut"));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    // Borde de arriba del agujero → Tornillo: M6, apoyado en la cara, largo para placa + tuerca
    await filter("Aristas");
    await b.click(...(await at([-3.3, 0, 10])), { wait: 800 });
    const picked = await b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
    if (picked.join() !== "edge") throw new Error(`elegido: ${picked}`);
    await b.clickText("Tornillo");
    for (let t = 0; t < 20 && (await last()).kind.shape.type !== "bolt"; t++) await sleep(250);
    await sleep(1500);
    const bolt = (await last()).kind;
    if (bolt.shape.size !== "M6" || bolt.shape.length !== 20 || bolt.op !== "new") throw new Error(`tornillo: ${JSON.stringify(bolt)}`);
    near(bolt.origin[2], 10, 1e-6, "apoyo");
    near(bolt.z[2], 1, 1e-6, "eje");
    await accept(b, 2000);
    let parts = (await evaluate()).parts;
    if (parts.length !== 2 || parts[1].name !== "Tornillo Allen M6×20") throw new Error(`piezas: ${parts.map((p) => p.name)}`);
    await b.shot("tornillo_en_agujero");
    // Tuerca suelta, y con rosca modelada pierde el filete
    await filter("Todo");
    await b.clickText("Tuerca");
    for (let t = 0; t < 20 && (await last()).kind.shape.type !== "nut"; t++) await sleep(250);
    await sleep(1500);
    await b.eval(`window.__cadStore.commit((d) => (d.features.at(-1).kind.origin = [0, 0, -12]))`);
    await sleep(1500);
    const plain = (await evaluate()).parts[2].volume;
    await b.clickText("Rosca modelada");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__cadStore.doc().features.at(-1).kind.shape.modeled && !window.__cadStore.busy()`)); t++) await sleep(250);
    await sleep(500);
    await accept(b, 3000);
    parts = (await evaluate()).parts;
    if (parts[2]?.name !== "Tuerca M6") throw new Error(`tuerca: ${parts.map((p) => p.name)}`);
    // Lo que queda entre el agujero liso de 6 y el macho M6: unos 5,3 mm³ por mm de alto (≈ 27,6)
    if (!(parts[2].volume > plain + 22 && parts[2].volume < plain + 33)) throw new Error(`tuerca roscada ${parts[2].volume} vs lisa ${plain}`);
    await b.shot("tuerca_roscada");
  },

  async "roscas coordinadas: tornillo en agujero roscado y rosca alrededor de un tornillo"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const last = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1)))`);
    const idle = async (cond) => {
      for (let t = 0; t < 60 && !(await b.eval(`(${cond}) && !window.__cadStore.busy()`)); t++) await sleep(500);
      await sleep(800);
    };
    const question = () => b.eval(`document.querySelector("[data-thread-question]")?.textContent ?? ""`);
    await begin(b);
    // 1) Placa con un agujero M6 roscado (modelado, holgura 0,2) y después un tornillo
    const doc = await call("cad_get_document");
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }], entities: [{ id: 2, geometry: { type: "point", point: 1 } }], constraints: [], next_id: 3, origin: 0 };
    const add = (name, kind) => {
      const id = doc.next_id++;
      doc.features.push({ id, name, suppressed: false, kind });
      return id;
    };
    add("Placa", { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "join" });
    const sk = add("Centro", { type: "sketch", plane: { type: "xy" }, offset: 10, sketch });
    const hole = add("Agujero roscado", {
      type: "hole", sketch: sk, points: [], diameter: 5, depth: { type: "through_all" }, style: { type: "simple" }, tip_angle: 0, thread: "M6",
      modeled: { nominal: 6, pitch: 1, clearance: 0.2, left: false },
    });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2500);
    await b.eval(`window.__cadViewer.frameAll()`);
    // Tornillo M8 suelto en el origen: pregunta si va en la rosca del agujero (está en su eje)
    await b.clickText("Tornillo");
    await idle(`window.__cadStore.doc().features.at(-1).kind.shape?.type === "bolt"`);
    const q = await question();
    if (!q.includes("Agujero roscado")) throw new Error(`pregunta: ${q}`);
    await b.shot("pregunta_rosca");
    await b.clickText("Coordinar");
    await idle(`!!window.__cadStore.doc().features.at(-1).kind.link`);
    const bolt = (await last()).kind;
    if (bolt.link.feature !== hole || bolt.shape.size !== "M6" || bolt.shape.length !== 10 || !bolt.shape.modeled) throw new Error(`tornillo: ${JSON.stringify(bolt)}`);
    await accept(b, 4000);
    let r = await evaluate();
    const errs = r.status.filter((s) => s.state === "error");
    if (errs.length) throw new Error(`errores: ${JSON.stringify(errs)}`);
    const placed = r.threads.find((t) => !t.internal).placed;
    near(placed[0][2], 10, 1e-6, "cabeza en la boca");
    near(Math.hypot(placed[0][0], placed[0][1]), 0, 1e-6, "en el eje");
    if (r.parts.length !== 2) throw new Error(`piezas: ${r.parts.length}`);
    await b.shot("tornillo_coordinado");

    // 2) Al revés: el tornillo primero, una placa con un agujero liso alrededor y
    //    la rosca del agujero toma la del tornillo (elegido con un clic)
    const doc2 = await call("cad_get_document");
    doc2.features = [];
    const add2 = (name, kind, scope) => {
      const id = doc2.next_id++;
      doc2.features.push({ id, name, suppressed: false, kind, ...(scope ? { scope } : {}) });
      return id;
    };
    add2("Perno", { type: "primitive", shape: { type: "bolt", size: "M6", length: 12, head: "socket", modeled: true }, origin: [0, 0, 10.5], z: [0, 0, 1], x: [0.3, 0.8, 0], op: "new" });
    const plate = add2("Bloque", { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" });
    const sk2 = add2("Centro", { type: "sketch", plane: { type: "xy" }, offset: 10, sketch });
    const d1 = 6 - 1.082532;
    add2("Agujero liso", { type: "hole", sketch: sk2, points: [], diameter: d1, depth: { type: "through_all" }, style: { type: "simple" }, tip_angle: 0, thread: null }, [{ feature: plate, index: 0 }]);
    await call("cad_set_document", { document: doc2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2500);
    await b.eval(`window.__cadViewer.frameAll()`);
    // La rosca en la pared del agujero (paso 1,5 a propósito: el del tornillo manda)
    await b.eval(`window.__cadStore.addFeature({ type: "thread", face: { point: [${d1 / 2}, 0, 5], normal: [-1, 0, 0], tags: [] }, pitch: 1.5, length: 0, flip: false, left: false, clearance: 0.2 })`);
    await idle(`window.__cadStore.doc().features.at(-1).kind.type === "thread"`);
    for (let t = 0; t < 20 && !(await question()); t++) await sleep(500);
    const q2 = await question();
    if (!q2.includes("Perno")) throw new Error(`pregunta de la rosca: ${q2}`);
    await b.clickText("Elegir en el visor");
    await sleep(1500);
    // Clic en la cabeza del tornillo (su costado, arriba de la placa)
    await b.click(...(await at([0, -5, 14])), { wait: 1500 });
    await idle(`!!window.__cadStore.doc().features.at(-1).kind.link`);
    const th = (await last()).kind;
    if (th.link?.feature !== doc2.features[0].id || th.pitch !== 1) throw new Error(`rosca: ${JSON.stringify(th)}`);
    await accept(b, 5000);
    r = await evaluate();
    const errs2 = r.status.filter((s) => s.state === "error");
    if (errs2.length) throw new Error(`errores: ${JSON.stringify(errs2)}`);
    const it = r.threads.find((t) => t.internal);
    if (!it || it.spec.pitch !== 1 || it.spec.nominal !== 6) throw new Error(`rosca calculada: ${JSON.stringify(it)}`);
    await b.shot("rosca_coordinada");
  },

  async "parámetros y fórmulas"(b) {
    await begin(b);
    // La caja queda con su diálogo abierto: parámetros y campos van al borrador
    await b.clickText("Caja");
    await sleep(1500);
    // Parámetro nuevo (p1 = 10) renombrado a "ancho" y puesto en 30
    await b.click(...(await b.eval(`(() => { const e = document.querySelector('[aria-label="Agregar parámetro"]'); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`)), { wait: 1200 });
    const setParam = (nth, value) =>
      b.eval(`(() => {
        // La fila del parámetro (nombre = expresión), no el campo de la caja con la fórmula
        const row = [...document.querySelectorAll("input")].filter((i) => (i.value === "p1" || i.value === "ancho") && i.nextElementSibling?.textContent === "=")[0].parentElement;
        const i = row.querySelectorAll("input")[${nth}];
        i.value = ${JSON.stringify(value)};
        i.dispatchEvent(new Event("change", { bubbles: true }));
      })()`);
    await setParam(0, "ancho");
    await sleep(1200);
    await setParam(1, "30");
    await sleep(1200);
    // Ancho de la caja = ancho, alto = ancho / 3 (la caja quedó elegida)
    await setInput(b, "Ancho (X)", "ancho");
    await sleep(1500);
    await setInput(b, "Alto (Z)", "ancho / 3");
    await sleep(1500);
    let r = await body();
    near(r.bbox_max[0] - r.bbox_min[0], 30, 1e-6, "ancho = 30");
    near(r.bbox_max[2] - r.bbox_min[2], 10, 1e-6, "alto = ancho / 3");
    const fx = await b.eval(`[...document.querySelectorAll("span")].filter((s) => s.textContent === "fx").length`);
    if (fx < 2) throw new Error("faltan las marcas fx");
    await setParam(1, "45");
    await sleep(2000);
    r = await body();
    near(r.bbox_max[0] - r.bbox_min[0], 45, 1e-6, "ancho = 45");
    near(r.bbox_max[2] - r.bbox_min[2], 15, 1e-6, "alto sigue la fórmula");
    // Fórmula inválida: queda marcada y no cambia nada
    await setInput(b, "Alto (Z)", "ancho * ");
    await sleep(800);
    const bad = await b.eval(`!!document.querySelector("input.border-error")`);
    if (!bad) throw new Error("la fórmula mala no se marcó");
    // Cota del sketch con fórmula desde el visor
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    // La cota del ancho se pide sola: fórmula + Enter; el alto, Escape (queda lo dibujado)
    await b.send("Input.insertText", { text: "ancho * 2" });
    await b.key("Enter", "Enter", 13);
    await sleep(600);
    await b.key("Escape", "Escape", 27);
    await sleep(1200);
    const label = await b.eval(`[...document.querySelectorAll("button.font-mono")].map((x) => x.textContent).find((t) => t.startsWith("ancho"))`);
    if (!label?.startsWith("ancho * 2 = 90")) throw new Error(`etiqueta: ${label}`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const doc = await call("cad_get_document");
    const key = Object.keys(doc.bindings).find((k) => k.includes("constraints"));
    if (!key || doc.bindings[key] !== "ancho * 2") throw new Error("la cota no quedó vinculada");
  },
};

siEsPrincipal(import.meta.url, escenarios);
