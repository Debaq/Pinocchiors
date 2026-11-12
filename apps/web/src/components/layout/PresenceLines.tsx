// Quién más está en una pestaña: una línea del color de cada persona pegada al
// borde de abajo. Siempre ocupa el mismo ancho; con varias personas se reparte
// en partes iguales, una al lado de la otra. La usan las pestañas de las áreas
// y las de los diseños y ensambles (y cualquier otra barra de pestañas).

import { For, Show } from "solid-js";
import type { Peer } from "../../lib/collab";

export const PresenceLines = (props: { peers: Peer[] }) => (
  <Show when={props.peers.length > 0}>
    <span
      data-presence-lines
      class="pointer-events-none absolute inset-x-1.5 bottom-0 flex h-[3px] gap-px overflow-hidden rounded-full"
      title={props.peers.map((p) => p.name).join(", ")}
      aria-label={props.peers.map((p) => p.name).join(", ")}
    >
      <For each={props.peers}>{(p) => <span class="h-full flex-1" style={{ background: p.color }} />}</For>
    </span>
  </Show>
);
