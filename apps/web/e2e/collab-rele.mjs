// Relé en Node que hace de sala entre las dos páginas del e2e de colaboración:
// imita lo que hace `pinocchio-collab` (anfitrión, invitados, silenciado hasta
// admitir, clave, expulsar…) sobre HTTP. El transporte P2P real se prueba en
// libs/collab/tests; acá se prueba la lógica de la interfaz con backends reales.

import { createServer } from "node:http";

export function crearRele({ bridge, exportDir }) {
  /** páginas: "host" | "guest" → cola de eventos y quién espera */
  const pages = { host: { queue: [], waiter: null, held: false }, guest: { queue: [], waiter: null, held: false } };
  const room = { open: false, ticket: "", approval: true, host: null, guest: null };
  const log = [];

  const flush = (name) => {
    const p = pages[name];
    if (p.waiter && p.queue.length && !p.held) {
      const w = p.waiter;
      p.waiter = null;
      const out = p.queue.splice(0);
      w(out);
    }
  };
  const push = (name, ev) => {
    pages[name].queue.push(ev);
    flush(name);
  };
  const status = (name) =>
    name === "host" && room.open
      ? { on: true, host: true, ticket: room.ticket, my_id: "A" }
      : name === "guest" && room.guest
        ? { on: true, host: false, ticket: room.ticket, my_id: "B" }
        : { on: false, host: false, ticket: null, my_id: null };

  const callBridge = async (name, cmd, args) => {
    const r = await fetch(`http://127.0.0.1:${bridge[name]}/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) });
    return r.json();
  };

  async function command(name, cmd, a) {
    log.push(`${name} ${cmd}`);
    const host = name === "host";
    switch (cmd) {
      case "collab_host":
        room.open = true;
        room.ticket = "CLAVE-1";
        room.approval = !!a.approval;
        room.host = { name: a.name, color: a.color };
        room.guest = null;
        return status("host");
      case "collab_status":
        return status(name);
      case "collab_join": {
        if (!room.open || a.ticket !== room.ticket) {
          push("guest", { kind: "rejected", reason: "clave incorrecta" });
          return { on: true, host: false, ticket: a.ticket, my_id: "B" };
        }
        room.guest = { name: a.name, color: a.color, muted: room.approval };
        push("host", { kind: "joined", id: "B", name: a.name, color: a.color });
        push("guest", { kind: "joined", id: "A", name: room.host.name, color: room.host.color });
        return status("guest");
      }
      case "collab_leave":
        if (host && room.open) {
          if (room.guest) push("guest", { kind: "left", id: "A" });
          room.open = false;
          room.guest = null;
        } else if (!host && room.guest) {
          room.guest = null;
          push("host", { kind: "left", id: "B" });
        }
        return status(name);
      case "collab_send": {
        if (host) {
          const g = room.guest;
          if (!g || g.muted) return null;
          if (a.only && a.only !== "B") return null;
          if (a.except === "B") return null;
          push("guest", { kind: "message", from: "A", msg: a.msg });
        } else {
          push("host", { kind: "message", from: "B", msg: a.msg });
        }
        return null;
      }
      case "collab_send_light":
        if (host) {
          if (room.guest && !room.guest.muted) push("guest", { kind: "light", from: "A", bytes: a.bytes });
        } else {
          push("host", { kind: "light", from: "B", bytes: a.bytes });
        }
        return null;
      case "collab_set_muted":
        if (room.guest && a.id === "B") room.guest.muted = !!a.muted;
        return null;
      case "collab_set_approval":
        room.approval = !!a.on;
        return null;
      case "collab_kick":
        if (room.guest && a.id === "B") {
          room.guest = null;
          push("host", { kind: "left", id: "B" });
          push("guest", { kind: "left", id: "A" });
        }
        return null;
      case "collab_rotate_key":
        room.ticket = "CLAVE-" + (Number(room.ticket.split("-")[1]) + 1);
        return status("host");
      case "collab_send_project": {
        // La copia real: el backend del anfitrión escribe el proyecto y el del invitado lo abre
        const path = `${exportDir}/incoming.pinocchio`;
        await callBridge("host", "save_project", { path, ui: a.ui, onlyIfChanged: false, design: a.design });
        push("guest", { kind: "project", from: "A", path, bytes: 1 });
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
        const p = pages[parts[1]];
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
        const p = pages[parts[1]];
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
