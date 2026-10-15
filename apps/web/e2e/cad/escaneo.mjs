// Trabajo sobre un escaneo importado: comparar, alinear, referencias y operaciones hasta el escaneo.
//   node apps/web/e2e/cad/escaneo.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo escaneo              (lo mismo)

import { writeFileSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { call, SPECULUM, near, sphereStl, twoCubesStl, importModel, begin, evaluate, sketchOn, body, accept, tab } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
  async "escaneo: cilindro elegido con un clic y comparar el diseño con colores de desviación"(b) {
    await b.eval(`window.__nextPath = [${JSON.stringify(SPECULUM)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(4000);
    await begin(b);
    await b.clickContains("Cilindro / agujero");
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([6.3, -6.3, 38])`)), { wait: 3000 });
    await b.clickContains("Agregar el cilindro");
    await sleep(3000);
    // El cilindro elegido con un clic mide lo que la pared del escaneo
    const cyl = await body();
    near(cyl.bbox_max[0] - cyl.bbox_min[0], 18, 0.05, "diámetro del cilindro escaneado");
    await b.eval(`[...document.querySelectorAll('[aria-label="Comparar con el escaneo"] button')].find((x) => x.textContent.trim() === "Comparar").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Dentro de tolerancia")`)); t++) await sleep(500);
    const text = await b.eval(`document.querySelector('[aria-label="Comparar con el escaneo"]').innerText`);
    const within = parseFloat((text.match(/Dentro de tolerancia\s+([\d,]+)/) ?? [])[1]?.replace(",", "."));
    // El cilindro calza con su zona del escaneo: una parte apreciable queda dentro de ±0,2 mm
    if (!(within > 1)) throw new Error(`dentro de tolerancia: ${text}`);
    if (!(await b.eval(`!!window.__cadViewer.scan.geometry.getAttribute("color")`))) throw new Error("el escaneo no se coloreó");
    await b.shot("desviacion");
    await b.clickText("Quitar los colores");
    await sleep(500);
    if (await b.eval(`!!window.__cadViewer.scan.geometry.getAttribute("color")`)) throw new Error("quedaron los colores");
  },

  async "escaneo: sigue de referencia con una pieza activa"(b) {
    // El "escaneo": dos cubos de 20 mm en STL (24 triángulos)
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-dos-cubos.stl`;
    writeFileSync(path, twoCubesStl());
    await importModel(b, path);
    await begin(b);
    const scanCount = () => b.eval(`window.__cadViewer?.scan?.geometry.index.count / 3`);
    for (let t = 0; t < 20 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`no se ve el escaneo: ${await scanCount()}`);
    // Una pieza, a Fabricar (queda activa) y de vuelta a Diseñar
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    await sleep(1000);
    const rows = () => b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    await b.eval(`[...document.querySelector("[data-outliner]").querySelectorAll("span")].find((x) => x.textContent === "Pieza 1").click()`);
    for (let t = 0; t < 40 && (await rows()).indexOf("Malla generada") !== (await rows()).indexOf("Pieza 1") + 1; t++) await sleep(250);
    const got = await rows();
    if (got.indexOf("Malla generada") !== got.indexOf("Pieza 1") + 1) throw new Error(`la pieza no quedó activa: ${JSON.stringify(got)}`);
    await b.clickText("Diseñar");
    await sleep(1000);
    for (let t = 0; t < 20 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`el escaneo desapareció con la pieza activa: ${await scanCount()}`);
    // Y las medidas se toman del escaneo, no de la pieza: 12 caras planas (6 por cubo, sin unir)
    const found = await b.eval(`window.__cadStore.scanDetect().then((d) => d.filter((x) => x.shape.type === "plane").length)`);
    if (found < 7) throw new Error(`planos detectados: ${found} (esperaba los de los dos cubos)`);
    await tab(b, "Desde el escaneo");
    await sleep(500);
    await b.shot("escaneo-pieza-activa");
    // Un segundo escaneo (un solo cubo): pasa a ser el de referencia al
    // importarlo (queda activo) y se puede volver a elegir el primero
    const one = path.replace("dos-cubos", "un-cubo");
    writeFileSync(one, twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n");
    await b.eval(`window.__nextPath = ${JSON.stringify(one)}`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", ctrlKey: true }))`);
    for (let t = 0; t < 60 && !(await b.eval(`document.body.innerText.includes("escaneo-un-cubo")`)); t++) await sleep(250);
    // Al terminar de importar la app pasa sola a Preparar: esperar ese cambio
    // (el panel de Remallar) antes de volver a Diseñar, para que no lo deshaga
    for (let t = 0; t < 60 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await b.clickText("Diseñar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Desde el escaneo"]')`)); t++) await sleep(250);
    await tab(b, "Desde el escaneo");
    for (let t = 0; t < 40 && (await scanCount()) !== 12; t++) await sleep(250);
    if ((await scanCount()) !== 12) throw new Error(`el escaneo nuevo no es la referencia: ${await scanCount()}`);
    if (!(await b.eval(`!!document.querySelector('[aria-label="Escaneo de referencia"]')`))) throw new Error("falta el selector de escaneo");
    const first = await b.eval(`window.__objects().find((o) => o.name === "escaneo-dos-cubos.stl").id`);
    await b.eval(`window.__cadStore.commit((d) => { d.scan_object = ${first}; })`);
    for (let t = 0; t < 40 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`no volvió al primer escaneo: ${await scanCount()}`);
    const planes = await b.eval(`window.__cadStore.scanDetect().then((d) => d.filter((x) => x.shape.type === "plane").length)`);
    if (planes < 7) throw new Error(`las medidas no son del escaneo elegido: ${planes} planos`);
    await b.shot("escaneo-elegido");
  },

  async "escaneo: alinear por cara al piso y punto al origen"(b) {
    // Un cubo de 20 mm girado 30° en X y 20° en Z y corrido: el "escaneo"
    const rot = (p) => {
      const [ax, az] = [Math.PI / 6, Math.PI / 9];
      const [x, y, z] = p;
      const [y1, z1] = [y * Math.cos(ax) - z * Math.sin(ax), y * Math.sin(ax) + z * Math.cos(ax)];
      const [x2, y2] = [x * Math.cos(az) - y1 * Math.sin(az), x * Math.sin(az) + y1 * Math.cos(az)];
      return [x2 + 15, y2 - 5, z1 + 12];
    };
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const tilted = stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => "vertex " + rot([+x, +y, +z]).join(" "));
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-inclinado.stl`;
    writeFileSync(path, tilted);
    await importModel(b, path);
    await begin(b);
    await tab(b, "Desde el escaneo");
    await sleep(800);
    const planes = () => b.eval(`window.__cadStore.scanDetect().then((d) => JSON.stringify(d.filter((x) => x.shape.type === "plane").map((x) => x.shape.plane)))`).then(JSON.parse);
    // Cara de abajo del cubo original (z = 0): su centro girado
    const bottom = rot([10, 10, 0]);
    const clickAt = async (p) => {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
      await b.click(x, y);
    };
    // Que se vea la cara de abajo: vista desde abajo
    await b.eval(`window.__cadViewer.lookFrom([0.3, -0.4, -1])`);
    await sleep(1200);
    await b.clickText("Cara al piso");
    await sleep(300);
    await clickAt(bottom);
    let floor;
    for (let t = 0; t < 40; t++) {
      floor = (await planes()).find((p) => p.normal[2] < -0.999);
      if (floor && Math.abs(floor.origin[2]) < 0.05) break;
      await sleep(250);
    }
    if (!floor || Math.abs(floor.origin[2]) > 0.05) throw new Error(`la cara no quedó en el piso: ${JSON.stringify(floor)} ${JSON.stringify(await planes())}`);
    // Punto al origen: el centro de la cara de arriba (ahora z = 20)
    const top = (await planes()).find((p) => p.normal[2] > 0.999);
    if (!top || Math.abs(top.origin[2] - 20) > 0.05) throw new Error(`cara de arriba: ${JSON.stringify(top)}`);
    await b.eval(`window.__cadViewer.lookFrom([0.2, -0.3, 1])`);
    await sleep(1200);
    await b.clickText("Punto al origen");
    await sleep(300);
    await clickAt(top.origin);
    let after;
    for (let t = 0; t < 40; t++) {
      after = (await planes()).find((p) => p.normal[2] > 0.999);
      if (after && Math.abs(after.origin[2]) < 0.05) break;
      await sleep(250);
    }
    if (!after || Math.abs(after.origin[2]) > 0.05) throw new Error(`la cara de arriba no pasó por el origen: ${JSON.stringify(after)}`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(500);
    await b.shot("escaneo-alineado");
  },

  async "escaneo: referencias ligadas siguen al alinear"(b) {
    const rot = (p) => {
      const [ax, az] = [Math.PI / 6, Math.PI / 9];
      const [x, y, z] = p;
      const [y1, z1] = [y * Math.cos(ax) - z * Math.sin(ax), y * Math.sin(ax) + z * Math.cos(ax)];
      const [x2, y2] = [x * Math.cos(az) - y1 * Math.sin(az), x * Math.sin(az) + y1 * Math.cos(az)];
      return [x2 + 15, y2 - 5, z1 + 12];
    };
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-inclinado-refs.stl`;
    writeFileSync(path, stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => "vertex " + rot([+x, +y, +z]).join(" ")));
    await importModel(b, path);
    await begin(b);
    await tab(b, "Desde el escaneo");
    await sleep(800);
    const clickAt = async (p) => {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
      await b.click(x, y);
    };
    const refs = () => b.eval(`JSON.stringify(window.__cadStore.result()?.references ?? [])`).then(JSON.parse);
    // Desde arriba se ve la cara de arriba del cubo (z = 20 sin girar)
    await b.eval(`window.__cadViewer.lookFrom([0.2, -0.3, 1])`);
    await sleep(1200);
    const top = rot([10, 10, 20]);
    await b.clickText("Plano de zona");
    await sleep(300);
    await clickAt(top);
    for (let t = 0; t < 40 && (await refs()).length < 1; t++) await sleep(250);
    await b.clickText("Punto");
    await sleep(300);
    await clickAt(top);
    for (let t = 0; t < 40 && (await refs()).length < 2; t++) await sleep(250);
    let r = await refs();
    const plane = r.find((x) => x.kind === "plane");
    const point = r.find((x) => x.kind === "point");
    if (!plane || !point) throw new Error(`referencias: ${JSON.stringify(r)}`);
    // Inclinado: la normal del plano es la de la cara de arriba girada
    if (plane.plane.normal[2] > 0.95) throw new Error(`el plano ya estaba horizontal: ${JSON.stringify(plane.plane)}`);
    // Alinear: la cara de abajo al piso
    await b.eval(`window.__cadViewer.lookFrom([0.3, -0.4, -1])`);
    await sleep(1200);
    await b.clickText("Cara al piso");
    await sleep(300);
    await clickAt(rot([10, 10, 0]));
    // Las referencias van con el escaneo: plano horizontal a 20 mm, punto a 20 mm
    let ok = false;
    for (let t = 0; t < 60 && !ok; t++) {
      await sleep(250);
      r = await refs();
      const p = r.find((x) => x.kind === "plane")?.plane;
      const q = r.find((x) => x.kind === "point")?.point;
      ok = !!p && p.normal[2] > 0.999 && Math.abs(p.origin[2] - 20) < 0.05 && !!q && Math.abs(q[2] - 20) < 0.05;
    }
    if (!ok) throw new Error(`las referencias no siguieron al escaneo: ${JSON.stringify(r)}`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-referencias");
  },

  async "escaneo: el sketch se ancla al corte del escaneo"(b) {
    // Cubo de 20 mm centrado en el origen: la planta lo corta en un cuadrado
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-centrado.stl`;
    writeFileSync(path, stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => `vertex ${+x - 10} ${+y - 10} ${+z - 10}`));
    await importModel(b, path);
    await begin(b);
    await sketchOn(b);
    const outline = () => b.eval(`window.__cadUi.scanOutline()?.sketch.entities.length ?? 0`);
    for (let t = 0; t < 40 && (await outline()) < 4; t++) await sleep(250);
    if ((await outline()) !== 4) throw new Error(`el corte no salió como un cuadrado: ${await outline()} entidades`);
    if (!(await b.eval(`!!window.__cadViewer.scanSection`))) throw new Error("no se dibuja el corte");
    const before = await b.eval(`__cadUi.session().sketch.entities.length`);
    await b.clickText("Usar el corte");
    await sleep(1500);
    const lines = await b.eval(`__cadUi.session().sketch.entities.filter((e) => e.geometry.type === "line").length`);
    if (lines - before < 4) throw new Error(`no se copió el corte: ${before} → ${lines}`);
    // Las líneas copiadas están sobre el cuadrado de ±10 mm
    const xs = await b.eval(`JSON.stringify(__cadUi.session().sketch.points.map((p) => [Math.round(p.x), Math.round(p.y)]))`);
    if (!xs.includes("[10,10]") || !xs.includes("[-10,-10]")) throw new Error(`puntos del corte: ${xs}`);
    await b.shot("escaneo-corte-sketch");
  },

  async "escaneo: cuerpo del escaneo deja la cuna en un bloque"(b) {
    // Cubo de 20 mm (0..20) como escaneo y un bloque que toma su mitad de abajo
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-cuna.stl`;
    writeFileSync(path, stl);
    await importModel(b, path);
    await begin(b);
    const doc = await call("cad_get_document");
    const id = doc.next_id;
    doc.features.push({ id, name: "Bloque", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 15, centered: false }, origin: [-10, -10, -5], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = id + 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await tab(b, "Desde el escaneo");
    await b.clickText("Escaneo como cuerpo");
    // Prepara el escaneo (vóxeles) y resta: unos segundos
    const state = () => b.eval(`JSON.stringify(window.__cadStore.result()?.status.map((s) => s.state.type ?? s.state))`).then(JSON.parse);
    for (let t = 0; t < 120 && (await state())?.length !== 2; t++) await sleep(250);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const r = await evaluate();
    const st = r.status.map((s) => s.state.type ?? s.state);
    if (st[1] !== "ok") throw new Error(`cuerpo del escaneo: ${JSON.stringify(r.status[1])}`);
    if (r.parts.length !== 1) throw new Error(`piezas: ${r.parts.length}`);
    // Se lleva la mitad de abajo del cubo con 0,2 mm de holgura: 20,4 × 20,4 × 10,2
    // (las aristas del cubo quedan algo redondeadas por los vóxeles)
    near(r.parts[0].volume, 40 * 40 * 15 - 20.4 * 20.4 * 10.2, 80, "bloque menos el escaneo");
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 1.2])`);
    await sleep(800);
    await b.shot("escaneo-cuna");
  },

  async "escaneo: extruir y llevar una cara hasta el escaneo"(b) {
    // Esfera de radio 20 con centro a 40 mm de altura (la panza en z = 20)
    const [R, cz] = [20, 40];
    await importModel(b, sphereStl(R, cz, "escaneo-esfera"));
    await begin(b);
    // Círculo de radio 5 en la planta, extruido hasta el escaneo con 0,5 mm de separación
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }],
      entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 5 } }],
      constraints: [],
      next_id: 3,
      origin: 0,
    };
    const doc = await call("cad_get_document");
    doc.features = [
      { id: 1, name: "Sketch 1", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } },
      { id: 2, name: "Columna", suppressed: false, kind: { type: "extrude", sketch: 1, regions: { type: "all" }, extent: { type: "up_to_scan", offset: 0.5 }, reverse: false, op: "new" } },
    ];
    doc.next_id = 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    let r;
    for (let t = 0; t < 120; t++) {
      r = await evaluate();
      if (r.status?.[1]?.state === "ok" || r.status?.[1]?.state === "error") break;
      await sleep(250);
    }
    if (r.status[1].state !== "ok") throw new Error(`hasta el escaneo: ${JSON.stringify(r.status[1])}`);
    // π r² · 40 − ∫ 2πρ √(R² − ρ²) dρ con R = 20,5
    const big = R + 0.5;
    const column = Math.PI * 25 * cz - ((2 * Math.PI) / 3) * (big ** 3 - (big * big - 25) ** 1.5);
    near(r.parts[0].volume, column, column * 0.01, "columna hasta la panza");
    // El diálogo muestra el modo y la separación
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(1500);
    const text = await b.eval(`document.body.innerText`);
    if (!text.includes("Hasta el escaneo") || !text.includes("Separación")) throw new Error("el diálogo no muestra «Hasta el escaneo» con su separación");
    await b.key("Escape", "Escape", 27);
    await sleep(500);

    // Cara al escaneo: la tapa de un bloque de 10 sube hasta la panza (sin separación)
    const d2 = await call("cad_get_document");
    d2.features = [
      { id: 1, name: "Bloque", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 10, dy: 10, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } },
      { id: 2, name: "Cara al escaneo 1", suppressed: false, kind: { type: "face_to_scan", faces: [{ point: [0, 0, 10], normal: [0, 0, 1] }], offset: 0 } },
    ];
    d2.next_id = 3;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    for (let t = 0; t < 120; t++) {
      r = await evaluate();
      if (r.status?.[1]?.state === "ok" || r.status?.[1]?.state === "error") break;
      await sleep(250);
    }
    if (r.status[1].state !== "ok") throw new Error(`cara al escaneo: ${JSON.stringify(r.status[1])}`);
    // Bloque + entre z = 10 y la panza (grilla)
    let added = 0;
    for (let i = 0; i < 100; i++)
      for (let j = 0; j < 100; j++) {
        const [x, y] = [-5 + (i + 0.5) * 0.1, -5 + (j + 0.5) * 0.1];
        added += (cz - Math.sqrt(R * R - x * x - y * y) - 10) * 0.01;
      }
    near(r.parts[0].volume, 1000 + added, (1000 + added) * 0.01, "bloque hasta la panza");
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.4])`);
    await sleep(800);
    await b.shot("escaneo-cara");
  },

  async "escaneo: carcasa y parche bajo un sketch"(b) {
    // Esfera de radio 20 con centro a 40 mm; círculo de radio 10 arriba, en z = 70
    const [R, cz, r] = [20, 40, 10];
    await importModel(b, sphereStl(R, cz, "escaneo-carcasa"));
    await begin(b);
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }],
      entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: r } }],
      constraints: [],
      next_id: 3,
      origin: 0,
    };
    const doc = await call("cad_get_document");
    doc.features = [{ id: 1, name: "Sketch 1", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 70, sketch } }];
    doc.next_id = 2;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const settle = async (n) => {
      let res;
      for (let t = 0; t < 240; t++) {
        res = await evaluate();
        if (res.status?.length === n && ["ok", "error"].includes(res.status[n - 1].state?.type ?? res.status[n - 1].state)) break;
        await sleep(250);
      }
      const st = res.status[n - 1].state;
      if ((st.type ?? st) !== "ok") throw new Error(`operación ${n}: ${JSON.stringify(res.status[n - 1])}`);
      return res;
    };
    // Parche desde el sketch: 0,2 mm de holgura y 2 mm hacia afuera, solo la tapa de arriba
    await tab(b, "Desde el escaneo");
    await b.clickText("Parche desde sketch");
    let res = await settle(2);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const [r1, r2] = [R + 0.2, R + 2.2];
    const cap = (big) => ((2 * Math.PI) / 3) * (big ** 3 - (big * big - r * r) ** 1.5);
    near(res.parts[0].volume, cap(r2) - cap(r1), (cap(r2) - cap(r1)) * 0.03, "parche bajo el círculo");
    if (res.body.bbox_min[2] < cz) throw new Error(`el parche baja hasta ${res.body.bbox_min[2]}: tomó también la cara de abajo`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-parche");
    // El diálogo muestra la forma, el espesor y el sketch
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(1500);
    const text = await b.eval(`document.body.innerText`);
    for (const w of ["Carcasa hacia afuera", "Espesor", "Solo bajo"]) if (!text.includes(w)) throw new Error(`el diálogo no muestra «${w}»`);
    await b.key("Escape", "Escape", 27);
    await sleep(500);
    // Carcasa entera: otra pieza, con el hueco de la esfera adentro
    await tab(b, "Desde el escaneo");
    await b.clickText("Carcasa");
    res = await settle(3);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const ball = (x) => (4 / 3) * Math.PI * x ** 3;
    const shell = res.parts.find((p) => Math.abs(p.volume - (ball(r2) - ball(r1))) < (ball(r2) - ball(r1)) * 0.03);
    if (!shell) throw new Error(`no hay una carcasa de ${(ball(r2) - ball(r1)).toFixed(0)} mm³: ${res.parts.map((p) => p.volume.toFixed(0))}`);
  },

  async "escaneo: pincel pinta un parche sobre la piel"(b) {
    const [R, cz] = [20, 40];
    await importModel(b, sphereStl(R, cz, "escaneo-pincel"));
    await begin(b);
    await tab(b, "Desde el escaneo");
    // Desde arriba, un trazo de 8 mm sobre la tapa de la esfera con el pincel de 5 mm de radio
    await b.eval(`window.__cadViewer.lookFrom([0.001, -0.001, 1])`);
    await sleep(800);
    await b.clickText("Pincel");
    await sleep(300);
    const top = (x) => [x, 0, cz + Math.sqrt(R * R - x * x)];
    const path = [];
    for (const x of [-4, -2, 0, 2, 4]) path.push(await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(top(x))})`));
    await b.dragPath(path);
    let res;
    for (let t = 0; t < 240; t++) {
      res = await evaluate();
      if (res.status?.length === 1 && ["ok", "error"].includes(res.status[0].state?.type ?? res.status[0].state)) break;
      await sleep(250);
    }
    const st = res.status?.[0]?.state;
    if ((st?.type ?? st) !== "ok") throw new Error(`parche pintado: ${JSON.stringify(res.status)}`);
    // Un trazo de 8 mm con pincel de 5 mm: entre el disco solo (r 5) y la pastilla completa, de 2 mm de espesor
    const v = res.parts[0].volume;
    if (!(v > 250 && v < 420)) throw new Error(`volumen del parche pintado: ${v}`);
    if (res.body.bbox_min[2] < cz + R * 0.8) throw new Error(`el parche baja hasta ${res.body.bbox_min[2]}`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-pincel");
  },

  async "escaneo: molde de dos mitades con llaves y bebedero"(b) {
    const [R, cz] = [20, 40];
    await importModel(b, sphereStl(R, cz, "escaneo-molde"));
    await begin(b);
    await tab(b, "Desde el escaneo");
    await b.clickText("Crear molde");
    let res;
    for (let t = 0; t < 480; t++) {
      res = await evaluate();
      const ok = res.status?.length >= 9 && res.status.every((x) => ["ok", "error"].includes(x.state?.type ?? x.state));
      if (ok) break;
      await sleep(250);
    }
    const bad = res.status.filter((x) => (x.state?.type ?? x.state) !== "ok");
    if (bad.length) throw new Error(`operaciones del molde con error: ${JSON.stringify(bad)}`);
    if (res.parts.length !== 2) throw new Error(`el molde tiene ${res.parts.length} piezas, no 2`);
    // Cajas de (40 + 16)² × 56 menos la esfera (con 0,2 de holgura) y el bebedero, más las llaves: dentro del 3 %
    const side = 2 * R + 16;
    const expected = side * side * side - (4 / 3) * Math.PI * (R + 0.2) ** 3 - Math.PI * 9 * (R + 8);
    const total = res.parts.reduce((a, p) => a + p.volume, 0);
    near(total, expected, expected * 0.03, "volumen del molde");
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-molde");
  },
};

siEsPrincipal(import.meta.url, escenarios);
