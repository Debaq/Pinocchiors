// Ejecutor de los escenarios de e2e/cad/*.mjs.
//
// Cada trabajador tiene su propio backend (cad_http en E2E_BRIDGE_PORT + i) y
// un solo Chromium: entre escenario y escenario la página se vuelve a abrir
// con lo guardado borrado (cdp.mjs: reload), que cuesta una fracción de lanzar
// otro navegador. Con --jobs N corren N trabajadores a la vez.
//
//   node apps/web/e2e/cad.mjs [filtro…] [opciones]
//
//   filtro              parte del nombre del escenario (varios: cualquiera)
//   -g, --grupo a,b     solo esos grupos (archivos de e2e/cad/)
//   -j, --jobs N        trabajadores en paralelo (E2E_JOBS; por defecto 1)
//   --exacto            el filtro tiene que ser el nombre completo
//   --lista             muestra los escenarios por grupo y sale
//   --aislado           un Chromium nuevo por escenario (como antes; más lento)
//
// Si el frontend (E2E_WEB_PORT, 5173) o un backend no responden, los levanta:
// Vite desde apps/web y cad_http desde E2E_BRIDGE_BIN o target/{release,debug}.

import { fork, spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { launch, sleep } from "../cdp.mjs";
import { OUT, tab } from "./_comun.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const WEB_DIR = join(HERE, "../..");
const ROOT = join(WEB_DIR, "../..");
const ENTRY = join(HERE, "../cad.mjs");
const WEB_PORT = Number(process.env.E2E_WEB_PORT ?? 5173);
const BRIDGE_BASE = Number(process.env.E2E_BRIDGE_PORT ?? 8766);
const TIMEOUT = Number(process.env.E2E_TIMEOUT ?? 600) * 1000;
/** Cada cuántos escenarios se cambia el Chromium por uno nuevo (memoria de WebGL…) */
const RELAUNCH = Number(process.env.E2E_RELANZAR ?? 30);
const TIMES = `${OUT}/tiempos.json`;

/** Todos los escenarios: [{ grupo, nombre, run }] en el orden de los archivos */
export async function cargarEscenarios() {
  const files = readdirSync(HERE).filter((f) => f.endsWith(".mjs") && !f.startsWith("_")).sort();
  const all = [];
  for (const f of files) {
    const { escenarios } = await import(pathToFileURL(join(HERE, f)).href);
    for (const [nombre, run] of Object.entries(escenarios)) all.push({ grupo: f.replace(/\.mjs$/, ""), nombre, run });
  }
  const seen = new Set();
  for (const e of all) {
    if (seen.has(e.nombre)) throw new Error("escenario repetido: " + e.nombre);
    seen.add(e.nombre);
  }
  return all;
}

function opciones(argv) {
  const o = { filtros: [], grupos: [], jobs: Number(process.env.E2E_JOBS ?? 1), exacto: false, lista: false, aislado: false, trabajador: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "-j" || a === "--jobs") o.jobs = Number(argv[++i]);
    else if (a.startsWith("--jobs=")) o.jobs = Number(a.slice(7));
    else if (a === "-g" || a === "--grupo") o.grupos.push(...argv[++i].split(","));
    else if (a.startsWith("--grupo=")) o.grupos.push(...a.slice(8).split(","));
    else if (a === "--exacto") o.exacto = true;
    else if (a === "--lista") o.lista = true;
    else if (a === "--aislado") o.aislado = true;
    else if (a === "--trabajador") o.trabajador = true;
    else if (a.startsWith("-")) throw new Error("opción desconocida: " + a);
    else o.filtros.push(a);
  }
  if (!(o.jobs >= 1)) throw new Error("--jobs tiene que ser 1 o más");
  return o;
}

const elegido = (o) => (e) =>
  (!o.grupos.length || o.grupos.includes(e.grupo)) &&
  (!o.filtros.length || o.filtros.some((f) => (o.exacto ? e.nombre === f : e.nombre.includes(f))));

// ── Servicios ────────────────────────────────────────────────────────────────

async function responde(url, init) {
  try {
    await fetch(url, { ...init, signal: AbortSignal.timeout(2000) });
    return true;
  } catch {
    return false;
  }
}
const bridgeUp = (port) => responde(`http://127.0.0.1:${port}/invoke/cad_evaluate`, { method: "POST", body: "{}" });
const webUp = () => responde(`http://localhost:${WEB_PORT}/e2e/cad-harness.html`);

const started = [];
function cleanup() {
  for (const p of started) {
    try {
      process.kill(-p.pid, "SIGKILL");
    } catch {
      try { p.kill("SIGKILL"); } catch {}
    }
  }
}

async function esperar(test, what, segundos) {
  for (let i = 0; i < segundos * 4; i++) {
    if (await test()) return;
    await sleep(250);
  }
  throw new Error(`${what} no respondió en ${segundos} s`);
}

async function asegurarWeb() {
  if (await webUp()) return;
  console.log(`· levanto Vite en :${WEB_PORT}`);
  const p = spawn("npx", ["vite", "--port", String(WEB_PORT), "--strictPort"], { cwd: WEB_DIR, stdio: "ignore", detached: true });
  started.push(p);
  await esperar(webUp, "Vite", 60);
}

