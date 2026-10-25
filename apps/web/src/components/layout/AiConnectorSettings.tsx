import { Component, For, JSX, Show, createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";

/** Un cliente de IA y si tiene el conector (`mcp::clients::ClientStatus`) */
interface ClientStatus {
  id: string;
  name: string;
  installed: boolean;
  connected: boolean;
  /** Registrado con la ruta de otra versión de la app */
  outdated: boolean;
  config: string;
}

interface McpStatus {
  command: string;
  args: string[];
  clients: ClientStatus[];
  skill: boolean;
  skill_path: string;
}

type Download = "config" | "guide" | "skill";

const Section = (props: { title: string; children: JSX.Element }) => (
  <section class="space-y-3 pb-4 mb-4 border-b border-border last:border-0 last:mb-0 last:pb-0">
    <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{props.title}</h3>
    {props.children}
  </section>
);

const Hint = (props: { children: JSX.Element }) => <p class="text-xs text-text-muted leading-relaxed">{props.children}</p>;

/**
 * Configuración ▸ Conector para IA: la app como servidor MCP (`--mcp`) para
 * Claude, Cursor, Gemini o Codex, y lo que el agente necesita para usarla.
 */
export const AiConnectorSettings: Component = () => {
  const [status, setStatus] = createSignal<McpStatus>();
  const [busy, setBusy] = createSignal<string>();
  const [message, setMessage] = createSignal<{ text: string; error?: boolean }>();

  const run = async (key: string, job: () => Promise<McpStatus | void>, done?: string) => {
    setBusy(key);
    setMessage(undefined);
    try {
      const next = await job();
      if (next) setStatus(next);
      if (done) setMessage({ text: done });
    } catch (e) {
      setMessage({ text: tBackend(String(e)), error: true });
    } finally {
      setBusy(undefined);
    }
  };

  onMount(() => void run("load", () => invoke<McpStatus>("mcp_status")));

  const commandLine = () => {
    const s = status();
    return s ? [s.command, ...s.args].map((a) => (a.includes(" ") ? `"${a}"` : a)).join(" ") : "";
  };

  const toggle = (c: ClientStatus) => {
    const on = !c.connected || c.outdated;
    void run(
      c.id,
      () => invoke<McpStatus>("mcp_connect", { client: c.id, on }),
      on ? t("Listo: reinicia {name} para que vea el conector.", { name: c.name }) : t("Se quitó el conector de {name}.", { name: c.name }),
    );
  };

  const copy = (kind: Download) =>
    void run(`copy-${kind}`, async () => {
      await navigator.clipboard.writeText(await invoke<string>("mcp_download", { kind }));
    }, t("Copiado al portapapeles."));

  const download = (kind: Download) => {
    const name = { config: "pinocchio-mcp.json", guide: "pinocchio-guia-agente.md", skill: "SKILL.md" }[kind];
    const ext = kind === "config" ? "json" : "md";
    void run(`save-${kind}`, async () => {
      const path = await save({ defaultPath: name, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
      if (!path) return;
      await invoke("write_text_file", { path, contents: await invoke<string>("mcp_download", { kind }) });
      setMessage({ text: t("Guardado en {path}.", { path }) });
    });
  };

  const state = (c: ClientStatus) =>
    c.connected
      ? c.outdated
        ? { text: t("Conectado a otra versión de la app"), class: "text-warning" }
        : { text: t("Conectado"), class: "text-success" }
      : c.installed
        ? { text: t("No conectado"), class: "text-text-muted" }
        : { text: t("No instalado"), class: "text-text-dim" };

  return (
    <>
      <Section title={t("Conector MCP")}>
        <Hint>
          {t(
            "Con el conector, una IA (Claude, Cursor, Gemini, Codex…) usa Pinocchio: diseña piezas, arma ensambles, saca planos y exporta STEP o STL. Trabaja con su propio motor, sin abrir esta ventana, y guarda proyectos que abres aquí.",
          )}
        </Hint>
        <div class="space-y-1">
          <div class="text-xs text-text-muted">{t("Comando que lanza la IA")}</div>
          <div class="flex items-center gap-2">
            <code class="flex-1 min-w-0 truncate px-2 py-1 rounded bg-surface/40 border border-border text-[11px] font-mono text-text" title={commandLine()}>
              {commandLine() || "…"}
            </code>
            <Button size="sm" variant="ghost" title={t("Copiar el comando")} disabled={!status()} onClick={() => void navigator.clipboard.writeText(commandLine()).then(() => setMessage({ text: t("Copiado al portapapeles.") }))}>
              <Icons.Copy size={13} />
            </Button>
          </div>
        </div>
      </Section>

      <Section title={t("Conectar con una IA")}>
        <Hint>{t("Agrega el conector a la configuración del programa elegido (antes guarda una copia del archivo). Después reinicia ese programa.")}</Hint>
        <ul class="divide-y divide-border/60">
          <For each={status()?.clients ?? []}>
            {(c) => (
              <li class="flex items-center gap-3 py-2">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-text">{c.name}</div>
                  <div class={`text-xs ${state(c).class}`}>{state(c).text}</div>
                </div>
                <Button
                  size="sm"
                  variant={c.connected && !c.outdated ? "ghost" : "primary"}
                  disabled={busy() !== undefined}
                  title={c.config}
                  onClick={() => toggle(c)}
                >
                  {c.connected ? (c.outdated ? t("Actualizar") : t("Desconectar")) : t("Conectar")}
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Section>

      <Section title={t("Descargables para el agente")}>
        <Download
          title={t("Configuración (mcp.json)")}
          hint={t("Para pegar en cualquier otro cliente MCP.")}
          busy={busy() !== undefined}
          onCopy={() => copy("config")}
          onSave={() => download("config")}
        />
        <Download
          title={t("Guía para el agente")}
          hint={t("Convenciones, flujos de trabajo y cada herramienta con sus argumentos (Markdown). Sirve como AGENTS.md o para pegar en las instrucciones de la IA.")}
          busy={busy() !== undefined}
          onCopy={() => copy("guide")}
          onSave={() => download("guide")}
        />
        <div class="flex items-start gap-3">
          <div class="flex-1 min-w-0 space-y-0.5">
            <div class="text-sm text-text">{t("Skill de Claude Code")}</div>
            <Hint>
              {status()?.skill ? t("Instalada: Claude Code sabe cuándo y cómo usar Pinocchio.") : t("Le enseña a Claude Code cuándo y cómo usar Pinocchio.")}
            </Hint>
          </div>
          <Button size="sm" variant={status()?.skill ? "ghost" : "primary"} disabled={busy() !== undefined} title={status()?.skill_path} onClick={() => void run("skill", () => invoke<McpStatus>("mcp_skill", { on: !status()?.skill }), status()?.skill ? t("Se quitó la skill.") : t("Skill instalada en {path}.", { path: status()?.skill_path ?? "" }))}>
            {status()?.skill ? t("Quitar") : t("Instalar")}
          </Button>
          <Button size="sm" variant="ghost" title={t("Guardar…")} disabled={busy() !== undefined} onClick={() => download("skill")}>
            <Icons.Download size={13} />
          </Button>
        </div>
      </Section>

      <Show when={message()}>
        {(m) => <p class={`text-xs ${m().error ? "text-error" : "text-text-muted"}`} role="status">{m().text}</p>}
      </Show>
      <Hint>{t("Que la IA maneje esta misma ventana en vivo llegará más adelante.")}</Hint>
    </>
  );
};

/** Un descargable: nombre, para qué sirve, copiar y guardar */
const Download: Component<{ title: string; hint: string; busy: boolean; onCopy: () => void; onSave: () => void }> = (props) => (
  <div class="flex items-start gap-3">
    <div class="flex-1 min-w-0 space-y-0.5">
      <div class="text-sm text-text">{props.title}</div>
      <Hint>{props.hint}</Hint>
    </div>
    <Button size="sm" variant="ghost" title={t("Copiar")} disabled={props.busy} onClick={props.onCopy}>
      <Icons.Copy size={13} />
    </Button>
    <Button size="sm" variant="ghost" title={t("Guardar…")} disabled={props.busy} onClick={props.onSave}>
      <Icons.Download size={13} />
    </Button>
  </div>
);
