import { Component, Show, createEffect, createResource, createSignal, on, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";

/** Espejo de apps/desktop/src/printer_control.rs */
interface Camera {
  name: string;
  stream_url: string;
  snapshot_url: string;
  /** "mjpeg": se puede mostrar leyendo fotos; otro ("webrtc"): cifrada/nube, no se puede */
  kind: string;
}

const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));

/**
 * La cámara de la impresora en la app. Lee fotos JPEG por el backend (`printer_snapshot`) y
 * las muestra seguidas — así anda sin problemas de contenido mixto ni CORS en el webview.
 * Solo sirve con cámaras MJPEG (K1 rooteada, Klipper con cámara); las cifradas (K1 de
 * fábrica 1.3.5.22) no se pueden ver local.
 */
export const PrinterCamera: Component<{
  conn: () => Record<string, unknown> | undefined;
  /** "moonraker" | "creality" | "flashforge" */
  printerKind: string;
}> = (props) => {
  const [cameras, { refetch }] = createResource(
    () => JSON.stringify(props.conn() ?? {}),
    async () => {
      const c = props.conn();
      if (!c) return [] as Camera[];
      return invoke<Camera[]>("printer_cameras", { conn: c }).catch(() => [] as Camera[]);
    },
  );
  const [on_, setOn] = createSignal(true);
  const [frame, setFrame] = createSignal<string>();
  const [error, setError] = createSignal<string>();
  const [fps, setFps] = createSignal(0);

  const camera = () => cameras()?.find((c) => c.kind === "mjpeg");
  const blocked = () => (cameras()?.length ?? 0) > 0 && !camera();

  // Bucle de fotos: pide una, la muestra, pide la siguiente (sin encimarse)
  let lastUrl: string | undefined;
  let stamps: number[] = [];
  createEffect(
    on([camera, on_], () => {
      let alive = true;
      const cam = camera();
      onCleanup(() => {
        alive = false;
        if (lastUrl) URL.revokeObjectURL(lastUrl);
        lastUrl = undefined;
        stamps = [];
        setFps(0);
      });
      if (!cam || !on_()) return;
      const tick = async () => {
        if (!alive) return;
        const c = props.conn();
        if (!c) return;
        const t0 = performance.now();
        try {
          const buf = await invoke<ArrayBuffer>("printer_snapshot", { conn: c, url: cam.snapshot_url });
          if (!alive) return;
          const url = URL.createObjectURL(new Blob([buf], { type: "image/jpeg" }));
          if (lastUrl) URL.revokeObjectURL(lastUrl);
          lastUrl = url;
          setFrame(url);
          setError(undefined);
          // Cuadros por segundo (promedio de los últimos ~2 s)
          const now = performance.now();
          stamps = [...stamps, now].filter((s) => now - s < 2000);
          setFps(stamps.length > 1 ? (stamps.length - 1) / ((now - stamps[0]) / 1000) : 0);
          // ~4 fps como tope; el límite real lo pone la red
          const wait = Math.max(0, 250 - (performance.now() - t0));
          setTimeout(() => void tick(), wait);
        } catch (e) {
          if (!alive) return;
          setError(errText(e));
          setFps(0);
          setTimeout(() => void tick(), 3000);
        }
      };
      void tick();
    }),
  );

  const saveFrame = () => {
    const url = frame();
    if (!url) return;
    const a = document.createElement("a");
    a.href = url;
    a.download = `camara-${new Date().toISOString().replace(/[:.]/g, "-")}.jpg`;
    a.click();
  };

  return (
    <div>
      <div class="space-y-1.5">
        <div class="flex items-center justify-between">
          <span class="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-text-muted">
            <Icons.Eye size={14} />
            {t("Cámara")}
          </span>
          <Show when={camera()}>
            <div class="flex items-center gap-2">
              <Show when={on_() && fps() > 0}>
                <span class="font-mono text-[10px] text-text-dim">{fps().toFixed(1)} fps</span>
              </Show>
              <button class="text-[11px] text-accent hover:underline" onClick={() => setOn(!on_())}>
                {on_() ? t("Pausar") : t("Ver")}
              </button>
            </div>
          </Show>
        </div>

        <Show when={camera()}>
          <div class="relative overflow-hidden rounded border border-border bg-black" style={{ "aspect-ratio": "4 / 3" }}>
            <Show
              when={frame()}
              fallback={
                <div class="absolute inset-0 flex items-center justify-center text-[11px] text-text-dim">
                  {on_() ? t("Conectando con la cámara...") : t("En pausa")}
                </div>
              }
            >
              <img src={frame()} alt={t("Cámara")} class="h-full w-full object-contain" />
            </Show>
            <Show when={on_() && frame()}>
              <button class="absolute bottom-1 right-1 rounded bg-black/50 p-1 text-white hover:bg-black/70" title={t("Guardar foto")} onClick={saveFrame}>
                <Icons.Download size={14} />
              </button>
            </Show>
          </div>
          <Show when={error()}>
            <p class="text-[11px] text-warning">{error()}</p>
          </Show>
        </Show>

        <Show when={blocked()}>
          <p class="text-[11px] text-warning">{t("La cámara de esta impresora está cifrada por el firmware (solo se ve por la app de Creality o la nube). Para verla aquí habría que rootearla.")}</p>
        </Show>

        <Show when={!cameras.loading && (cameras()?.length ?? 0) === 0}>
          <p class="text-[11px] text-text-dim">
            <Show
              when={props.printerKind === "creality"}
              fallback={t("No se encontró cámara. Enchufa una cámara USB (UVC) a la impresora y actívala.")}
            >
              {t("No se encontró cámara local. Si enchufaste una, con el firmware de fábrica (1.3.5.22) la cámara queda cifrada y no se ve aquí.")}
            </Show>
            {" "}
            <button class="text-accent hover:underline" onClick={() => void refetch()}>
              {t("Reintentar")}
            </button>
          </p>
        </Show>
      </div>
    </div>
  );
};
