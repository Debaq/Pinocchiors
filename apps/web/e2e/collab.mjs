// Colaboración de punta a punta (C6): dos ventanas con la app real y cada una
// con su backend CAD real; entre ellas, un relé en Node que imita la sala
// (e2e/collab-rele.mjs). Se prueba: clave, aprobación, proyecto compartido,
// diseño en vivo, choque de ediciones, bloqueo por pestaña, comentarios,
// presencia, roles, expulsar y cambiar la clave.
//
//   node apps/web/e2e/collab.mjs
//
// Levanta Vite (E2E_WEB_PORT, 5173) y los dos backends (cad_http en 8766 y
// 8767, de E2E_BRIDGE_BIN o ~/.cargo-target/{release,debug}/examples) si no
// están. Capturas en $E2E_OUT.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { launch, sleep } from "./cdp.mjs";
import { accept, begin, call } from "./cad/_comun.mjs";
import { crearRele } from "./collab-rele.mjs";

const WEB = Number(process.env.E2E_WEB_PORT ?? 5173);
const PORTS = { host: 8766, guest: 8767, guest2: 8768 };
const OUT = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
mkdirSync(OUT, { recursive: true });
const spawned = [];

const responde = (url) => fetch(url).then((r) => r.ok || r.status < 500, () => false);
const bridge = (name, cmd, args = {}) => fetch(`http://127.0.0.1:${PORTS[name]}/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());

async function waitFor(what, fn, ms = 20_000) {
  const t0 = Date.now();
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() - t0 > ms) throw new Error(`tiempo agotado: ${what}`);
    await sleep(250);
  }
}

async function levantar() {
  if (!(await responde(`http://localhost:${WEB}/e2e/collab-harness.html`))) {
    console.log(`· levanto Vite en :${WEB}`);
    const p = spawn("npx", ["vite", "--port", String(WEB), "--strictPort"], { cwd: new URL("../", import.meta.url).pathname, stdio: "ignore", detached: true });
    spawned.push(p);
    await waitFor("Vite", () => responde(`http://localhost:${WEB}/e2e/collab-harness.html`), 60_000);
  }
  const home = process.env.HOME;
  const bin = [process.env.E2E_BRIDGE_BIN, `${home}/.cargo-target/release/examples/cad_http`, `${home}/.cargo-target/debug/examples/cad_http`].find((p) => p && existsSync(p));
  if (!bin) throw new Error("falta cad_http: cargo build -p pinocchio-app --example cad_http");
  for (const port of Object.values(PORTS)) {
    if (await responde(`http://127.0.0.1:${port}/invoke/cad_status`)) continue;
    console.log(`· levanto un backend en :${port}`);
    spawned.push(spawn(bin, [String(port)], { stdio: "ignore", detached: true }));
    await waitFor(`backend ${port}`, () => responde(`http://127.0.0.1:${port}/invoke/cad_status`), 30_000);
  }
}

const M = (n) => `import("/src/lib/${n}.ts")`;
let passed = 0;
const step = async (name, fn) => {
  process.stdout.write(`  · ${name} … `);
  await fn();
  passed++;
  console.log("ok");
};

