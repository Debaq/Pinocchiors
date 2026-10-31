import { Component, createSignal, For, Index, Show, type JSX } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";
import logo from "../../assets/logo.svg";
import splashOpen from "../../assets/splash/open.svg";
import splashScan from "../../assets/splash/scan.svg";
import splashAnimate from "../../assets/splash/animate.svg";
import splashDesign from "../../assets/splash/design.svg";
import splashFabricate from "../../assets/splash/fabricate.svg";
import { locale, t } from "../../lib/i18n";

/** Una lámina por forma de empezar (scripts/logo.py), en el orden de las
 * opciones; al final el logo, que se ve sin el mouse sobre ninguna */
const SPLASHES = [splashOpen, splashScan, splashAnimate, splashDesign, splashFabricate, logo];
const BASE = SPLASHES.length - 1;

export interface WelcomeScreenProps {
  /** Un solo diálogo para proyectos .pinocchio y modelos 3D */
  onOpen?: () => void;
  /** Escanear con Orizon3D (pestaña Captura) */
  onScan?: () => void;
  /** Rig y animación: se puede armar un esqueleto sin modelo */
  onAnimate?: () => void;
  /** CAD paramétrico desde cero */
  onDesign?: () => void;
  /** Importar un modelo y pasar a fabricarlo */
  onFabricate?: () => void;
  /** Formatos de modelo que se importan (extensiones en minúscula) */
  formats?: string[];
  /** Última sesión guardada por el guardado automático */
  recovery?: { saved_at: number; source_name: string | null };
  onRecover?: () => void;
  /** Archivos abiertos hace poco */
  recent?: { path: string; name: string }[];
  onOpenRecent?: (path: string) => void;
}

interface Choice {
  title: string;
  detail: string;
  icon: JSX.Element;
  action?: () => void;
  primary?: boolean;
}

/**
 * Inicio sin trabajo abierto: el logo en una lámina a la izquierda y las formas
 * de empezar a la derecha; al pasar el mouse por una opción, la lámina muestra a
 * la marioneta haciéndola.
 * También se puede soltar un modelo sobre el visor
 */
