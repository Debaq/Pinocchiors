// Planos 2D: vistas, cortes, cotas y exportación.
//   node apps/web/e2e/cad/plano2d.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo plano2d              (lo mismo)

import { existsSync, readFileSync, rmSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, accept } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
  async "plano 2D con vistas, ocultas y exportar SVG"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Agujero pasante vertical: en el frente y el lateral aparece como líneas ocultas
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id, name: "Agujero", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 3, height: 40 }, origin: [0, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    const info = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      const views = [...svg.querySelectorAll("[data-view]")].map((g) => g.dataset.view);
      const hidden = svg.querySelectorAll('[data-view="front"] [data-kind="hidden"], [data-view="front"] [data-kind="hidden_outline"]').length;
      return { views, hidden, text: svg.textContent };
    })()`);
    if (info.views.join(",") !== "front,top,left,iso") throw new Error(`vistas: ${info.views}`);
    if (info.hidden !== 2) throw new Error(`ocultas del frente: ${info.hidden}`);
    // 20 mm en A4: escala 2:1 (o mayor)
    if (!/[25]:1/.test(info.text)) throw new Error(`escala: ${info.text}`);
    await b.shot("plano_2d");
    // Exportar: el mismo SVG a un archivo
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/plano.svg`;
    rmSync(path, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.clickText("Exportar SVG");
    await sleep(1500);
    if (!existsSync(path) || !readFileSync(path, "utf8").startsWith("<svg")) throw new Error("no se exportó el SVG");
    // Cotas a mano: el diámetro del agujero en la planta y el largo de una arista del frente
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
    await sleep(300);
    const at = (sel, pick) =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const lines = [...svg.querySelectorAll(${JSON.stringify(sel)})].map((l) => l.getAttribute("points").split(" ").map((p) => p.split(",").map(Number)));
        const pts = (${pick})(lines);
        const m = svg.getScreenCTM();
        const p = new DOMPoint(pts[0], pts[1]).matrixTransform(m);
        return [p.x, p.y];
      })()`);
    // El círculo más largo de la planta (muchos puntos)
    await b.click(...(await at('[data-view="top"] polyline[data-kind="visible"]', "(ls) => ls.sort((a, b) => b.length - a.length)[0][0]")), { wait: 1000 });
    // Una recta del frente: el medio de su primer tramo
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', "(ls) => { const l = ls.find((x) => x.length === 2); return [(l[0][0] + l[1][0]) / 2, (l[0][1] + l[1][1]) / 2]; }")), { wait: 1000 });
    const texts = await b.eval(`[...document.querySelectorAll("[data-sheet] [data-user-dim] text")].map((t) => t.textContent)`);
    if (!texts.includes("Ø6") || !texts.includes("20")) throw new Error(`cotas a mano: ${texts}`);
    if ((await call("cad_get_document")).drawing?.sheets?.[0]?.dims?.length !== 2) throw new Error("no se guardaron las cotas");
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
    // Corte A-A: el frente rayado y la línea en la planta
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-hatch="front"] polygon')`)); t++) await sleep(500);
    const cut = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      return { tris: svg.querySelectorAll('[data-hatch="front"] polygon').length, label: !!svg.querySelector("[data-section-label]"), line: !!svg.querySelector("[data-section-line]") };
    })()`);
    if (!(cut.tris > 0) || !cut.label || !cut.line) throw new Error(`corte: ${JSON.stringify(cut)}`);
    await b.shot("plano_corte");
    // PDF del mismo tamaño que la hoja (A4: 841,89 × 595,28 pt)
    const pdf = `${out}/plano.pdf`;
    rmSync(pdf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(pdf)}`);
    await b.clickText("Exportar PDF");
    await sleep(3000);
    if (!existsSync(pdf) || !readFileSync(pdf).subarray(0, 5).toString() === "%PDF-") throw new Error("no se exportó el PDF");
    const { execSync } = await import("node:child_process");
    try {
      const info = execSync(`pdfinfo ${pdf}`).toString();
      if (!/Page size:\s+841\.\d+ x 595\.\d+/.test(info)) throw new Error(`tamaño del PDF: ${info}`);
    } catch (e) {
      if (String(e).includes("tamaño")) throw e;
    }
    // DXF de la hoja (capas VISIBLE y OCULTA)
    const dxf = `${out}/plano.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    const text = existsSync(dxf) ? readFileSync(dxf, "utf8") : "";
    if (!text.includes("OCULTA") || !text.trimEnd().endsWith("EOF")) throw new Error("DXF raro");
    // El «Cerrar» del plano (hay otros en la página)
    await b.eval(`[...document.querySelectorAll('div[aria-label="Plano 2D"] button')].find((x) => x.textContent.trim() === "Cerrar").click()`);
    await sleep(500);
    if (await b.eval(`!!document.querySelector('div[aria-label="Plano 2D"]')`)) throw new Error("no se cerró");
  },

  async "plano 2D: dibujar sobre la hoja con el editor del sketch"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    const sheet = await b.eval(`(() => { const s = window.__cadUi.session(); return s && { ...s.sheet, open: !!document.querySelector('div[aria-label="Plano 2D"]') }; })()`);
    if (!sheet || sheet.width !== 297 || sheet.height !== 210 || !String(sheet.backdrop).startsWith("blob:")) throw new Error(`sesión de la hoja: ${JSON.stringify(sheet)}`);
    if (sheet.open) throw new Error("el plano sigue tapando el visor");
    if (await b.eval(`[...document.querySelectorAll("button")].some((x) => x.title?.startsWith("Usar arista") && x.offsetParent)`)) throw new Error("Usar arista no va sobre la hoja");
    await sleep(800);
    await b.shot("plano_dibujar");
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    // Una línea (mm de hoja) y un círculo
    await key("l");
    await b.click(...(await at([40, 40])), { wait: 400 });
    await b.click(...(await at([100, 40])), { wait: 400 });
    await key("Escape");
    await key("c");
    await b.click(...(await at([150, 60])), { wait: 400 });
    await b.click(...(await at([160, 60])), { wait: 800 });
    await key("Escape");
    const kinds = await b.eval(`window.__cadUi.session().sketch.entities.map((e) => e.geometry.type).sort().join(",")`);
    if (kinds !== "circle,line") throw new Error(`dibujado: ${kinds}`);
    const ends = await b.eval(`(() => { const s = window.__cadUi.session().sketch; const l = s.entities.find((e) => e.geometry.type === "line").geometry; return [l.start, l.end].map((id) => { const p = s.points.find((q) => q.id === id); return [p.x, p.y]; }); })()`);
    near(Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]), 60, 1.5, "largo de la línea");
    // De vuelta al plano: lo dibujado está en la hoja y en el documento
    await b.clickText("Volver al plano");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-layer]")`)); t++) await sleep(250);
    const layer = await b.eval(`document.querySelectorAll("[data-sheet] svg [data-layer] polyline").length`);
    if (layer !== 2) throw new Error(`en la hoja: ${layer}`);
    const saved = (await call("cad_get_document")).drawing?.sheets?.[0]?.sketch;
    if (saved?.entities?.length !== 2) throw new Error(`guardado: ${JSON.stringify(saved?.entities)}`);
    await b.shot("plano_con_dibujo");
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const dxf = `${out}/plano_dibujo.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    if (!(existsSync(dxf) && readFileSync(dxf, "utf8").includes("\nDIBUJO\n10\n"))) throw new Error("DXF sin lo dibujado");
    // Se vuelve a abrir con lo guardado; descartar no cambia nada y vuelve al plano
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    if ((await b.eval(`window.__cadUi.session().sketch.entities.length`)) !== 2) throw new Error("no se abrió lo guardado");
    await b.clickText("Descartar");
    await sleep(500);
    if (!(await b.eval(`!!document.querySelector('div[aria-label="Plano 2D"]') && !window.__cadUi.session()`))) throw new Error("descartar no volvió al plano");
  },

  async "plano 2D: medida al dibujar sobre la hoja (queda solo si se escribe o con Enter) y cotas donde se dejaron"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(800);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const asked = () => b.eval(`document.activeElement?.tagName === "INPUT"`);
    const lengths = () => b.eval(`window.__cadUi.session().sketch.constraints.filter((c) => c.type === "length").map((c) => c.value)`);
    const line = async (p, q) => {
      await key("l");
      await b.click(...(await at(p)), { wait: 400 });
      await b.click(...(await at(q)), { wait: 800 });
      if (!(await asked())) throw new Error("no se pidió la medida sobre la hoja");
    };
    // 1. Se escribe 50 + Enter: queda la cota y la línea mide 50
    await line([40, 40], [100, 40]);
    if ((await lengths()).length) throw new Error("la cota se agregó antes de escribirla");
    await b.send("Input.insertText", { text: "50" });
    await b.key("Enter", "Enter", 13);
    await sleep(1200);
    await key("Escape");
    if (JSON.stringify(await lengths()) !== "[50]") throw new Error(`con 50: ${JSON.stringify(await lengths())}`);
    // 2. Esc en el campo: no queda nada
    await line([40, 100], [100, 100]);
    await b.key("Escape", "Escape", 27);
    await sleep(600);
    await key("Escape");
    if ((await lengths()).length !== 1) throw new Error(`con Esc: ${JSON.stringify(await lengths())}`);
    // 3. Enter sin escribir: queda con lo dibujado
    await line([40, 150], [80, 150]);
    await b.key("Enter", "Enter", 13);
    await sleep(1200);
    await key("Escape");
    const ls = await lengths();
    if (ls.length !== 2) throw new Error(`con Enter: ${JSON.stringify(ls)}`);
    near(ls[1], 40, 1, "cota con lo dibujado");
    const ends = await b.eval(`(() => { const s = window.__cadUi.session().sketch; const l = s.entities.find((e) => e.geometry.type === "line").geometry; return [l.start, l.end].map((id) => { const p = s.points.find((q) => q.id === id); return [p.x, p.y]; }); })()`);
    near(Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]), 50, 1e-3, "la línea acotada");
    // 4. El texto de la cota de 50 arrastrado a mano: en la hoja sale ahí (y la línea de cota se alarga)
    await b.eval(`window.__cadUi.change((sk) => { const c = sk.constraints.find((c) => c.type === "length" && Math.abs(c.value - 50) < 1e-6); c.opts = { offset: [60, 15] }; })`);
    await sleep(800);
    await b.clickText("Volver al plano");
    const dimTexts = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-layer-dims] text")].map((t) => [t.textContent, +t.getAttribute("x"), +t.getAttribute("y")])`);
    for (let t = 0; t < 30 && (await dimTexts()).length < 2; t++) await sleep(250);
    const texts = await dimTexts();
    const fifty = texts.find((t) => t[0] === "50");
    if (!fifty || !texts.some((t) => t[0] === "40")) throw new Error(`cotas en la hoja: ${JSON.stringify(texts)}`);
    // Medio de la línea + (60, 15) en mm de hoja; en el SVG Y va hacia abajo (alto 210) y el renglón 1 mm arriba
    const mid = [(ends[0][0] + ends[1][0]) / 2, (ends[0][1] + ends[1][1]) / 2];
    near(fifty[1], mid[0] + 60, 0.5, "texto de la cota (x)");
    near(fifty[2], 210 - (mid[1] + 15) - 1, 0.5, "texto de la cota (y)");
    await b.shot("plano_medida_al_dibujar");
  },

  async "planos 2D en el Outliner: varios, elementos, ventana de propiedades"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    const ready = async () => {
      for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
      await sleep(300);
    };
    await ready();
    const row = (id) => `document.querySelector('[data-outliner-row=${JSON.stringify(id)}]')`;
    // Con el plano abierto, la barra izquierda es la del plano (no la de Diseñar)
    const side = () => b.eval(`({ cota: !!document.querySelector('button[aria-label="Cota"]'), extrude: [...document.querySelectorAll("button[aria-label]")].some((x) => x.getAttribute("aria-label").startsWith("Extrusión") && x.offsetParent) })`);
    const s1 = await side();
    if (!s1.cota || s1.extrude) throw new Error(`barra izquierda con el plano: ${JSON.stringify(s1)}`);
    if (!(await b.eval(`!!${row("group-drawings")} && !!${row("drawing-1")} && !!${row("drawing-1:view:iso")}`))) throw new Error("no está el grupo Planos 2D con el plano y sus vistas");
    // Otro plano con «+»: se abre con su hoja elegida
    await b.eval(`document.querySelector('[aria-label="Plano nuevo"]').click()`);
    for (let t = 0; t < 20 && (await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 2"; t++) await sleep(250);
    await ready();
    if (!(await b.eval(`!!document.querySelector('[data-drawing-props="sheet"]')`))) throw new Error("el plano nuevo no abrió su hoja");
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nombre del plano"]'); i.value = "Cortes"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    for (let t = 0; t < 20 && !(await b.eval(`${row("drawing-2")}?.textContent.includes("Cortes")`)); t++) await sleep(250);
    if (!(await b.eval(`${row("drawing-2")}?.textContent.includes("Cortes")`))) throw new Error("el nombre no llegó al Outliner");
    // El ojo de la isométrica la saca de la hoja
    await b.eval(`[...${row("drawing-2:view:iso")}.querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 30 && (await b.eval(`!!document.querySelector('[data-sheet] [data-view="iso"]')`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector('[data-sheet] [data-view="iso"]')`)) throw new Error("la isométrica sigue");
    // Una cota en el frente; clic en su texto la elige (ventana y fila del Outliner)
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
    await sleep(200);
    const mid = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      const l = [...svg.querySelectorAll('[data-view="front"] polyline[data-kind="visible"]')].map((x) => x.getAttribute("points").split(" ").map((p) => p.split(",").map(Number))).find((x) => x.length === 2);
      const p = new DOMPoint((l[0][0] + l[1][0]) / 2, (l[0][1] + l[1][1]) / 2).matrixTransform(svg.getScreenCTM());
      return [p.x, p.y];
    })()`);
    await b.click(...mid, { wait: 1000 });
    await b.key("Escape", "Escape", 27);
    const txt = await b.eval(`(() => { const t = document.querySelector('[data-sheet] [data-user-dim="0"] text'); if (!t) return null; const r = t.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    if (!txt) throw new Error("no se puso la cota");
    await b.click(...txt, { wait: 500 });
    const picked = await b.eval(`({ props: document.querySelector('[data-drawing-props]')?.dataset.drawingProps, value: document.querySelector("[data-dim-value]")?.textContent, row: ${row("drawing-2:dim:0")}?.className.includes("bg-accent/20") })`);
    if (picked.props !== "dim" || picked.value !== "20" || !picked.row) throw new Error(`cota elegida: ${JSON.stringify(picked)}`);
    // Supr la quita
    await b.key("Delete", "Delete", 46);
    for (let t = 0; t < 20 && (await b.eval(`!!${row("drawing-2:dim:0")}`)); t++) await sleep(250);
    if (await b.eval(`!!${row("drawing-2:dim:0")}`)) throw new Error("Supr no quitó la cota");
    // Clic en el otro plano del Outliner: se abre ese
    await b.eval(`${row("drawing-1")}.click()`);
    for (let t = 0; t < 20 && (await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 1"; t++) await sleep(250);
    if ((await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 1") throw new Error("no se abrió el Plano 1");
    await b.eval(`[...document.querySelectorAll('div[aria-label="Plano 2D"] button')].find((x) => x.textContent.trim() === "Cerrar").click()`);
    await sleep(500);
    const s2 = await side();
    if (s2.cota || !s2.extrude) throw new Error(`barra izquierda al cerrar el plano: ${JSON.stringify(s2)}`);
    const doc = (await call("cad_get_document")).drawing;
    if (doc?.sheets?.length !== 2 || !doc.sheets[1].hiddenViews.includes("iso") || doc.sheets[1].name !== "Cortes") throw new Error(`guardado: ${JSON.stringify(doc)}`);
    await b.shot("planos_outliner");
  },

  async "plano 2D: seis vistas en primer y tercer diedro"(b) {
    await begin(b);
    // Caja de 30 × 20 × 10 corrida: cada vista tiene otra forma
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    const shown = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-view]")].map((g) => g.dataset.view).join(",")`);
    if ((await shown()) !== "front,top,left,iso") throw new Error(`al empezar: ${await shown()}`);
    // Prender las que faltan con el ojo del Outliner
    for (const v of ["bottom", "right", "back"]) await b.eval(`[...document.querySelector('[data-outliner-row="drawing-1:view:${v}"]').querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 40 && (await shown()) !== "front,top,bottom,left,right,back,iso"; t++) await sleep(250);
    if ((await shown()) !== "front,top,bottom,left,right,back,iso") throw new Error(`las seis: ${await shown()}`);
    const boxes = () =>
      b.eval(`Object.fromEntries([...document.querySelectorAll("[data-sheet] svg [data-view]")].map((g) => { const r = g.getBBox(); return [g.dataset.view, { cx: r.x + r.width / 2, cy: r.y + r.height / 2 }]; }))`);
    const check = (v, first) => {
      const [up, down, l, r] = first ? ["bottom", "top", "right", "left"] : ["top", "bottom", "left", "right"];
      if (!(v[up].cy < v.front.cy && v[down].cy > v.front.cy)) throw new Error(`${first ? "1.er" : "3.er"} diedro arriba/abajo: ${JSON.stringify(v)}`);
      if (!(v[l].cx < v.front.cx && v[r].cx > v.front.cx && v.back.cx > v[r].cx)) throw new Error(`${first ? "1.er" : "3.er"} diedro costados: ${JSON.stringify(v)}`);
    };
    check(await boxes(), true);
    await b.shot("plano_seis_vistas_iso_e");
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].projection = "third"; })`);
    for (let t = 0; t < 40; t++) {
      const v = await boxes();
      if (v.top && v.front && v.top.cy < v.front.cy) break;
      await sleep(250);
    }
    await sleep(500);
    check(await boxes(), false);
    await b.shot("plano_seis_vistas_iso_a");
    // Escala propia de la posterior: rótulo debajo y la fila del Outliner lo dice
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].viewScales = { back: 2 }; })`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-view-label="back"]')`)); t++) await sleep(250);
    const label = await b.eval(`document.querySelector('[data-sheet] [data-view-label="back"]')?.textContent`);
    if (label !== "Posterior (2:1)") throw new Error(`rótulo: ${label}`);
    if (!(await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:back"]')?.textContent.includes("(2:1)")`))) throw new Error("el Outliner no muestra la escala");
    await b.shot("plano_escala_por_vista");
  },

  async "plano 2D: las vistas como referencia del dibujo (anclar, usar, recortar, cotas que siguen al modelo)"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    // Escala fija 1:1: las medidas en la hoja son las de la pieza
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 1; })`);
    await sleep(1500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(800);
    // Esquinas del frente en mm de hoja (Y hacia arriba)
    const front = await b.eval(`(() => {
      const sh = window.__cadUi.session().sheet;
      const v = sh.views.find((x) => x.name === "front");
      const pts = v.lines.flatMap((l) => l.points);
      const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
      const S = (p) => [v.x + p[0] * v.scale, sh.height - v.y + p[1] * v.scale];
      return { lo: S([Math.min(...xs), Math.min(...ys)]), hi: S([Math.max(...xs), Math.max(...ys)]) };
    })()`);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    // 1. Anclar: una línea que empieza cerca de la esquina de arriba a la derecha del frente queda justo en ella
    await key("l");
    await b.click(...(await at([front.hi[0] + 0.4, front.hi[1] + 0.4])), { wait: 400 });
    await b.click(...(await at([front.hi[0] + 30, front.hi[1] + 25])), { wait: 400 });
    await key("Escape");
    await key("Escape");
    let s = await sk();
    const first = s.entities.find((e) => e.geometry.type === "line");
    const p0 = pos(s, first.geometry.start);
    near(p0[0], front.hi[0], 1e-6, "anclado a la esquina (x)");
    near(p0[1], front.hi[1], 1e-6, "anclado a la esquina (y)");
    // 2. Usar el borde de abajo del frente: queda ligado
    await key("j");
    await b.click(...(await at([(front.lo[0] + front.hi[0]) / 2, front.lo[1]])), { wait: 600 });
    s = await sk();
    if ((s.uses ?? []).length !== 1) throw new Error(`usar: ${JSON.stringify(s.uses)}`);
    const used = s.entities.find((e) => e.id === s.uses[0].entity);
    near(Math.abs(pos(s, used.geometry.end)[0] - pos(s, used.geometry.start)[0]), 20, 1e-6, "el borde usado mide 20");
    if ((await b.eval(`window.__cadUi.session().sheet.links.length`)) !== 1) throw new Error("no quedó el vínculo");
    // 3. Recortar contra la vista: una línea que entra al frente por la izquierda; clic adentro quita ese tramo
    const y = (front.lo[1] + front.hi[1]) / 2;
    await key("l");
    await b.click(...(await at([front.lo[0] - 15, y + 3])), { wait: 400 });
    await b.click(...(await at([front.lo[0] + 8, y + 3])), { wait: 400 });
    await key("Escape");
    await key("Escape");
    await key("t");
    await b.click(...(await at([front.lo[0] + 5, y + 3])), { wait: 600 });
    s = await sk();
    const cut = s.entities.filter((e) => e.geometry.type === "line" && !(s.uses ?? []).some((u) => u.entity === e.id)).map((e) => [pos(s, e.geometry.start), pos(s, e.geometry.end)]).find((l) => Math.abs(l[0][1] - (y + 3)) < 1 && Math.abs(l[1][1] - (y + 3)) < 1);
    if (!cut) throw new Error(`recortar: ${JSON.stringify(s.entities.map((e) => e.geometry.type))}`);
    near(Math.max(cut[0][0], cut[1][0]), front.lo[0], 1e-6, "recortada en el borde de la vista");
    // 4. Una cota de largo en la línea usada y de vuelta al plano: sale en la hoja
    await b.eval(`window.__cadUi.change((sk) => sk.constraints.push({ type: "length", line: ${used.id}, value: 20, reference: true }))`);
    await sleep(800);
    await b.clickText("Volver al plano");
    const dimTexts = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-layer-dims] text")].map((t) => t.textContent)`);
    for (let t = 0; t < 30 && !(await dimTexts()).includes("20"); t++) await sleep(250);
    if (!(await dimTexts()).includes("20")) throw new Error(`cotas del dibujo: ${await dimTexts()}`);
    const saved = (await call("cad_get_document")).drawing.sheets[0];
    if (saved.links?.length !== 1) throw new Error(`vínculos guardados: ${JSON.stringify(saved.links)}`);
    await b.shot("plano_referencias");
    // 5. La caja pasa a 30 de ancho: lo ligado (y su cota) la sigue
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    box.kind.shape.dx = 30;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    for (let t = 0; t < 40 && !(await dimTexts()).includes("30"); t++) await sleep(250);
    if (!(await dimTexts()).includes("30")) throw new Error(`la cota no siguió al modelo: ${await dimTexts()}`);
    await b.shot("plano_referencias_modelo");
  },

  async "plano 2D: las escalas dicen la verdad (lo dibujado mide la pieza por la escala)"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    Object.assign(box.kind.shape, { dx: 100, dy: 60, dz: 40 });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await sleep(500);
    // Tamaño de cada vista en la pieza (ancho × alto en la hoja)
    const size = { front: [100, 40], back: [100, 40], top: [100, 60], bottom: [100, 60], left: [60, 40], right: [60, 40] };
    const measure = () =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const scale = [...svg.querySelectorAll("[data-title-block] text")].map((t) => t.textContent).find((t) => /^\\d+:\\d+$/.test(t));
        const views = Object.fromEntries([...svg.querySelectorAll("[data-view]")].map((g) => { const r = g.getBBox(); return [g.dataset.view, [r.width, r.height]]; }));
        return { scale, views };
      })()`);
    const check = (m, what) => {
      const [a, c] = m.scale.split(":").map(Number);
      const k = a / c;
      for (const [name, [w, h]] of Object.entries(m.views)) {
        if (!size[name]) continue;
        near(w, size[name][0] * k, 0.05, `${what}: ancho de ${name} a ${m.scale}`);
        near(h, size[name][1] * k, 0.05, `${what}: alto de ${name} a ${m.scale}`);
      }
    };
    const four = await measure();
    check(four, "4 vistas");
    // Lo que dicen las ventanas (Hoja y la de una vista) también es la escala de verdad
    const said = () => b.eval(`[...document.querySelectorAll('[data-drawing-props] button, [data-drawing-props] [data-scale-now]')].map((x) => x.textContent.trim()).join(" | ")`);
    await b.clickText("Hoja");
    await sleep(400);
    const sheetBefore = await said();
    for (const v of ["bottom", "right", "back"]) await b.eval(`[...document.querySelector('[data-outliner-row="drawing-1:view:${v}"]').querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 40 && Object.keys((await measure()).views).length < 7; t++) await sleep(250);
    await sleep(500);
    const seven = await measure();
    if (Object.keys(seven.views).length !== 7) throw new Error(`vistas: ${Object.keys(seven.views)}`);
    check(seven, "7 vistas");
    const sheetAfter = await said();
    if (four.scale === seven.scale) throw new Error("con más vistas la automática debía bajar");
    if (!sheetBefore.includes(four.scale) || !sheetAfter.includes(seven.scale) || sheetAfter.includes(four.scale)) throw new Error(`la ventana Hoja no dice la escala: ${sheetBefore} → ${sheetAfter}`);
    const bar = await b.eval(`document.querySelector("[data-sheet-scale]")?.textContent`);
    if (bar !== `Escala ${seven.scale} (auto)`) throw new Error(`barra: ${bar}`);
  },

  async "plano 2D: mover vistas (y lo dibujado con ellas), vista nueva y girar en 3D"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    Object.assign(doc.features.find((f) => f.kind.type === "primitive").kind.shape, { dx: 40, dy: 20, dz: 10 });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 1; })`);
    await sleep(1500);
    // En pantalla (para arrastrar) y en mm de la hoja (para medir: la hoja en pantalla puede cambiar de tamaño)
    const box = (name) => b.eval(`(() => { const g = document.querySelector('[data-sheet] [data-view="${name}"]'); if (!g) return null; const r = g.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; })()`);
    const mm = (name) => b.eval(`(() => { const g = document.querySelector('[data-sheet] [data-view="${name}"]'); if (!g) return null; const r = g.getBBox(); return [r.x, r.y, r.width, r.height]; })()`);
    const screenSize = () => b.eval(`(() => { const r = document.querySelector("[data-sheet]").getBoundingClientRect(); return [r.width, r.height]; })()`);
    const drag = async (from, to, modifiers = 0) => {
      await b.mouse("mouseMoved", from[0], from[1], { buttons: 0 });
      await b.mouse("mousePressed", from[0], from[1], { modifiers });
      for (let i = 1; i <= 6; i++) await b.mouse("mouseMoved", from[0] + ((to[0] - from[0]) * i) / 6, from[1] + ((to[1] - from[1]) * i) / 6, { modifiers });
      await sleep(200);
      await b.mouse("mouseReleased", to[0], to[1], { modifiers, buttons: 0 });
      await sleep(1200);
    };
    const sheets = async () => (await call("cad_get_document")).drawing.sheets[0];
    // La lateral se arrastra en diagonal: solo se corre en horizontal (sigue alineada con el frente)
    const l0 = await box("left");
    const m0 = await mm("left");
    const s0 = await screenSize();
    const c0 = [l0[0] + l0[2] / 2, l0[1] + l0[3] / 2];
    await drag(c0, [c0[0] + 60, c0[1] + 40]);
    const m1 = await mm("left");
    if (!(m1[0] > m0[0] + 10) || Math.abs(m1[1] - m0[1]) > 1e-6) throw new Error(`arrastre alineado (mm): ${m0} → ${m1}`);
    if (!(await sheets()).viewAt?.left) throw new Error("no se guardó dónde quedó");
    const s1 = await screenSize();
    if (Math.abs(s1[0] - s0[0]) > 0.5 || Math.abs(s1[1] - s0[1]) > 0.5) throw new Error(`la hoja cambió de tamaño en pantalla: ${s0} → ${s1}`);
    // Acomodar la devuelve
    await b.clickText("Acomodar");
    for (let t = 0; t < 20 && Math.abs((await mm("left"))[0] - m0[0]) > 1e-6; t++) await sleep(250);
    if (Math.abs((await mm("left"))[0] - m0[0]) > 1e-6) throw new Error("Acomodar no la devolvió");
    // Una línea dibujada encima del frente se mueve con él
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(600);
    // Caja del frente en mm de hoja (Y hacia arriba)
    const fr = await b.eval(`(() => {
      const sh = window.__cadUi.session().sheet;
      const v = sh.views.find((x) => x.name === "front");
      const pts = v.lines.flatMap((l) => l.points);
      const S = (p) => [v.x + p[0] * v.scale, sh.height - v.y + p[1] * v.scale];
      return { lo: S([Math.min(...pts.map((p) => p[0])), Math.min(...pts.map((p) => p[1]))]), hi: S([Math.max(...pts.map((p) => p[0])), Math.max(...pts.map((p) => p[1]))]) };
    })()`);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const lerp = (u, v) => [fr.lo[0] + (fr.hi[0] - fr.lo[0]) * u, fr.lo[1] + (fr.hi[1] - fr.lo[1]) * v];
    await key("l");
    // Adentro del frente, lejos de sus bordes y de sus medios
    await b.click(...(await at(lerp(0.17, 0.31))), { wait: 400 });
    await b.click(...(await at(lerp(0.71, 0.77))), { wait: 400 });
    await key("Escape");
    await key("Escape");
    await b.clickText("Volver al plano");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-layer] polyline")`)); t++) await sleep(250);
    if ((await sheets()).owners?.length !== 1) throw new Error(`dueños: ${JSON.stringify((await sheets()).owners)}`);
    const linePts = () => b.eval(`document.querySelector("[data-sheet] svg [data-layer] polyline").getAttribute("points")`);
    const before = (await linePts()).split(" ").map((p) => p.split(",").map(Number));
    const f0 = await box("front");
    await drag([f0[0] + f0[2] * 0.8, f0[1] + f0[3] * 0.8], [f0[0] + f0[2] * 0.8 + 50, f0[1] + f0[3] * 0.8 + 30]);
    for (let t = 0; t < 20 && (await linePts()) === before.map((p) => p.join(",")).join(" "); t++) await sleep(250);
    const after = (await linePts()).split(" ").map((p) => p.split(",").map(Number));
    const sv = (await sheets()).viewAt.front;
    const dx = after[0][0] - before[0][0];
    const dy = after[0][1] - before[0][1];
    if (!(Math.abs(dx) > 10 && Math.abs(dy) > 5)) throw new Error(`lo dibujado no siguió al frente: ${before} → ${after} (frente en ${sv})`);
    near(after[1][0] - after[0][0], before[1][0] - before[0][0], 1e-6, "misma línea, corrida");
    // Vista nueva (mira como la isométrica) y un giro de 90° con su botón
    await b.clickText("Vista nueva");
    for (let t = 0; t < 30 && !(await box("vista-1")); t++) await sleep(250);
    if (!(await box("vista-1"))) throw new Error("no apareció la vista nueva");
    if ((await b.eval(`document.querySelector('[data-sheet] [data-view-label="vista-1"]')?.textContent`)) !== "Vista 1") throw new Error("la vista nueva no lleva su nombre");
    const dir0 = (await sheets()).viewDirs["vista-1"];
    await b.eval(`[...document.querySelectorAll('[aria-label="Girar la vista"] button')].find((x) => x.textContent === "→").click()`);
    for (let t = 0; t < 20 && JSON.stringify((await sheets()).viewDirs["vista-1"]) === JSON.stringify(dir0); t++) await sleep(250);
    const dir1 = (await sheets()).viewDirs["vista-1"];
    if (JSON.stringify(dir1) === JSON.stringify(dir0)) throw new Error("el botón no giró la vista");
    // Ctrl+arrastrar la isométrica la gira: durante el arrastre se ve la vista previa
    const iso = await box("iso");
    const ci = [iso[0] + iso[2] / 2, iso[1] + iso[3] / 2];
    await b.mouse("mouseMoved", ci[0], ci[1], { buttons: 0 });
    await b.mouse("mousePressed", ci[0], ci[1], { modifiers: 2 });
    for (let i = 1; i <= 5; i++) await b.mouse("mouseMoved", ci[0] + i * 12, ci[1], { modifiers: 2 });
    await sleep(300);
    const preview = await b.eval(`document.querySelectorAll("[data-orbit-preview] polyline").length`);
    await b.mouse("mouseReleased", ci[0] + 60, ci[1], { modifiers: 2, buttons: 0 });
    await sleep(1500);
    if (!(preview > 0)) throw new Error("sin vista previa al girar");
    const isoDir = (await sheets()).viewDirs?.iso;
    if (!isoDir) throw new Error("no se guardó el giro de la isométrica");
    await b.shot("plano_girar_mover");
  },

  async "plano 2D: cortes por línea y escalonados, medio corte, rotura, vista 3D cortada y sombreada"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Caja de 40 × 20 × 10 con dos agujeros pasantes verticales (en x = −10 y x = 10)
    const doc = await call("cad_get_document");
    Object.assign(doc.features.find((f) => f.kind.type === "primitive").kind.shape, { dx: 40, dy: 20, dz: 10 });
    for (const [x, y] of [[-10, -3], [10, 3]]) {
      doc.features.push({ id: doc.next_id, name: `Agujero ${x}`, suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 2, height: 40 }, origin: [x, y, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
      doc.next_id += 1;
    }
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 2; })`);
    await sleep(1500);
    // Punto de una vista (mm de la pieza) → pantalla
    const at = (view, p) =>
      b.eval(`(() => {
        const sh = window.__cadStore.doc().drawing.sheets[0];
        const svg = document.querySelector("[data-sheet] svg");
        const g = svg.querySelector('[data-view="${view}"]');
        const pts = [...g.querySelectorAll("polyline")].flatMap((l) => l.getAttribute("points").split(" ").map((q) => q.split(",").map(Number)));
        const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
        // Centro de la vista en la hoja y la escala (2:1)
        const [cx, cy] = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
        const q = new DOMPoint(cx + ${p[0]} * 2, cy - ${p[1]} * 2).matrixTransform(svg.getScreenCTM());
        return [q.x, q.y];
      })()`);
    const tool = (name) => b.eval(`document.querySelector('button[aria-label="${name}"]').click()`);
    const count = (sel) => b.eval(`document.querySelectorAll('[data-sheet] ${sel}').length`);
    const wait = async (sel, what) => {
      for (let t = 0; t < 40 && !(await count(sel)); t++) await sleep(300);
      if (!(await count(sel))) throw new Error(`no apareció: ${what}`);
    };
    // 1. Corte recto B-B por el medio de la superior (de izquierda a derecha: se mira hacia atrás)
    await tool("Corte");
    await b.click(...(await at("top", [-22, 0])), { wait: 300 });
    await b.click(...(await at("top", [22, 0])), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-hatch="corte-B"] polygon', "rayado del corte B-B");
    await wait('[data-cut-line="B"]', "línea de corte B");
    if (!(await b.eval(`[...document.querySelectorAll("[data-sheet] [data-view-label]")].some((t) => t.textContent === "Corte B-B")`))) throw new Error("sin rótulo Corte B-B");
    // 2. Escalonado C-C: pasa por los dos agujeros (y = −3 y y = 3)
    await tool("Corte");
    for (const p of [[-22, -3], [0, -3], [0, 3], [22, 3]]) await b.click(...(await at("top", p)), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-hatch="corte-C"] polygon', "rayado del corte escalonado");
    if (!(await b.eval(`document.querySelector('[data-outliner-row="drawing-1:cut:C"]')?.textContent.includes("escalonado")`))) throw new Error("el Outliner no dice escalonado");
    // 3. Medio corte del frente (derecha)
    await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:front"]').click()`);
    await sleep(400);
    await b.eval(`[...document.querySelectorAll('[aria-label="Medio corte"] button')].find((x) => x.textContent === "Derecha").click()`);
    await wait('[data-hatch="front"] polygon', "rayado del medio corte");
    await wait('[data-center-line="front"]', "línea central del medio corte");
    // 4. Rotura en la lateral izquierda
    await tool("Rotura");
    for (const p of [[-4, -2], [4, -2], [4, 3], [-4, 3]]) await b.click(...(await at("left", p)), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-break-line="left"]', "contorno de la rotura");
    await wait('[data-hatch="left"] polygon', "rayado de la rotura");
    // 5. Isométrica cortada en un cuarto y sombreada
    await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:iso"]').click()`);
    await sleep(400);
    await b.eval(`[...document.querySelectorAll('[aria-label="Cortar la vista 3D"] button')].find((x) => x.textContent === "Un cuarto").click()`);
    await wait('[data-hatch="iso"] polygon', "rayado de la isométrica cortada");
    await b.eval(`[...document.querySelectorAll('[data-drawing-props] label')].find((l) => l.textContent.trim() === "Sombreada").querySelector("input").click()`);
    await wait('[data-view-image="iso"]', "imagen sombreada");
    await b.shot("plano_cortes");
    // Las vistas de los cortes entran en la hoja
    const inside = await b.eval(`(() => { const svg = document.querySelector("[data-sheet] svg"); const W = svg.viewBox.baseVal.width, H = svg.viewBox.baseVal.height; return ["corte-B", "corte-C"].map((n) => { const r = svg.querySelector('[data-view="' + n + '"]').getBBox(); return r.x >= 10 && r.y >= 10 && r.x + r.width <= W - 10 && r.y + r.height <= H - 10; }); })()`);
    if (inside.some((x) => !x)) throw new Error(`las vistas de los cortes se salen de la hoja: ${inside}`);
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const png = `${out}/iso_sombreada.png`;
    rmSync(png, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(png)}`);
    await b.clickText("Guardar imagen PNG");
    await sleep(1500);
    if (!existsSync(png) || readFileSync(png).subarray(1, 4).toString() !== "PNG") throw new Error("no se guardó el PNG");
    // El PDF lleva la imagen
    const pdf = `${out}/plano_cortes.pdf`;
    rmSync(pdf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(pdf)}`);
    await b.clickText("Exportar PDF");
    await sleep(3000);
    if (!existsSync(pdf)) throw new Error("no se exportó el PDF");
    const { execSync } = await import("node:child_process");
    try {
      const list = execSync(`pdfimages -list ${pdf}`).toString();
      if (list.trim().split("\n").length < 3) throw new Error(`el PDF no tiene la imagen: ${list}`);
    } catch (e) {
      if (String(e).includes("no tiene")) throw e;
    }
    await b.key("Escape", "Escape", 27);
    await sleep(400);
    await b.shot("plano_cortes_hoja");
  },

  async "plano 2D: rayado por pieza según su material, DXF de los cortes y vista del corte alineada"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Dos piezas: la caja de acero y otra al lado de PLA
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    Object.assign(box.kind.shape, { dx: 20, dy: 20, dz: 10 });
    doc.features.push({ id: doc.next_id++, name: "Caja 2", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [20, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const parts = (await evaluate()).parts;
    if (parts.length !== 2) throw new Error(`piezas: ${parts.length}`);
    await b.eval(`window.__cadStore.commit((d) => { d.parts = ${JSON.stringify(parts.map((p, i) => ({ part: p.id, material: i === 0 ? { name: "Acero", density: 7850 } : { name: "PLA", density: 1240 } })))}; })`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
    for (let t = 0; t < 40 && (await b.eval(`document.querySelectorAll('[data-sheet] [data-hatch="front"]').length`)) < 2; t++) await sleep(300);
    const groups = await b.eval(`[...document.querySelectorAll('[data-sheet] [data-hatch="front"]')].map((g) => { const id = g.getAttribute("fill").slice(5, -1); const p = document.querySelector('[data-sheet] pattern[id="' + id + '"]'); return { lines: p.querySelectorAll("line").length, rot: p.getAttribute("patternTransform") }; })`);
    // Acero de a pares (dos líneas); PLA cruzado (dos líneas, una horizontal) y con el ángulo al revés por ser la vecina
    if (groups.length !== 2 || groups[0].rot === groups[1].rot) throw new Error(`rayados por pieza: ${JSON.stringify(groups)}`);
    if (!groups.every((g) => g.lines === 2)) throw new Error(`dibujo de los rayados: ${JSON.stringify(groups)}`);
    // Corte por una línea vertical en el frente: su vista va a un costado, alineada (sin la lateral ni la isométrica, hay lugar)
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].hiddenViews = ["bottom", "right", "back", "left", "iso"]; })`);
    await sleep(1500);
    await b.eval(`document.querySelector('button[aria-label="Corte"]').click()`);
    const at = (p) => b.eval(`(() => { const svg = document.querySelector("[data-sheet] svg"); const g = svg.querySelector('[data-view="front"]').getBBox(); const q = new DOMPoint(g.x + g.width * ${p[0]}, g.y + g.height * ${p[1]}).matrixTransform(svg.getScreenCTM()); return [q.x, q.y]; })()`);
    await b.click(...(await at([0.25, -0.15])), { wait: 300 });
    await b.click(...(await at([0.25, 1.15])), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-view="corte-B"]')`)); t++) await sleep(300);
    const al = await b.eval(`(() => { const f = document.querySelector('[data-sheet] [data-view="front"]').getBBox(); const c = document.querySelector('[data-sheet] [data-view="corte-B"]').getBBox(); return { fy: f.y + f.height / 2, cy: c.y + c.height / 2, fx: f.x, cx: c.x }; })()`);
    near(al.cy, al.fy, 0.05, "la vista del corte a la altura del frente");
    // DXF con las líneas de corte, las letras y el rayado
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const dxf = `${out}/plano_cortes.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    const text = existsSync(dxf) ? readFileSync(dxf, "utf8") : "";
    if (!text.includes("\nRAYADO\n") || !text.includes("\nCORTES\n")) throw new Error("el DXF no trae el rayado o los cortes");
    await b.key("Escape", "Escape", 27);
    await sleep(300);
    await b.shot("plano_material");
  },

  async "plano 2D: radio, ángulo, detalle y corte movido"(b) {
    await begin(b);
    // Medio cono (r 10 → 5, alto 10) cortado en y < 0: arcos en la planta, trapecio en el frente
    const doc = await call("cad_get_document");
    const prim = (shape, origin, op) => ({ id: doc.next_id++, name: shape.type, suppressed: false, kind: { type: "primitive", shape, origin, z: [0, 0, 1], x: [1, 0, 0], op } });
    doc.features.push(prim({ type: "cone", r1: 10, r2: 5, height: 10 }, [0, 0, 0], "join"));
    doc.features.push(prim({ type: "box", dx: 30, dy: 15, dz: 30, centered: true }, [0, -7.5, -10], "cut"));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await sleep(500);
    const toolBtn = (name) => b.eval(`document.querySelector('button[aria-label=${JSON.stringify(name)}]').click()`);
    // Punto de la hoja (coordenadas del SVG) → pantalla
    const at = (sel, pick) =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const lines = [...svg.querySelectorAll(${JSON.stringify(sel)})].map((l) => l.getAttribute("points").split(" ").map((p) => p.split(",").map(Number)));
        const pts = (${pick})(lines);
        const p = new DOMPoint(pts[0], pts[1]).matrixTransform(svg.getScreenCTM());
        return [p.x, p.y];
      })()`);
    // Radio: el arco más largo de la planta, en su medio
    await toolBtn("Cota");
    await sleep(300);
    await b.click(...(await at('[data-view="top"] polyline[data-kind="visible"]', "(ls) => { const l = ls.sort((a, b) => b.length - a.length)[0]; return l[Math.floor(l.length / 2)]; }")), { wait: 1000 });
    // Ángulo: la base del frente y un costado inclinado
    await toolBtn("Ángulo");
    await sleep(300);
    const seg = (kind) =>
      `(ls) => { const s = ls.filter((l) => l.length === 2).map((l) => ({ l, dx: l[1][0] - l[0][0], dy: l[1][1] - l[0][1] })); const k = ${JSON.stringify(kind)} === "h" ? s.filter((x) => Math.abs(x.dy) < 1e-6).sort((a, b) => Math.abs(b.dx) - Math.abs(a.dx))[0] : s.find((x) => Math.abs(x.dx) > 1e-6 && Math.abs(x.dy) > 1e-6); const l = k.l; return [l[0][0] * 0.7 + l[1][0] * 0.3, l[0][1] * 0.7 + l[1][1] * 0.3]; }`;
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("h"))), { wait: 600 });
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("s"))), { wait: 1000 });
    const texts = await b.eval(`[...document.querySelectorAll("[data-sheet] [data-user-dim] text, [data-sheet] [data-radius] text, [data-sheet] [data-angle] text")].map((t) => t.textContent)`);
    if (!texts.includes("R10")) throw new Error(`radio: ${texts}`);
    // atan(10 / 5) = 63,43° (o su suplemento si se eligió el otro lado)
    if (!texts.some((t) => t === "63,43°" || t === "116,57°")) throw new Error(`ángulo: ${texts}`);
    // Detalle en el frente
    await toolBtn("Detalle");
    await sleep(300);
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("s"))), { wait: 1000 });
    const det = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      return { view: svg.querySelectorAll('[data-view="detalle-B"] polyline').length, mark: !!svg.querySelector('[data-detail-mark="B"]'), label: svg.querySelector('[data-detail="B"] text')?.textContent, chip: !!document.querySelector('[data-outliner-row="drawing-1:detail:B"]') && !!document.querySelector('[data-drawing-props="detail"]') };
    })()`);
    if (!(det.view > 0) || !det.mark || !det.chip || !/^Detalle B \(\d+:1\)$/.test(det.label ?? "")) throw new Error(`detalle: ${JSON.stringify(det)}`);
    if ((await call("cad_get_document")).drawing?.sheets?.[0]?.details?.length !== 1) throw new Error("no se guardó el detalle");
    // Corte A-A por el medio (Y = 5) y después en Y = 2: el rayado es el ancho del cono ahí
    const hatchW = () =>
      b.eval(`(() => {
        const ps = [...document.querySelectorAll('[data-sheet] [data-hatch="front"] polygon')].flatMap((p) => p.getAttribute("points").split(" ").map((q) => Number(q.split(",")[0])));
        return ps.length ? Math.max(...ps) - Math.min(...ps) : 0;
      })()`);
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
    for (let t = 0; t < 30 && !(await hatchW()); t++) await sleep(500);
    await sleep(800);
    const mid = await hatchW();
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Posición del corte"]'); i.value = "2"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    for (let t = 0; t < 30 && Math.abs((await hatchW()) - mid) < 1e-6; t++) await sleep(500);
    await sleep(800);
    // 2·√(100 − 4) contra 2·√(100 − 25)
    near((await hatchW()) / mid, Math.sqrt(96 / 75), 0.01, "ancho del corte en Y = 2 contra el medio");
    await b.shot("plano_2d_detalle");
  },
};

siEsPrincipal(import.meta.url, escenarios);
