// Relé en Node que hace de sala entre las dos páginas del e2e de colaboración:
// imita lo que hace `pinocchio-collab` (anfitrión, invitados, silenciado hasta
// admitir, clave, expulsar…) sobre HTTP. El transporte P2P real se prueba en
// libs/collab/tests; acá se prueba la lógica de la interfaz con backends reales.

import { createServer } from "node:http";

export function crearRele({ bridge, exportDir }) {
  /** Páginas: "host" y los invitados ("guest", "guest2"…); cada una con su cola de eventos */
  const pages = {};
  const page = (name) => (pages[name] ??= { queue: [], waiter: null, held: false });
  /** Id de cada página en la sala (como el id de iroh) */
  const ID = { host: "A", guest: "B", guest2: "C", guest3: "D" };
  const room = { open: false, ticket: "", approval: true, host: null, guests: new Map() };
  const log = [];

  const flush = (name) => {
    const p = page(name);
    if (p.waiter && p.queue.length && !p.held) {
      const w = p.waiter;
      p.waiter = null;
      w(p.queue.splice(0));
    }
  };
  const push = (name, ev) => {
    page(name).queue.push(ev);
    flush(name);
  };
  const guestById = (id) => [...room.guests.entries()].find(([, g]) => g.id === id);
  const status = (name) =>
    name === "host" && room.open
      ? { on: true, host: true, ticket: room.ticket, my_id: "A" }
      : room.guests.has(name)
        ? { on: true, host: false, ticket: room.ticket, my_id: ID[name] }
        : { on: false, host: false, ticket: null, my_id: null };

  const callBridge = async (name, cmd, args) => {
    const r = await fetch(`http://127.0.0.1:${bridge[name]}/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) });
    return r.json();
  };

  async function command(name, cmd, a) {
    log.push(`${name} ${cmd}${a?.msg?.t === "data" ? " " + a.msg.kind : ""}${a?.only ? " →" + a.only : ""}`);
    const host = name === "host";
    switch (cmd) {
      case "collab_host":
        room.open = true;
        room.ticket = "CLAVE-1";
        room.approval = !!a.approval;
        room.host = { name: a.name, color: a.color };
        room.guests.clear();
        return status("host");
      case "collab_status":
        return status(name);
      case "collab_join": {
        if (!room.open || a.ticket !== room.ticket) {
          push(name, { kind: "rejected", reason: "clave incorrecta" });
          return { on: true, host: false, ticket: a.ticket, my_id: ID[name] };
        }
        const id = ID[name];
        room.guests.set(name, { id, name: a.name, color: a.color, muted: room.approval });
        push("host", { kind: "joined", id, name: a.name, color: a.color });
        push(name, { kind: "joined", id: "A", name: room.host.name, color: room.host.color });
        return status(name);
      }
      case "collab_leave":
        if (host && room.open) {
          for (const g of room.guests.keys()) push(g, { kind: "left", id: "A" });
          room.open = false;
          room.guests.clear();
        } else if (!host && room.guests.has(name)) {
          const { id } = room.guests.get(name);
          room.guests.delete(name);
          push("host", { kind: "left", id });
        }
        return status(name);
      case "collab_send": {
        if (host) {
          for (const [g, info] of room.guests) {
            // Como `Room`: a uno solo pasa aunque esté silenciado; a todos, solo a los admitidos
            if (a.only) {
              if (a.only === info.id) push(g, { kind: "message", from: "A", msg: a.msg });
            } else if (!info.muted && a.except !== info.id) push(g, { kind: "message", from: "A", msg: a.msg });
          }
        } else if (room.guests.has(name)) {
          push("host", { kind: "message", from: ID[name], msg: a.msg });
        }
        return null;
      }
      case "collab_send_light": {
        if (host) {
          for (const [g, info] of room.guests) if (!info.muted) push(g, { kind: "light", from: "A", bytes: a.bytes });
        } else if (room.guests.has(name)) {
          const me = room.guests.get(name);
          push("host", { kind: "light", from: me.id, bytes: a.bytes });
          // El anfitrión reparte la presencia de un invitado admitido a los demás
          if (!me.muted) for (const [g, info] of room.guests) if (g !== name && !info.muted) push(g, { kind: "light", from: "A", bytes: a.bytes });
        }
        return null;
      }
      case "collab_set_muted": {
        const g = guestById(a.id);
        if (g) g[1].muted = !!a.muted;
        return null;
      }
      case "collab_set_approval":
        room.approval = !!a.on;
        return null;
      case "collab_kick": {
        const g = guestById(a.id);
        if (g) {
          room.guests.delete(g[0]);
          push("host", { kind: "left", id: a.id });
          push(g[0], { kind: "left", id: "A" });
        }
        return null;
      }
      case "collab_rotate_key":
        room.ticket = "CLAVE-" + (Number(room.ticket.split("-")[1]) + 1);
        return status("host");
      case "collab_send_project": {
        // La copia real: el backend del anfitrión escribe el proyecto y el del invitado lo abre
        const g = guestById(a.to);
        if (!g) return 0;
        const path = `${exportDir}/incoming-${g[0]}.pinocchio`;
        await callBridge("host", "save_project", { path, ui: a.ui, onlyIfChanged: false, design: a.design });
        push(g[0], { kind: "project", from: "A", path, bytes: 1 });
        return 1;
      }
      default:
        return null;
    }
  }

  const server = createServer(async (req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Headers", "*");
    if (req.method === "OPTIONS") return res.writeHead(204).end();
    const url = new URL(req.url, "http://x");
    const parts = url.pathname.split("/").filter(Boolean);
    const json = (code, v) => res.writeHead(code, { "content-type": "application/json" }).end(JSON.stringify(v));
    try {
      if (parts[0] === "poll") {
        const p = page(parts[1]);
        const out = await new Promise((resolve) => {
          p.waiter = resolve;
          flush(parts[1]);
          setTimeout(() => {
            if (p.waiter === resolve) {
              p.waiter = null;
              resolve([]);
            }
          }, 10_000);
        });
        return json(200, out);
      }
      if (parts[0] === "cmd") {
        let body = "";
        for await (const c of req) body += c;
        return json(200, await command(parts[1], parts[2], JSON.parse(body || "{}")));
      }
      if (parts[0] === "hold") {
        // Retiene (o suelta) lo que llega a una página: para provocar un choque de ediciones
        const p = page(parts[1]);
        p.held = parts[2] === "on";
        flush(parts[1]);
        return json(200, null);
      }
      if (parts[0] === "log") return json(200, log);
      json(404, "no existe");
    } catch (e) {
      json(500, String(e));
    }
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve({ port: server.address().port, close: () => server.close(), log })));
}
