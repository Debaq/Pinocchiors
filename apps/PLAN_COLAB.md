# PLAN_COLAB — trabajar en conjunto, en tiempo real, dentro de la app

Meta: dos o más personas en el mismo proyecto a la vez. Se ven los clics y cursores del otro, hay chat, comentarios anclados a piezas (tipo issues dentro de la app) y se edita en común.

## Decisión de diseño (la importante)

**El anfitrión manda; no hay CRDT sobre el modelo.**
El estado real vive en Rust (`AppState`: árbol de operaciones CAD, mallas, sketches). Un CRDT (Yjs/Automerge) sirve para texto y listas, no para fusionar un B-rep ni una malla de 60k vértices. Entonces:

- Una persona es **anfitrión**: su app tiene el proyecto y ejecuta todos los comandos.
- Las demás son **invitadas**: mandan comandos y reciben el resultado. Se reusa `cad::bridge::try_dispatch` y la idea de turnos/snapshots de `mcp/live.rs`.
- Lo liviano (presencia, chat, comentarios) sí es compartido y sin dueño: se manda como mensajes pequeños.

Ventaja: no hay conflictos de fusión. Costo: el invitado depende del anfitrión (si este se cae, la sesión se pausa).

## Fases

### C0 — Transporte y sala (DECIDIDO 2026-11-11: P2P con iroh)
- **iroh** (QUIC punto a punto en Rust): atraviesa NAT, cae a un relay solo si no hay camino directo, y todo va cifrado de extremo a extremo. No hace falta servidor propio.
- La «sala» es un ticket corto (código para pegar/compartir) que identifica al anfitrión. Los invitados se conectan directo a él.
- Dos canales por conexión: mensajes livianos sin garantía (cursores, clics) y flujo fiable (comandos, chat, snapshot por trozos).
- Cliente en `apps/desktop/src/collab/` (mod, transport, room). Config ▸ Colaborar: crear sala, unirse, salir.
- Voz y pantalla: fuera de la app (Discord/Jitsi); WebRTC en WebKitGTK es poco fiable.
- Verificar primero que el crate compila en el workspace (edition 2024, rust 1.85) y cuánto pesa.
- Prueba: dos instancias en la misma máquina y luego en dos PCs; ping/pong y reconexión.
- **HECHO 2026-11-11 (sin commitear):** crate `libs/collab` (iroh 1.4): `Room::host/join`, ticket, canal fiable + datagramas; `tests/sala.rs` pasa. Cableado: `apps/desktop/src/collab.rs` (comandos `collab_host/join/leave/status/send/send_light`, evento `collab://event`), `apps/web/src/lib/collab.ts`, Configuración ▸ Colaborar (`CollabSettings.tsx`: nombre/color, crear sala, ticket, unirse, lista, chat de prueba). tsc y cargo check OK. **Falta:** probar en la app real con dos PCs por internet (relays).

### C1 — Presencia (ver al otro)
- Lista de personas con nombre y color en la barra superior.
- Cursor 3D de cada uno en el visor (punto sobre la superficie + etiqueta con nombre), pestaña activa, selección resaltada con su color.
- **Clics**: onda breve en el punto donde hizo clic.
- Botón «Seguir a…»: la cámara copia la del otro.
- Mensajes sin estado, ~20 Hz, se descartan los viejos.

### C2 — Chat y comentarios
- Chat en panel izquierdo con pestañas (convención de paneles).
- **Comentarios anclados**: un comentario se pega a una pieza, cara o punto; aparece como pin en el visor; hilo con respuestas y estado abierto/resuelto. Es el «issues» dentro de la app.
- Se guardan en el `.pinocchio` (`ui.json`, clave `comments`).
- Un comentario puede mandarse al chat de IA («arregla esto»).

### C3 — Co-edición (anfitrión autoritativo)
- Invitado manda `{cmd, args}`; el anfitrión valida, ejecuta con `try_dispatch_with` y difunde el resultado.
- **Bloqueo por pestaña/objeto**: mientras alguien edita un sketch, ese sketch queda con su color y los demás lo ven en solo lectura (evita pisarse).
- Deshacer: cada persona deshace **sus** pasos; el historial muestra el autor.

### C4 — Sincronizar el estado
- Al entrar: el anfitrión manda un snapshot (el `.pinocchio` zip) por trozos.
- Después: difusión del **log de comandos** (no de la malla). El invitado reaplica el comando localmente cuando es determinista; si no, pide los datos resultantes.
- Detección de desfase (hash de estado cada N comandos) y re-sincronizar entero si no cuadra.
- Reconexión: pide solo lo que le falta desde su último número de secuencia.

### C5 — Permisos y seguridad
- Roles por persona: ver / comentar / editar. El anfitrión los cambia en vivo.
- Clave de sala; expulsar; el relay no ve el contenido (cifrado extremo a extremo con la clave de sala).
- Regla ya fijada: **nada de tocar impresoras desde un invitado** (ver `no-tocar-impresiones`). Los comandos de impresión no se aceptan de invitados.

### C6 — Pruebas
- Prueba de unidad del protocolo y del log de comandos.
- e2e con dos instancias (anfitrión + invitado) por CDP: ver el cursor, un comentario, una extrusión hecha por el invitado.
- Probar con red real entre dos PCs antes de dar por cerrado.

## Orden recomendado
C0 → C1 → C2 (ya es útil: se ven y conversan) → C4 → C3 → C5 → C6.

## Riesgos
- Comandos no deterministas (solver, OpenCASCADE): por eso C4 verifica por hash.
- Latencia con mallas grandes: se manda el comando, no el resultado.
- Dependencias de red sin internet al compilar: revisar en C0 qué crates hay.

## Decidido
1. Transporte: iroh P2P.
2. Invitado: solo con la app (sin versión web).