function bridgeBin() {
  const tries = [process.env.E2E_BRIDGE_BIN, join(ROOT, "target/release/examples/cad_http"), join(ROOT, "target/debug/examples/cad_http")];
  return tries.find((p) => p && existsSync(p));
}

async function asegurarBackend(port) {
  if (await bridgeUp(port)) return;
  const bin = bridgeBin();
  if (!bin) throw new Error(`no hay backend en :${port} ni binario cad_http (cargo build -p pinocchio-app --example cad_http, o E2E_BRIDGE_BIN)`);
  console.log(`· levanto ${bin.replace(ROOT + "/", "")} en :${port}`);
  const p = spawn(bin, [String(port)], { stdio: "ignore", detached: true });
  started.push(p);
  await esperar(() => bridgeUp(port), `cad_http :${port}`, 60);
}

// ── Navegador ────────────────────────────────────────────────────────────────

/** Las herramientas de los menús de la barra y de las pestañas del panel: si
 * lo pedido no está a la vista, se abre su menú o se prueba en cada pestaña */
function conMenus(b) {
  const clickText = b.clickText.bind(b);
  const clickContains = b.clickContains.bind(b);
  b.clickContains = async (text) => {
    try {
      return await clickContains(text);
    } catch (e) {
      for (const name of ["Diseño", "Pieza", "Inspección", "Desde el escaneo"]) {
        try {
          await tab(b, name);
          return await clickContains(text);
        } catch {}
      }
      throw e;
    }
  };
  b.clickText = async (text, nth = 0) => {
    try {
      return await clickText(text, nth);
    } catch (e) {
      const menu = await b.eval(`[...document.querySelectorAll("[data-toolbar-menu]")].find((m) => JSON.parse(m.dataset.items).includes(${JSON.stringify(text)}))?.dataset.toolbarMenu`);
      if (menu) {
        await clickText(menu);
        await sleep(200);
        // En la lista de la familia cada ítem lleva su atajo al lado (el texto no es exacto)
        const r = await b.eval(`(() => { const e = [...document.querySelectorAll("[data-family-list] button")].find((x) => [...x.childNodes].some((n) => n.textContent.trim() === ${JSON.stringify(text)})); if (!e) return null; const q = e.getBoundingClientRect(); return [q.x + q.width / 2, q.y + q.height / 2]; })()`);
        if (r) return b.click(r[0], r[1]);
        return clickText(text, nth);
      }
      // Lo del panel de Diseñar está repartido en pestañas: probar en cada una
      for (const tab of ["Diseño", "Pieza", "Inspección", "Desde el escaneo"]) {
        if (!(await b.eval(`(() => { const t = document.querySelector('nav button[aria-label=${JSON.stringify(tab)}]'); t?.click(); return !!t; })()`))) continue;
        await sleep(400);
        try {
          return await clickText(text, nth);
        } catch {}
      }
      throw e;
    }
  };
  return b;
}