async function main() {
  await levantar();
  const exportDir = `${OUT}/collab-copia`;
  mkdirSync(exportDir, { recursive: true });
  const rele = await crearRele({ bridge: PORTS, exportDir });
  const url = (peer) => `http://localhost:${WEB}/e2e/collab-harness.html?bridge=${PORTS[peer]}&relay=${rele.port}&peer=${peer}`;
  const host = await launch(url("host"), { width: 1400, height: 900 });
  const guest = await launch(url("guest"), { width: 1400, height: 900 });
  const guest2 = await launch(url("guest2"), { width: 1400, height: 900 });
  const both = [host, guest, guest2];
  // Nombres y colores de cada uno (se recuerdan en la página)
  const named = (b, name, color) => b.eval(`(async () => { const c = await ${M("collab")}; c.setCollabName(${JSON.stringify(name)}); c.setCollabColor(${JSON.stringify(color)}); })()`);
  await named(host, "Ana", "#ff8800");
  await named(guest, "Beto", "#2ec4b6");
  await named(guest2, "Carla", "#6a4c93");
  const status = (b) => b.eval(`(async () => (await ${M("collab")}).status())()`);
  const msg = (b) => b.eval(`document.body.innerText`);

  try {
    // ── Preparar: el anfitrión tiene una caja en Diseñar; el invitado, nada ──
    await step("el anfitrión diseña una caja", async () => {
      await begin(host);
      await host.clickText("Sólidos");
      await sleep(600);
      await host.clickText("Caja");
      await sleep(1500);
      await accept(host);
      const n = await host.eval(`window.__cadStore.doc().features.length`);
      if (n < 1) throw new Error("no hay caja");
    });

    await step("el anfitrión crea la sala (con aprobación)", async () => {
      const ticket = await host.eval(`(async () => { const c = await ${M("collab")}; await c.hostCollab(); return c.status().ticket; })()`);
      if (ticket !== "CLAVE-1") throw new Error("ticket: " + ticket);
    });

    await step("con una clave incorrecta no se entra", async () => {
      await guest.eval(`(async () => { const c = await ${M("collab")}; await c.joinCollab("CLAVE-MALA"); })()`);
      await waitFor("rechazo", async () => !(await status(guest)).on);
    });

    await step("con la clave buena el invitado queda esperando aprobación", async () => {
      await guest.eval(`(async () => { const c = await ${M("collab")}; await c.joinCollab("CLAVE-1"); })()`);
      await waitFor("el anfitrión ve a alguien", () => host.eval(`(async () => (await ${M("collab")}).peers().length === 1)()`));
      const pending = await host.eval(`(async () => { const r = await ${M("collabRoles")}; const c = await ${M("collab")}; return r.pendingPeers(c.peers()).length; })()`);
      if (pending !== 1) throw new Error("pendientes: " + pending);
    });

    await step("sin admitir no le llega nada (ni el chat)", async () => {
      await host.eval(`(async () => (await ${M("collab")}).sendChat("secreto"))()`);
      await sleep(1200);
      const n = await guest.eval(`(async () => (await ${M("collab")}).chat().length)()`);
      if (n !== 0) throw new Error("el pendiente recibió el chat");
      const backend = await bridge("guest", "cad_get_document");
      if (backend && backend.features?.length) throw new Error("el pendiente recibió el proyecto");
    });

    await step("al admitirlo recibe el proyecto del anfitrión", async () => {
      await host.eval(`(async () => (await ${M("collabRoles")}).admit("B"))()`);
      await waitFor("el diseño llegó al backend del invitado", async () => {
        const d = await bridge("guest", "cad_get_document");
        return d && d.features?.length >= 1;
      }, 60_000);
      await waitFor("el invitado muestra Diseñar", () => guest.eval(`!!window.__cadStore?.doc()`), 30_000);
      const a = await bridge("host", "cad_tabs");
      const b = await bridge("guest", "cad_tabs");
      if (JSON.stringify(a.tabs.map((t) => t.name)) !== JSON.stringify(b.tabs.map((t) => t.name))) throw new Error("pestañas distintas");
    });

    const dx = (b) => b.eval(`window.__cadStore.doc()?.features.find((f) => f.kind.type === "primitive")?.kind.shape.dx`);
    const setDx = (b, v) => b.eval(`window.__cadStore.commit((d) => { d.features.find((f) => f.kind.type === "primitive").kind.shape.dx = ${v}; })`);

    await step("un cambio del anfitrión llega al invitado y se recalcula", async () => {
      await setDx(host, 77);
      await waitFor("dx=77 en el invitado", async () => (await dx(guest)) === 77);
      await waitFor("el backend del invitado recalculó", async () => {
        const r = await bridge("guest", "cad_evaluate");
        return r?.body && Math.abs(r.body.bbox_max[0] - r.body.bbox_min[0] - 77) < 1e-6;
      });
    });

    await step("un cambio del invitado llega al anfitrión", async () => {
      await setDx(guest, 33);
      await waitFor("dx=33 en el anfitrión", async () => (await dx(host)) === 33);
    });

    await step("el chat avisa quién cambió el diseño", async () => {
      await waitFor("aviso", async () => (await msg(host)).length > 0 && (await host.eval(`(async () => (await ${M("collab")}).chat().some((c) => c.system))()`)));
    });

    await step("dos cambios a la vez chocan: gana el del anfitrión", async () => {
      await sleep(600);
      await fetch(`http://127.0.0.1:${rele.port}/hold/guest/on`);
      await setDx(host, 20);
      await sleep(400);
      await setDx(guest, 30);
      await sleep(800);
      await fetch(`http://127.0.0.1:${rele.port}/hold/guest/off`);
      await waitFor("los dos quedan en 20", async () => (await dx(host)) === 20 && (await dx(guest)) === 20, 20_000);
    });

    await step("bloqueo: lo que edita el anfitrión no lo edita el invitado", async () => {
      const tab = await host.eval(`window.__cadStore.tabs().active`);
      await host.eval(`(async () => (await ${M("collabLocks")}).setEditing(${tab}))()`);
      await waitFor("el invitado ve el bloqueo", () => guest.eval(`(async () => !!(await ${M("collabLocks")}).lockedByOther(${tab}))()`));
      await setDx(guest, 5);
      await sleep(800);
      if ((await dx(guest)) === 5) throw new Error("el invitado pudo editar una pestaña bloqueada");
      await guest.shot("collab-bloqueo");
      await host.eval(`(async () => (await ${M("collabLocks")}).setEditing(null))()`);
      await waitFor("se libera", () => guest.eval(`(async () => !(await ${M("collabLocks")}).lockedByOther(${tab}))()`));
    });

    await step("comentarios anclados viajan en los dos sentidos", async () => {
      const id = await host.eval(`(async () => (await ${M("collabComments")}).addThread({ view: "design", x: 0.5, y: 0.5, target: "Pieza" }, "Revisar acá"))()`);
      if (!id) throw new Error("no se creó el hilo");
      await waitFor("el invitado ve el hilo", () => guest.eval(`(async () => (await ${M("collabComments")}).threads().length === 1)()`));
      await guest.eval(`(async () => { const c = await ${M("collabComments")}; c.addReply(c.threads()[0].id, "Listo"); })()`);
      await waitFor("el anfitrión ve la respuesta", () => host.eval(`(async () => (await ${M("collabComments")}).threads()[0].comments.length === 2)()`));
    });

    await step("presencia: el cursor del invitado se ve en el anfitrión", async () => {
      const r = await guest.eval(`(() => { const e = document.querySelector('[data-collab-view]'); if (!e) return null; const b = e.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2]; })()`);
      if (!r) throw new Error("no hay visor");
      for (let i = 0; i < 6; i++) {
        await guest.mouse("mouseMoved", r[0] + i * 8, r[1] + i * 5, { buttons: 0 });
        await sleep(80);
      }
      await waitFor("cursor remoto", () => host.eval(`(async () => Object.keys((await ${M("collabPresence")}).cursors()).length > 0)()`), 8000);
      await host.shot("collab-cursor");
    });

    await step("rol «solo ver»: el invitado no edita ni comenta", async () => {
      await host.eval(`(async () => (await ${M("collabRoles")}).setRole("B", "view"))()`);
      await waitFor("el invitado lo sabe", () => guest.eval(`(async () => !(await ${M("collabRoles")}).canEdit())()`));
      await setDx(guest, 9);
      await sleep(800);
      if ((await dx(guest)) === 9) throw new Error("un invitado «solo ver» pudo editar");
      const made = await guest.eval(`(async () => (await ${M("collabComments")}).addThread({ view: "design", x: 0.1, y: 0.1 }, "no debería"))()`);
      if (made) throw new Error("un invitado «solo ver» pudo comentar");
      await host.eval(`(async () => (await ${M("collabRoles")}).setRole("B", "edit"))()`);
      await waitFor("vuelve a poder", () => guest.eval(`(async () => (await ${M("collabRoles")}).canEdit())()`));
    });

    await step("el anfitrión edita mientras un segundo invitado espera aprobación", async () => {
      await guest2.eval(`(async () => (await ${M("collab")}).joinCollab("CLAVE-1"))()`);
      await waitFor("el anfitrión ve a Carla", () => host.eval(`(async () => (await ${M("collab")}).peers().length === 2)()`));
      await setDx(host, 44);
      await waitFor("Beto recibe 44", async () => (await dx(guest)) === 44);
      await host.eval(`(async () => (await ${M("collabRoles")}).admit("C"))()`);
      await waitFor("Carla abre el proyecto", () => guest2.eval(`!!window.__cadStore?.doc()`), 60_000);
      await waitFor("Carla tiene 44", async () => (await dx(guest2)) === 44, 30_000);
    });

    await step("el primer cambio del que recién entró no rebota", async () => {
      await setDx(guest2, 55);
      await waitFor("el anfitrión acepta 55", async () => (await dx(host)) === 55);
      await waitFor("Beto también ve 55", async () => (await dx(guest)) === 55);
      if ((await dx(guest2)) !== 55) throw new Error("a Carla le rebotó su cambio");
    });

    await step("los invitados se conocen entre ellos (nombre y color)", async () => {
      const ve = (b) => b.eval(`(async () => (await ${M("collab")}).peers().map((p) => p.name + " " + p.color).sort().join(", "))()`);
      await waitFor("Beto ve a Ana y a Carla", async () => (await ve(guest)) === "Ana #ff8800, Carla #6a4c93");
      await waitFor("Carla ve a Ana y a Beto", async () => (await ve(guest2)) === "Ana #ff8800, Beto #2ec4b6");
    });

    await step("el chat de un invitado llega a otro con el nombre de quien lo escribió", async () => {
      await guest2.eval(`(async () => (await ${M("collab")}).sendChat("hola de Carla"))()`);
      await waitFor("Beto lo ve firmado por Carla", () =>
        guest.eval(`(async () => (await ${M("collab")}).chat().some((c) => c.text === "hola de Carla" && c.name === "Carla"))()`),
      );
    });

    await step("el cursor de un invitado se ve en otro, con su nombre", async () => {
      const r = await guest2.eval(`(() => { const e = document.querySelector('[data-collab-view]'); const b = e.getBoundingClientRect(); return [b.x + b.width / 3, b.y + b.height / 3]; })()`);
      for (let i = 0; i < 6; i++) {
        await guest2.mouse("mouseMoved", r[0] + i * 7, r[1] + i * 4, { buttons: 0 });
        await sleep(80);
      }
      await waitFor("Beto ve el cursor de Carla", () => guest.eval(`(async () => "C" in (await ${M("collabPresence")}).cursors())()`), 8000);
      await waitFor("con su nombre en pantalla", async () => (await guest.eval(`document.body.innerText`)).includes("Carla"));
      await guest.shot("collab-dos-invitados");
    });

    await step("el bloqueo del anfitrión se ve con su color", async () => {
      const tab = await host.eval(`window.__cadStore.tabs().active`);
      await host.eval(`(async () => (await ${M("collabLocks")}).setEditing(${tab}))()`);
      await waitFor("Beto ve el color de Ana", () => guest.eval(`(async () => (await ${M("collabLocks")}).lockedByOther(${tab})?.color === "#ff8800")()`));
      await host.eval(`(async () => (await ${M("collabLocks")}).setEditing(null))()`);
    });

    await step("al irse Carla, Beto deja de verla y sigue en la sala", async () => {
      await guest2.eval(`(async () => (await ${M("collab")}).leaveCollab())()`);
      await waitFor("Beto ya no la ve", () => guest.eval(`(async () => (await ${M("collab")}).peers().every((p) => p.name !== "Carla"))()`));
      if (!(await status(guest)).on) throw new Error("Beto salió de la sala");
    });

    await step("expulsar: el invitado queda fuera", async () => {
      await host.eval(`(async () => (await ${M("collabRoles")}).kick("B"))()`);
      await waitFor("el invitado salió", async () => !(await status(guest)).on);
    });

    await step("cambiar la clave: el ticket viejo ya no sirve", async () => {
      const t = await host.eval(`(async () => (await ${M("collabRoles")}).rotateKey())()`);
      if (t === "CLAVE-1") throw new Error("la clave no cambió");
      await guest.eval(`(async () => (await ${M("collab")}).joinCollab("CLAVE-1"))()`);
      await waitFor("rechazo con la clave vieja", async () => !(await status(guest)).on);
      await guest.eval(`(async () => (await ${M("collab")}).joinCollab(${JSON.stringify(t)}))()`);
      await waitFor("entra con la nueva", async () => (await status(guest)).on);
    });

    const errs = [...host.logs, ...guest.logs].filter((l) => l.startsWith("EXCEPTION"));
    if (errs.length) throw new Error("excepciones en la página:\n" + errs.join("\n"));
  } catch (e) {
    // Qué tenía cada ventana en pantalla y sus errores, para saber por qué falló
    for (const [n, b] of [["anfitrión", host], ["invitado", guest], ["invitado 2", guest2]]) {
      const text = await b.eval(`document.body.innerText.slice(0, 300)`).catch(() => "?");
      const cadErr = await b.eval(`JSON.stringify({ err: window.__cadStore?.error?.() ?? null, dx: window.__cadStore?.doc()?.features.find((f) => f.kind.type === "primitive")?.kind.shape.dx, peers: (window.__collabPeers ?? null) })`).catch(() => null);
      console.error(`\n--- ${n}: ${JSON.stringify(text.slice(0, 80))} · error del diseño: ${cadErr}\n${b.logs.filter((l) => /^(error|EXCEPTION)/.test(l) && !l.includes("Template shapes")).slice(-4).join("\n")}`);
    }
    console.error("\n--- relé (últimos):\n" + rele.log.filter((l) => !l.endsWith("light")).slice(-20).join("\n"));
    throw e;
  } finally {
    await host.shot("collab-host").catch(() => {});
    await guest.shot("collab-guest").catch(() => {});
    for (const b of both) await b.close?.().catch?.(() => {});
    rele.close();
  }
  console.log(`\n✓ ${passed} pasos`);
}

main()
  .catch((e) => {
    console.error(`\n✗ ${e.message}`);
    process.exitCode = 1;
  })
  .finally(() => {
    for (const p of spawned) {
      try {
        process.kill(-p.pid);
      } catch {}
    }
    setTimeout(() => process.exit(process.exitCode ?? 0), 500);
  });
