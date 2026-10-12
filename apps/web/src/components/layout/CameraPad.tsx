import { Component, onCleanup } from "solid-js";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";

export interface CameraPadProps {
  /** Mueve la cámara en el plano de la vista: -1/0/1 en cada eje (derecha y arriba son positivos); Alt = paso fino, Shift = grande */
  onMove: (dx: number, dy: number, mods: { shiftKey: boolean; altKey: boolean }) => void;
}

/** Flechas en forma de teclado para mover la cámara en el plano; mantenidas, repiten */
export const CameraPad: Component<CameraPadProps> = (props) => {
  let timer: number | undefined;
  const stop = () => {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      window.clearInterval(timer);
    }
    timer = undefined;
  };
  onCleanup(stop);

  const key = (dx: number, dy: number, rotate: string, label: string, place: string) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      class={`flex items-center justify-center w-8 h-8 rounded-md border border-border bg-bg-darker/85 text-text-muted shadow hover:text-text hover:bg-accent/25 active:translate-y-px cursor-pointer select-none ${place}`}
      onPointerDown={(e) => {
        e.preventDefault();
        stop();
        const mods = { shiftKey: e.shiftKey, altKey: e.altKey };
        props.onMove(dx, dy, mods);
        // Mantenida, repite tras una pausa corta para que un clic sea un solo paso
        timer = window.setTimeout(() => {
          timer = window.setInterval(() => props.onMove(dx, dy, mods), 30);
        }, 350);
      }}
      onPointerUp={stop}
      onPointerLeave={stop}
      onPointerCancel={stop}
    >
      <Icons.CaretRight size={14} class={rotate} />
    </button>
  );

  // Distribución de las flechas de un teclado: arriba al centro y las otras tres debajo
  return (
    <div class="absolute bottom-3 right-3 z-10 grid grid-cols-3 grid-rows-2 gap-1" role="group" aria-label={t("Mover la cámara")}>
      {key(0, 1, "-rotate-90", t("Mover la cámara arriba"), "col-start-2 row-start-1")}
      {key(-1, 0, "rotate-180", t("Mover la cámara a la izquierda"), "col-start-1 row-start-2")}
      {key(0, -1, "rotate-90", t("Mover la cámara abajo"), "col-start-2 row-start-2")}
      {key(1, 0, "", t("Mover la cámara a la derecha"), "col-start-3 row-start-2")}
    </div>
  );
};