/** Corre escenarios de a uno contra el backend de `port`, con un solo Chromium */
function trabajador(port, aislado) {
  const url = `http://localhost:${WEB_PORT}/e2e/cad-harness.html?bridge=${port}`;
  const call = (cmd) => fetch(`http://127.0.0.1:${port}/invoke/${cmd}`, { method: "POST", body: "{}" });
  let b = null;
  let uses = 0;
  const cerrar = async () => {
    if (b) await b.close();
    b = null;
  };
  return {
    cerrar,
    async correr(e) {
      const t0 = Date.now();
      await call("cad_close");
      if (b && (aislado || uses >= RELAUNCH)) await cerrar();
      if (b) await b.reload(url);
      else [b, uses] = [conMenus(await launch(url)), 0];
      uses++;
      let timer;
      try {
        // Con la máquina cargada la app tarda en montar: esperar el encabezado
        for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Diseñar")`)); t++) await sleep(500);
        await Promise.race([e.run(b), new Promise((_, no) => (timer = setTimeout(() => no(new Error(`tiempo agotado (${TIMEOUT / 1000} s)`)), TIMEOUT)))]);
        const errors = b.logs.filter((l) => l.startsWith("EXCEPTION"));
        if (errors.length) throw new Error(errors.join("\n"));
        return { ok: true, ms: Date.now() - t0 };
      } catch (err) {
        try {
          await b.shot(e.nombre.replace(/\W+/g, "_"));
        } catch {}
        // Lo que quedó a medias no tiene que pasar al escenario siguiente
        await cerrar();
        return { ok: false, error: err?.message ?? String(err), ms: Date.now() - t0 };
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

// ── Principal ────────────────────────────────────────────────────────────────

function leerTiempos() {
  try {
    return JSON.parse(readFileSync(TIMES, "utf8"));
  } catch {
    return {};
  }
}

const seg = (ms) => `${(ms / 1000).toFixed(1)} s`;

/** Proceso hijo: recibe nombres por IPC y devuelve resultados */
async function modoTrabajador(o) {
  const todos = new Map((await cargarEscenarios()).map((e) => [e.nombre, e]));
  const w = trabajador(Number(process.env.E2E_BRIDGE_PORT), o.aislado);
  // Lo que imprime cada escenario va junto con su resultado (si no, se mezcla entre trabajadores)
  let salida = [];
  console.log = (...a) => salida.push(a.join(" "));
  process.on("message", async (m) => {
    if (m.fin) {
      await w.cerrar();
      process.exit(0);
    }
    salida = [];
    const r = await w.correr(todos.get(m.nombre));
    process.send({ nombre: m.nombre, ...r, salida });
  });
  // Si el principal se corta, no dejar el Chromium andando
  process.on("disconnect", async () => {
    await w.cerrar();
    process.exit(1);
  });
  process.send({ listo: true });
}

/**
 * Corre los escenarios elegidos por la línea de comandos. `base` limita los
 * candidatos (al lanzar un archivo de grupo directamente: solo los suyos).
 */
export async function ejecutar(argv = process.argv.slice(2), base = null) {
  const o = opciones(argv);
  if (o.trabajador) return modoTrabajador(o);

  const todos = await cargarEscenarios();
  const elegidos = todos.filter((e) => (!base || base.has(e.nombre)) && elegido(o)(e));
  if (o.lista) {
    let grupo = null;
    for (const e of elegidos) {
      if (e.grupo !== grupo) console.log(`\n${(grupo = e.grupo)}`);
      console.log(`  ${e.nombre}`);
    }
    console.log(`\n${elegidos.length} escenarios`);
    return;
  }
  if (!elegidos.length) {
    console.log("ningún escenario coincide (--lista los muestra)");
    process.exit(1);
  }
  mkdirSync(OUT, { recursive: true });
  const jobs = Math.min(o.jobs, elegidos.length);
  process.on("exit", cleanup);
  for (const sig of ["SIGINT", "SIGTERM"]) process.on(sig, () => process.exit(130));

  await asegurarWeb();
  for (let i = 0; i < jobs; i++) await asegurarBackend(BRIDGE_BASE + i);

  // Los más largos primero (según la corrida anterior): los trabajadores terminan más parejos
  const tiempos = leerTiempos();
  const cola = jobs > 1 ? [...elegidos].sort((a, c) => (tiempos[c.nombre] ?? 60000) - (tiempos[a.nombre] ?? 60000)) : [...elegidos];
  const t0 = Date.now();
  const fallas = [];
  let hechos = 0;
  const informar = (e, r, salida = []) => {
    hechos++;
    tiempos[e.nombre] = r.ms;
    for (const l of salida) console.log(l);
    const n = jobs > 1 ? `[${hechos}/${elegidos.length}] ` : "";
    if (r.ok) console.log(`✓ ${n}${e.nombre} (${seg(r.ms)})`);
    else {
      fallas.push(e.nombre);
      console.log(`✗ ${n}${e.nombre}: ${r.error}`);
    }
  };

  if (jobs === 1) {
    const w = trabajador(BRIDGE_BASE, o.aislado);
    for (const e of cola) informar(e, await w.correr(e));
    await w.cerrar();
  } else {
    const porNombre = new Map(elegidos.map((e) => [e.nombre, e]));
    await Promise.all(
      Array.from({ length: jobs }, (_, i) =>
        new Promise((done, fail) => {
          const args = ["--trabajador", ...(o.aislado ? ["--aislado"] : [])];
          const child = fork(ENTRY, args, { env: { ...process.env, E2E_BRIDGE_PORT: String(BRIDGE_BASE + i) } });
          const siguiente = () => {
            const e = cola.shift();
            if (e) child.send({ nombre: e.nombre });
            else child.send({ fin: true });
          };
          child.on("message", (m) => {
            if (!m.listo) informar(porNombre.get(m.nombre), m, m.salida);
            siguiente();
          });
          child.on("exit", (code) => (code === 0 ? done() : fail(new Error(`el trabajador ${i + 1} terminó con ${code}`))));
        }),
      ),
    );
  }

  try {
    writeFileSync(TIMES, JSON.stringify(tiempos, null, 1));
  } catch {}
  console.log(`\n${elegidos.length - fallas.length}/${elegidos.length} bien en ${seg(Date.now() - t0)}${jobs > 1 ? ` con ${jobs} trabajadores` : ""}`);
  if (fallas.length) {
    console.log("Fallaron (para repetir uno: node apps/web/e2e/cad.mjs --exacto \"nombre\"):");
    for (const f of fallas) console.log(`  ${f}`);
  }
  process.exit(fallas.length ? 1 : 0);
}

/** Al final de cada archivo de grupo: `node e2e/cad/<grupo>.mjs [filtro…]` corre solo ese grupo */
export function siEsPrincipal(metaUrl, escenarios) {
  // Sin await: ejecutar() vuelve a importar este mismo archivo y esperaría a que termine de cargar
  if (process.argv[1] && pathToFileURL(process.argv[1]).href === metaUrl)
    ejecutar(process.argv.slice(2), new Set(Object.keys(escenarios))).catch((e) => {
      console.error(`✗ ${e.message}`);
      process.exit(1);
    });
}
