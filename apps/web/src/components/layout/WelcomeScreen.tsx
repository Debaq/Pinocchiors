import { Component, For, Show, type JSX } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";
import logo from "../../assets/logo.svg";

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
}

interface Choice {
  title: string;
  detail: string;
  icon: JSX.Element;
  action?: () => void;
  primary?: boolean;
}

/**
 * Inicio sin trabajo abierto: el logo en una lámina a la izquierda y las
 * formas de empezar a la derecha. También se puede soltar un modelo sobre el visor
 */
export const WelcomeScreen: Component<WelcomeScreenProps> = (props) => {
  const formats = () => (props.formats?.length ? props.formats.map((f) => f.toUpperCase()).join(", ") : "GLB, GLTF, OBJ, STL");
  const choices = (): Choice[] => [
    {
      title: "Abrir / importar",
      detail: `Proyecto .pinocchio o modelo ${formats()}`,
      icon: <Icons.FolderOpen size={20} />,
      action: props.onOpen,
      primary: true,
    },
    {
      title: "Escanear",
      detail: "Revopoint o plato giratorio, y nubes de puntos",
      icon: <Icons.Scan size={20} />,
      action: props.onScan,
    },
    {
      title: "Animar",
      detail: "Esqueleto, pesos, poses y animación, con o sin modelo",
      icon: <Icons.PersonArmsSpread size={20} />,
      action: props.onAnimate,
    },
    {
      title: "Diseñar",
      detail: "Piezas paramétricas desde un boceto",
      icon: <Icons.Ruler size={20} />,
      action: props.onDesign,
    },
    {
      title: "Fabricar",
      detail: "Importar un modelo para imprimirlo en piezas",
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
          <img src={logo} alt="" class="w-full max-w-60 select-none" draggable={false} />
          <div class="text-center mt-3">
            <div class="text-2xl font-bold tracking-tight text-splash-text">Pinocchio</div>
            <div class="text-[11px] text-splash-muted">Escanear, reparar, animar y fabricar modelos 3D</div>
          </div>
        </div>

        <div class="flex-1 min-w-0 flex flex-col gap-4 p-6">
          <div class="space-y-0.5">
            <h2 class="text-base font-semibold text-text">¿Por dónde empezamos?</h2>
            <p class="text-xs text-text-muted">También puedes arrastrar un modelo 3D sobre el visor</p>
          </div>

          <div class="flex flex-col gap-1.5">
            <For each={choices()}>
              {(choice) => (
                <button
                  class={clsx(
                    "flex items-center gap-3 px-3 py-2.5 rounded-xl border text-left transition-colors",
                    "disabled:opacity-40 disabled:cursor-not-allowed",
                    choice.primary
                      ? "border-accent/50 bg-accent/10 hover:bg-accent/20"
                      : "border-transparent hover:bg-bg-lighter hover:border-border"
                  )}
                  disabled={!choice.action}
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

          <Show when={props.recovery}>
            {(r) => (
              <button
                class="mt-auto flex items-center gap-2 text-xs text-text-muted hover:text-text"
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
    </div>
  );
};
