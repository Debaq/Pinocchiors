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

**C1 HECHO 2026-11-11.** Cursor con nombre y color, onda en cada clic, puntos en la cabecera. El cursor lleva un punto 3D (`CameraRig.rayPoint/toNdc`, registro en `lib/collabViews.ts`, `collabView()` en `CadViewer` y `Viewer3D`): el otro lo proyecta con su propia cámara, así se ve en el mismo lugar del modelo aunque miren distinto. La etiqueta muestra lo que tiene elegido (objeto en el visor 3D, pestaña en Diseñar). «Seguir a…»: clic en el punto de color de la cabecera copia la cámara de esa persona (se manda cada 120 ms si cambia, o cada 1,5 s); tocar el visor o la rueda la suelta. Límites: el punto 3D es a la distancia del objetivo de la cámara (sin lanzar un rayo contra la malla), y la «selección» es el nombre, no un resaltado de caras. Eso último pasa a C3 (bloqueos por pieza). Sin probar con dos instancias reales.

### C2 — Chat y comentarios (HECHO 2026-11-11)
- Panel izquierdo `CollabPanel.tsx` con pestañas **Chat** y **Comentarios**; se abre desde la insignia de la cabecera (`CollabBadge`, siempre visible; cuenta sin leer y abiertos). El chat de prueba salió de Configuración.
- **Comentarios anclados** (`lib/collabComments.ts`): hilo pegado a un punto de la superficie (`CollabViewer.surface`, raycast contra la pieza; si no toca, el punto del rayo), con respuestas, resuelto/reabrir, borrar (ConfirmDialog), «Ir al punto» (traslada la cámara) y «Preguntar a la IA» (abre el chat con el hilo). Pines numerados y de color en el visor que siguen a la cámara; modo «Poner un comentario» (clic en el visor, Esc cancela).
- Sirven sin sala (anotaciones del proyecto). Con sala: el anfitrión es dueño; el invitado manda `comment_op` y el anfitrión reparte; al entrar un invitado recibe `comments_full` (y recupera los suyos al salir). Se guardan en `ui.json` (`comments`); no entran en el deshacer.
- Probado en Chromium sin Tauri (panel, pines, guardar y recargar). **Falta:** probar con dos instancias reales; si el anfitrión abre otro proyecto con invitados dentro, no se les reenvía (lo cubre C4); un pin no se oculta si hay pieza delante.

### C3 — Co-edición (HECHO 2026-11-11)
- **Bloqueo por pestaña de diseño** (`lib/collabLocks.ts`): quien abre un sketch o un diálogo de operación toma la pestaña mientras dure. Los demás la ven con un punto del color de quien la edita (`DesignTabs`) y un aviso «X está editando esta pestaña: solo lectura»; sus cambios quedan bloqueados (`CadStore.setCollabGuard`) y un sketch o diálogo que intenten abrir ahí se cierra con aviso. El anfitrión decide (`acquire` / `release` / `state` / `denied`); si se va quien la tenía, se libera sola. Granularidad: la pestaña (no objeto ni sketch suelto).
- **Deshacer:** al llegar un cambio de otra persona, tus pasos de deshacer de esa pestaña se vacían (partían de otra versión; deshacerlos pisaría lo que llegó). Deshacer vuelve a valer desde ahí y solo toca tus pasos nuevos. No es un deshacer selectivo por autor sobre el árbol de operaciones.
- **Autor:** el chat muestra «X cambió el diseño de «Soporte»» (como mucho cada 30 s por pestaña). El panel Historial de las operaciones de malla no lleva autor.
- Probado en Chromium sin Tauri con una sala simulada (pedir, negar, no quitar, liberar al irse). tsc OK. **Falta:** probar con dos instancias reales y la parte visual del punto/aviso.

### C4 — Sincronizar el estado (HECHO 2026-11-11)
Cambio de diseño respecto de lo escrito antes: **no hay log de comandos**. El diseño CAD es un documento (`cad_model::Document`) que la ventana empuja al backend con `cad_set_document`; compartir ese documento es más simple y más robusto que repetir comandos.
- **Proyecto entero al entrar:** el anfitrión exporta una copia (`project::export_copy_impl`, no marca su proyecto como guardado) y la manda por un stream QUIC propio (`Room::send_blob`, sin trocear; probado con 3 MB). El invitado la recibe en disco (`app_data/collab/incoming.pinocchio`), pregunta si hay trabajo sin guardar y la abre con el flujo normal (`handleOpenProject(path, recovered, shared)`). «Resincronizar con el anfitrión» (Configuración ▸ Colaborar) la pide otra vez.
- **Diseño en vivo (`lib/collabDesign.ts`):** cada cambio confirmado de la pestaña activa (gancho en `CadStore.sendNow`, sin previews) se manda; el otro lo aplica (`CadStore.applyRemote`) y lo recalcula con su motor. Detección de choques: cada cambio lleva el hash canónico del documento anterior; el anfitrión rechaza el que no coincide y devuelve su versión (`forced`). Lo que no se puede aplicar ahora (diálogo abierto, otra pestaña, IA trabajando) queda pendiente y se aplica solo.
- Pruebas: `libs/collab/tests/sala.rs` (archivo de 3 MB) y `apps/mcp/tests/collab_copy.rs` (exportar y abrir la copia en otro estado, diseño y pestañas iguales). tsc y cargo check OK.
- **Límites:** pestañas nuevas/borradas y mallas (reparar, rig, animación) no viajan en vivo: el invitado se resincroniza (lo pide solo si llega un diseño de una pestaña que no tiene). Si el anfitrión abre otro proyecto con invitados dentro, hay que resincronizar a mano. Los comentarios sí van en vivo (C2). Deshacer no es por autor todavía (C3). Sin probar con dos instancias reales.

### C5 — Permisos y seguridad (HECHO 2026-11-11)
- **Clave de sala:** el ticket lleva una clave de 128 bits; el anfitrión rechaza (y cierra) a quien no la trae. «Cambiar la clave» invalida el ticket anterior sin sacar a los que ya están. Sin clave correcta el invitado recibe `Rejected` y sale con aviso.
- **Aprobación (por defecto activa):** quien entra queda *silenciado en `Room`* (no recibe nada de lo que se reparte —chat, diseño, comentarios, bloqueos— ni se reparte lo suyo) hasta que el anfitrión lo admite («X quiere entrar» en el panel y contador en la cabecera). Recién ahí se le manda el proyecto.
- **Roles** (ver / comentar / editar, el anfitrión los cambia en vivo en Configuración ▸ Colaborar; el de entrada se elige antes de crear la sala): el anfitrión descarta lo que llega sin permiso (`setCollabPermit`: ver = presencia y resincronizar; comentar = + chat y comentarios; editar = + diseño y bloqueos). El invitado lo sabe (`collab_role`) y su interfaz queda de solo lectura (editar bloqueado, comentar/chat deshabilitados).
- **Expulsar / rechazar:** `Room::kick`.
- **Impresoras:** ningún invitado puede tocarlas **por construcción**: el único canal es el de datos de la sala (diseño, comentarios, bloqueos, chat) y el anfitrión nunca ejecuta comandos que mande un invitado.
- **Cifrado:** QUIC/TLS de iroh ya es de extremo a extremo entre las dos apps (el relay solo ve bytes cifrados); no se agrega otra capa.
- Pruebas (`libs/collab/tests/sala.rs`): sin clave no se entra, expulsar, rotar clave (el viejo falla, el nuevo entra), con aprobación el pendiente no recibe nada hasta ser admitido.

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
