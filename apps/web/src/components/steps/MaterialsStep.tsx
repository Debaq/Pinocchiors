import { Component, For, Show } from "solid-js";
import { Button, Panel } from "../ui";
import * as Icons from "../icons";
import type { TextureSelection } from "../layout/TextureEditor";
import { cssColor, formatBytes, materialChannels, Row, type SceneStructure } from "./StructureStep";
import { t } from "../../lib/i18n";

export interface MaterialsStepProps {
  structure?: SceneStructure;
  /** URL (blob) de cada textura, para las miniaturas */
  textureUrls?: string[];
  /** Clic en una textura: abrirla en el editor de texturas */
  onOpenTexture?: (selection: TextureSelection) => void;
  /** El panel lateral de materiales está abierto */
  panelOpen?: boolean;
  onTogglePanel?: () => void;
}

/** Materiales y texturas del modelo: el resumen y las imágenes; se editan en el panel lateral */
export const MaterialsStep: Component<MaterialsStepProps> = (props) => {
  const thumb = (index: number | null) => (index === null ? undefined : props.textureUrls?.[index]);

  return (
    <div class="space-y-4">
      <h3 class="text-sm font-semibold text-text">{t("Materiales")}</h3>
      <Show
        when={props.structure}
        fallback={<p class="text-xs text-text-muted">{t("Importa un modelo para ver sus materiales y texturas.")}</p>}
      >
        {(structure) => (
          <>
            <p class="text-xs text-text-muted leading-relaxed">
              {t("Color, brillo, glow y opacidad se editan en el panel lateral. Doble clic sobre una parte del modelo para cambiar solo su material. Aquí están las texturas: un clic abre el editor 2D/3D.")}
            </p>
            <Button class="w-full" onClick={() => props.onTogglePanel?.()}>
              {props.panelOpen ? t("Cerrar el panel de materiales") : t("Abrir el panel de materiales")}
            </Button>
            <div class="space-y-1.5">
              <Row label={t("Materiales")} value={structure().materials.length} />
              <Row
                label={t("Texturas")}
                value={`${structure().textures.length} (${formatBytes(structure().textures.reduce((a, tex) => a + tex.bytes, 0))})`}
              />
            </div>

            <Show when={structure().materials.some((m) => materialChannels(m).some(([, tex]) => tex !== null))}>
              <Panel title={t("Texturas por material")} icon={<Icons.Sliders size={14} />} defaultOpen>
                <div class="space-y-3 pt-1">
                  <For each={structure().materials}>
                    {(m, i) => (
                      <Show when={materialChannels(m).some(([, tex]) => tex !== null)}>
                        <div class="space-y-1.5">
                          <div class="flex items-center gap-2">
                            <span class="w-4 h-4 rounded border border-border shrink-0" style={{ background: cssColor(m.base_color) }} />
                            <span class="text-xs text-text font-medium truncate">{m.name || t("material {i}", { i: i() })}</span>
                          </div>
                          <div class="grid grid-cols-3 gap-1.5">
                            <For each={materialChannels(m).filter(([, tex]) => tex !== null)}>
                              {([label, tex, slot]) => (
                                <button
                                  class="space-y-0.5 text-left group"
                                  title={t("Abrir en el editor de texturas")}
                                  onClick={() => props.onOpenTexture?.({ material: i(), slot })}
                                >
                                  <Show when={thumb(tex)} fallback={<div class="aspect-square rounded bg-bg-lighter" />}>
                                    <img
                                      src={thumb(tex)}
                                      class="aspect-square w-full object-cover rounded border border-border group-hover:border-accent transition-colors"
                                    />
                                  </Show>
                                  <div class="text-[10px] text-text-muted truncate group-hover:text-text">{label}</div>
                                </button>
                              )}
                            </For>
                          </div>
                        </div>
                      </Show>
                    )}
                  </For>
                </div>
              </Panel>
            </Show>

            <Show when={structure().textures.length > 0}>
              <Panel title={t("Texturas ({n})", { n: structure().textures.length })} icon={<Icons.Checkerboard size={14} />} defaultOpen={false}>
                <div class="space-y-2 pt-1">
                  <For each={structure().textures}>
                    {(tex, i) => (
                      <button
                        class="flex gap-2 items-center w-full text-left rounded hover:bg-surface/40 transition-colors"
                        title={t("Abrir en el editor de texturas")}
                        onClick={() => {
                          // Primer material y mapa que usan esta textura
                          for (const [material, m] of structure().materials.entries()) {
                            const channel = materialChannels(m).find(([, index]) => index === i());
                            if (channel) return props.onOpenTexture?.({ material, slot: channel[2] });
                          }
                        }}
                      >
                        <Show when={thumb(i())} fallback={<div class="w-10 h-10 rounded bg-bg-lighter shrink-0" />}>
                          <img src={thumb(i())} class="w-10 h-10 object-cover rounded border border-border shrink-0" />
                        </Show>
                        <div class="min-w-0 flex-1">
                          <div class="text-xs text-text truncate">{tex.name || t("textura {i}", { i: i() })}</div>
                          <div class="text-[10px] text-text-muted">
                            {tex.format} · {tex.width}×{tex.height} · {formatBytes(tex.bytes)}
                          </div>
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </Panel>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
};