export const WelcomeScreen: Component<WelcomeScreenProps> = (props) => {
  // Con el mouse sobre una opción, la lámina muestra a la marioneta haciéndola
  const [hovered, setHovered] = createSignal<number>();
  const shown = () => hovered() ?? BASE;
  const formats = () => (props.formats?.length ? props.formats.map((f) => f.toUpperCase()).join(", ") : "GLB, GLTF, OBJ, STL");
  const choices = (): Choice[] => [
    {
      title: t("Abrir / importar"),
      detail: t("Proyecto .pinocchio o modelo {formats}", { formats: formats() }),
      icon: <Icons.FolderOpen size={20} />,
      action: props.onOpen,
      primary: true,
    },
    {
      title: t("Escanear"),
      detail: t("Revopoint o plato giratorio, y nubes de puntos"),
      icon: <Icons.Scan size={20} />,
      action: props.onScan,
    },
    {
      title: t("Animar"),
      detail: t("Esqueleto, pesos, poses y animación, con o sin modelo"),
      icon: <Icons.PersonArmsSpread size={20} />,
      action: props.onAnimate,
    },
    {
      title: t("Diseñar"),
      detail: t("Piezas paramétricas desde un boceto"),
      icon: <Icons.Ruler size={20} />,
      action: props.onDesign,
    },
    {
      title: t("Fabricar"),
      detail: t("Importar un modelo para imprimirlo en piezas"),
      icon: <Icons.Printer size={20} />,
      action: props.onFabricate,
    },
  ];

  return (
    <div class="absolute inset-0 flex items-center justify-center z-10 pointer-events-auto p-8 overflow-auto">
      <div
        class={clsx(
          "flex flex-col sm:flex-row overflow-hidden",
          "rounded-2xl border border-border shadow-2xl",
          "bg-bg-darker/90 backdrop-blur-sm",
          "max-w-3xl w-full"
        )}
      >
        <div class="sm:w-[44%] shrink-0 bg-splash flex flex-col items-center justify-center px-6 pt-6 pb-5">
          <div class="grid w-full max-w-60">
            <Index each={SPLASHES}>
              {(src, i) => (
                <img
                  src={src()}
                  alt=""
                  class={clsx(
                    "col-start-1 row-start-1 w-full select-none transition-opacity duration-200",
                    shown() === i ? "opacity-100" : "opacity-0"
                  )}
                  draggable={false}
                />
              )}
            </Index>
          </div>
          <div class="text-center mt-3">
            <div class="text-2xl font-bold tracking-tight text-splash-text">Pinocchio</div>
            <div class="text-[11px] text-splash-muted">{t("Escanear, reparar, animar y fabricar modelos 3D")}</div>
          </div>
        </div>

        <div class="flex-1 min-w-0 flex flex-col gap-4 p-6">
          <div class="space-y-0.5">
            <h2 class="text-base font-semibold text-text">{t("¿Por dónde empezamos?")}</h2>
            <p class="text-xs text-text-muted">{t("También puedes arrastrar un modelo 3D sobre el visor")}</p>
          </div>

          <div class="flex flex-col gap-1.5" onMouseLeave={() => setHovered(undefined)}>
            {/* Sin acción no se ofrece (Escanear en Android) */}
            <For each={choices().filter((c) => c.action)}>
              {(choice, i) => (
                <button
                  class={clsx(
                    "flex items-center gap-3 px-3 py-2.5 rounded-xl border text-left transition-colors",
                    "disabled:opacity-40 disabled:cursor-not-allowed",
                    choice.primary
                      ? "border-accent/50 bg-accent/10 hover:bg-accent/20"
                      : "border-transparent hover:bg-bg-lighter hover:border-border"
                  )}
                  disabled={!choice.action}
                  onMouseEnter={() => setHovered(i())}
                  onFocus={() => setHovered(i())}
                  onClick={() => choice.action?.()}
                >
                  <span
                    class={clsx(
                      "w-9 h-9 shrink-0 rounded-lg flex items-center justify-center",
                      choice.primary ? "bg-accent/20 text-accent" : "bg-bg-lighter text-text-muted"
                    )}
                  >
                    {choice.icon}
                  </span>
                  <span class="min-w-0">
                    <span class="block text-sm font-medium text-text">{choice.title}</span>
                    <span class="block text-[11px] text-text-muted leading-snug">{choice.detail}</span>
                  </span>
                </button>
              )}
            </For>
          </div>

          <Show when={(props.recent?.length ?? 0) > 0}>
            <div class="space-y-1">
              <span class="text-[11px] font-medium uppercase tracking-wide text-text-dim">{t("Recientes")}</span>
              <For each={props.recent}>
                {(r) => (
                  <button
                    class="flex w-full items-center gap-2 px-2 py-1 rounded-md text-xs text-text-muted hover:text-text hover:bg-bg-lighter text-left"
                    title={r.path}
                    onClick={() => props.onOpenRecent?.(r.path)}
                  >
                    <Icons.Cube size={13} class="shrink-0" />
                    <span class="truncate">{r.name}</span>
                  </button>
                )}
              </For>
            </div>
          </Show>

          <Show when={props.recovery}>
            {(r) => (
              <button
                class="mt-auto flex items-center gap-2 text-xs text-text-muted hover:text-text"
                onClick={() => props.onRecover?.()}
              >
                <Icons.ArrowCounterClockwise size={13} />
                {r().source_name ? t("Recuperar la sesión de {name}", { name: r().source_name! }) : t("Recuperar la sesión")} ·{" "}
                {new Date(r().saved_at * 1000).toLocaleString(locale(), { dateStyle: "short", timeStyle: "short" })}
              </button>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
};
