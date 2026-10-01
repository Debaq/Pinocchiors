import { Component, For, Show, type JSX } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";

export interface WelcomeScreenProps {
  onImport?: () => void;
  /** Escanear con Orizon3D (pestaña Captura) */
  onScan?: () => void;
  /** Abrir una nube de puntos PLY para limpiarla y mallarla */
  onOpenCloud?: () => void;
  /** Trabajar sin modelo: elegir un esqueleto, animarlo y exportarlo */
  onSkeletonOnly?: () => void;
  onOpenProject?: () => void;
  /** Formatos de modelo que se importan (extensiones en minúscula) */
  formats?: string[];
  /** Última sesión guardada por el guardado automático */
  recovery?: { saved_at: number; source_name: string | null };
  onRecover?: () => void;
}

interface Choice {
  title: string;
  detail: string;
  icon: JSX.Element;
  action?: () => void;
  primary?: boolean;
}

/**
 * Inicio sin trabajo abierto: todas las formas de empezar, no solo importar
 * un modelo. También se puede soltar un modelo sobre el visor
 */
export const WelcomeScreen: Component<WelcomeScreenProps> = (props) => {
  const formats = () => (props.formats?.length ? props.formats.map((f) => f.toUpperCase()).join(", ") : "GLB, GLTF, OBJ, STL");
  const choices = (): Choice[] => [
    {
      title: "Importar un modelo",
      detail: `${formats()}, con sus texturas`,
      icon: <Icons.FolderOpen size={22} />,
      action: props.onImport,
      primary: true,
    },
    {
      title: "Escanear",
      detail: "Revopoint por USB, en una o varias tomas",
      icon: <Icons.Scan size={22} />,
      action: props.onScan,
    },
    {
      title: "Abrir una nube de puntos",
      detail: "PLY: limpiar, fusionar y mallar",
      icon: <Icons.Stack size={22} />,
      action: props.onOpenCloud,
    },
    {
      title: "Empezar con un esqueleto",
      detail: "Armar, animar y exportar sin modelo",
      icon: <Icons.Bone size={22} />,
      action: props.onSkeletonOnly,
    },
    {
      title: "Abrir un proyecto",
      detail: "Archivo .pinocchio guardado",
      icon: <Icons.FloppyDisk size={22} />,
      action: props.onOpenProject,
    },
  ];

  return (
    <div class="absolute inset-0 flex items-center justify-center z-10 pointer-events-auto p-8 overflow-auto">
      <div
        class={clsx(
          "flex flex-col gap-5 p-8",
          "rounded-2xl border-2 border-dashed border-border",
          "bg-bg-darker/80 backdrop-blur-sm",
          "max-w-xl w-full"
        )}
      >
        <div class="text-center space-y-1">
          <h2 class="text-lg font-semibold text-text">¿Por dónde empezamos?</h2>
          <p class="text-xs text-text-muted">También puedes arrastrar un modelo 3D sobre el visor</p>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <For each={choices()}>
            {(choice) => (
              <button
                class={clsx(
                  "flex items-center gap-3 p-3 rounded-xl border text-left transition-colors",
                  "disabled:opacity-40 disabled:cursor-not-allowed",
                  choice.primary
                    ? "sm:col-span-2 border-accent/50 bg-accent/10 hover:bg-accent/20"
                    : "border-border bg-bg-lighter/40 hover:bg-bg-lighter hover:border-accent/40"
                )}
                disabled={!choice.action}
                onClick={() => choice.action?.()}
              >
                <span
                  class={clsx(
                    "w-10 h-10 shrink-0 rounded-lg flex items-center justify-center",
                    choice.primary ? "bg-accent/20 text-accent" : "bg-bg-darker text-text-muted"
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

        <Show when={props.recovery}>
          {(r) => (
            <button
              class="flex items-center justify-center gap-2 text-xs text-text-muted hover:text-text"
              onClick={() => props.onRecover?.()}
            >
              <Icons.ArrowCounterClockwise size={13} />
              Recuperar la sesión{r().source_name ? ` de ${r().source_name}` : ""} ·{" "}
              {new Date(r().saved_at * 1000).toLocaleString([], { dateStyle: "short", timeStyle: "short" })}
            </button>
          )}
        </Show>
      </div>
    </div>
  );
};
