import { Component, Show } from "solid-js";
import * as Icons from "../icons";
import type { CadStore } from "../../lib/cad";
import { t } from "../../lib/i18n";
import { aiLabel, live, setLivePaused } from "../../lib/mcpLive";

/**
 * Modo en vivo prendido: arriba al centro, qué hace la IA (o si espera) y el
 * botón para pausarla. Mientras trabaja se puede seguir mirando y orbitando;
 * editar espera a que termine.
 */
export const LiveAiBadge: Component<{ cad: CadStore }> = (props) => (
  <Show when={live().on}>
    <div
      class="fixed top-12 left-1/2 -translate-x-1/2 z-40 flex items-center gap-2 pl-3 pr-1 py-1 rounded-full border border-border bg-bg-lighter/95 shadow-lg backdrop-blur text-xs"
      role="status"
      aria-label={t("IA en vivo")}
    >
      <span
        class="w-2 h-2 rounded-full"
        classList={{ "bg-accent animate-pulse": !!props.cad.aiTurn(), "bg-success": !props.cad.aiTurn() && !live().paused, "bg-warning": live().paused }}
      />
      <span class="text-text max-w-[22rem] truncate">
        {live().paused ? t("IA en pausa") : props.cad.aiTurn() ? t("IA: {title}…", { title: aiLabel(props.cad.aiTurn()!) }) : t("IA en vivo")}
      </span>
      <button
        class="flex items-center gap-1 px-2 py-0.5 rounded-full text-text-muted hover:text-text hover:bg-surface/50"
        title={live().paused ? t("Dejar que la IA siga") : t("Que la IA espere hasta que la reanudes")}
        onClick={() => void setLivePaused(!live().paused)}
      >
        <Show when={live().paused} fallback={<Icons.Pause size={11} />}>
          <Icons.Play size={11} />
        </Show>
        {live().paused ? t("Reanudar") : t("Pausar")}
      </button>
    </div>
  </Show>
);
