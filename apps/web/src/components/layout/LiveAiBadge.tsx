import { Component, Show } from "solid-js";
import * as Icons from "../icons";
import type { CadStore } from "../../lib/cad";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { aiLabel, aiProgress, live, setLivePaused } from "../../lib/mcpLive";

/**
 * Modo en vivo prendido, en la cabecera junto al chat: qué hace la IA (o si
 * espera) y el botón para pausarla. Mientras trabaja se puede seguir mirando
 * y orbitando; editar espera a que termine.
 */
export const LiveAiBadge: Component<{ cad: CadStore }> = (props) => {
  // Un comando largo: cuánto lleva (y qué hace, al pasar el mouse)
  const percent = () => {
    const p = aiProgress();
    return p && props.cad.aiTurn() ? ` ${Math.round(p.fraction * 100)} %` : "";
  };
  const title = () =>
    live().paused
      ? t("IA en pausa")
      : props.cad.aiTurn()
        ? t("IA: {title}…", { title: aiLabel(props.cad.aiTurn()!) }) + percent() + (aiProgress()?.message ? ` — ${tBackend(aiProgress()!.message)}` : "")
        : t("IA en vivo");
  return (
    <Show when={live().on}>
      <div
        class="h-7 flex items-center gap-1.5 pl-2.5 pr-0.5 rounded-md border border-border text-xs"
        role="status"
        aria-label={t("IA en vivo")}
        title={title()}
      >
        <span
          class="w-2 h-2 rounded-full shrink-0"
          classList={{ "bg-accent animate-pulse": !!props.cad.aiTurn(), "bg-success": !props.cad.aiTurn() && !live().paused, "bg-warning": live().paused }}
        />
        <span class="text-text max-w-[13rem] truncate">
          {live().paused ? t("IA en pausa") : props.cad.aiTurn() ? t("IA: {title}…", { title: aiLabel(props.cad.aiTurn()!) }) + percent() : t("IA en vivo")}
        </span>
        <button
          class="flex items-center justify-center w-6 h-6 rounded text-text-muted hover:text-text hover:bg-surface/50"
          title={live().paused ? t("Dejar que la IA siga") : t("Que la IA espere hasta que la reanudes")}
          aria-label={live().paused ? t("Reanudar") : t("Pausar")}
          onClick={() => void setLivePaused(!live().paused)}
        >
          <Show when={live().paused} fallback={<Icons.Pause size={11} />}>
            <Icons.Play size={11} />
          </Show>
        </button>
      </div>
    </Show>
  );
};
