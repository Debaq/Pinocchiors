import { Component, For, Match, Switch } from "solid-js";
import { Dynamic } from "solid-js/web";
import { type Block, type Inline, parseMarkdown } from "../../lib/markdown";

/** Enlaces de la IA: solo http(s), en el navegador del sistema (nunca dentro de la ventana) */
const openLink = (e: MouseEvent, href: string) => {
  e.preventDefault();
  if (!/^https?:\/\//i.test(href)) return;
  void import("@tauri-apps/plugin-shell").then((m) => m.open(href)).catch(() => window.open(href, "_blank", "noopener"));
};

const Inlines: Component<{ nodes: Inline[] }> = (props) => (
  <For each={props.nodes}>
    {(n) => (
      <Switch>
        <Match when={n.t === "text" && n}>{(x) => x().v}</Match>
        <Match when={n.t === "code" && n}>{(x) => <code class="px-1 py-px rounded bg-surface/70 font-mono text-[0.88em] break-words">{x().v}</code>}</Match>
        <Match when={n.t === "b" && n}>{(x) => <strong class="font-semibold text-text">{<Inlines nodes={x().c} />}</strong>}</Match>
        <Match when={n.t === "i" && n}>{(x) => <em class="italic">{<Inlines nodes={x().c} />}</em>}</Match>
        <Match when={n.t === "a" && n}>
          {(x) => (
            <a class="text-accent underline underline-offset-2 cursor-pointer" href={x().href} title={x().href} onClick={(e) => openLink(e, x().href)}>
              <Inlines nodes={x().c} />
            </a>
          )}
        </Match>
      </Switch>
    )}
  </For>
);

const HEADING = ["text-base", "text-[15px]", "text-sm", "text-sm", "text-sm", "text-sm"];

const Blocks: Component<{ nodes: Block[] }> = (props) => (
  <For each={props.nodes}>
    {(b) => (
      <Switch>
        <Match when={b.t === "p" && b}>{(x) => <p class="whitespace-pre-wrap">{<Inlines nodes={x().c} />}</p>}</Match>
        <Match when={b.t === "h" && b}>
          {(x) => (
            <Dynamic component={`h${Math.min(x().level, 6)}`} class={`${HEADING[x().level - 1]} font-semibold text-text pt-1`}>
              <Inlines nodes={x().c} />
            </Dynamic>
          )}
        </Match>
        <Match when={b.t === "code" && b}>
          {(x) => <pre class="rounded border border-border bg-bg-darker px-2.5 py-2 font-mono text-xs leading-snug overflow-x-auto whitespace-pre">{x().v}</pre>}
        </Match>
        <Match when={b.t === "list" && b}>
          {(x) => (
            <Dynamic
              component={x().ordered ? "ol" : "ul"}
              start={x().ordered && x().start !== 1 ? x().start : undefined}
              class={`pl-5 space-y-1 marker:text-text-muted ${x().ordered ? "list-decimal" : "list-disc"}`}
            >
              <For each={x().items}>
                {(item) => (
                  <li class="pl-0.5 space-y-1">
                    <Blocks nodes={item} />
                  </li>
                )}
              </For>
            </Dynamic>
          )}
        </Match>
        <Match when={b.t === "quote" && b}>
          {(x) => (
            <blockquote class="border-l-2 border-border pl-3 text-text-muted space-y-2">
              <Blocks nodes={x().c} />
            </blockquote>
          )}
        </Match>
        <Match when={b.t === "hr"}>
          <hr class="border-border" />
        </Match>
        <Match when={b.t === "table" && b}>
          {(x) => (
            <div class="overflow-x-auto">
              <table class="border-collapse text-xs">
                <thead>
                  <tr>
                    <For each={x().head}>
                      {(c, i) => (
                        <th class="border border-border bg-surface/40 px-2 py-1 font-semibold text-text" style={{ "text-align": x().align[i()] ?? "left" }}>
                          <Inlines nodes={c} />
                        </th>
                      )}
                    </For>
                  </tr>
                </thead>
                <tbody>
                  <For each={x().rows}>
                    {(row) => (
                      <tr>
                        <For each={row}>
                          {(c, i) => (
                            <td class="border border-border px-2 py-1 align-top" style={{ "text-align": x().align[i()] ?? "left" }}>
                              <Inlines nodes={c} />
                            </td>
                          )}
                        </For>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          )}
        </Match>
      </Switch>
    )}
  </For>
);

/** Respuesta de la IA con formato (Markdown) */
export const Markdown: Component<{ text: string; class?: string }> = (props) => (
  <div class={`space-y-2 ${props.class ?? ""}`}>
    <Blocks nodes={parseMarkdown(props.text)} />
  </div>
);
