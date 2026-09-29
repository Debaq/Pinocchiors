import { Component, For, Show, createEffect, createMemo } from "solid-js";
import { clsx } from "clsx";
import type { HistoryNode } from "../../lib/history";
import * as Icons from "../icons";

export interface HistoryPanelProps {
  nodes: readonly HistoryNode[];
  current: number;
  canReach: (id: number) => boolean;
  onGoTo: (id: number) => void;
}

interface Row {
  node: HistoryNode;
  depth: number;
  /** pasado (hasta el actual), futuro (lo que rehace Ctrl+Shift+Z) o rama aparte */
  kind: "past" | "current" | "future" | "branch";
}

/**
 * Árbol del historial, del más viejo al más nuevo. La línea principal es la
 * que pasa por el estado actual (y sigue por lo que rehace Rehacer); las
 * otras ramas van sangradas antes de continuar. Clic en un paso lleva ahí.
 */
export const HistoryPanel: Component<HistoryPanelProps> = (props) => {
  let listRef: HTMLDivElement | undefined;

  const rows = createMemo(() => {
    const nodes = props.nodes;
    const onPath = new Set<number>();
    for (let n: number | null = props.current; n !== null; n = nodes[n].parent) onPath.add(n);
    const out: Row[] = [];
    // Pila explícita: el historial puede ser largo
    const stack: { id: number; depth: number; branch: boolean }[] = [{ id: 0, depth: 0, branch: false }];
    while (stack.length > 0) {
      const { id, depth, branch } = stack.pop()!;
      const node = nodes[id];
      const kind = id === props.current ? "current" : onPath.has(id) ? "past" : branch ? "branch" : "future";
      out.push({ node, depth, kind });
      if (node.children.length === 0) continue;
      const main =
        node.children.find((c) => onPath.has(c)) ?? node.redoChild ?? node.children[node.children.length - 1];
      // Se apila al revés: primero salen las ramas (sangradas), después la principal
      stack.push({ id: main, depth, branch });
      for (const c of [...node.children].reverse()) {
        if (c !== main) stack.push({ id: c, depth: depth + 1, branch: true });
      }
    }
    return out;
  });

  const branches = () => props.nodes.filter((n) => n.children.length > 1).length;

  // El actual a la vista
  createEffect(() => {
    props.current;
    queueMicrotask(() => listRef?.querySelector("[data-current]")?.scrollIntoView({ block: "nearest" }));
  });

  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div class="space-y-2">
      <p class="text-xs text-text-muted leading-relaxed">
        Clic en un paso para volver a él. Si deshaces y haces otra cosa se abre una rama y lo anterior queda
        acá. Los hitos (<Icons.Lock size={10} class="inline -mt-0.5" />) todavía no se pueden deshacer.
        <Show when={branches() > 0}> {branches()} {branches() === 1 ? "bifurcación" : "bifurcaciones"}.</Show>
      </p>
      <div ref={listRef} class="rounded-md border border-border py-1 max-h-[60vh] overflow-y-auto">
        <For each={rows()}>
          {(row) => {
            const reachable = () => props.canReach(row.node.id);
            return (
              <button
                data-current={row.kind === "current" ? "" : undefined}
                class={clsx(
                  "w-full flex items-center gap-1.5 pr-2 py-0.5 text-left text-xs",
                  row.kind === "current" && "bg-accent/20 text-accent font-medium",
                  row.kind === "past" && "text-text",
                  row.kind === "future" && "text-text-muted",
                  row.kind === "branch" && "text-text-muted/70",
                  reachable() && row.kind !== "current" ? "hover:bg-surface/40" : "cursor-default"
                )}
                style={{ "padding-left": `${8 + row.depth * 12}px` }}
                title={reachable() ? undefined : "Hay un hito en el medio: no se puede volver hasta acá"}
                onClick={() => row.kind !== "current" && reachable() && props.onGoTo(row.node.id)}
              >
                <Show when={row.depth > 0}>
                  <span class="text-text-muted/50">└</span>
                </Show>
                <Show
                  when={row.node.milestone}
                  fallback={
                    <span
                      class={clsx(
                        "w-1.5 h-1.5 rounded-full shrink-0",
                        row.kind === "current" ? "bg-accent" : row.kind === "past" ? "bg-text-muted" : "border border-text-muted"
                      )}
                    />
                  }
                >
                  <Icons.Lock size={10} class="shrink-0" />
                </Show>
                <span class="flex-1 truncate">{row.node.description}</span>
                <span class="text-[10px] font-mono text-text-muted shrink-0">{time(row.node.time)}</span>
              </button>
            );
          }}
        </For>
      </div>
    </div>
  );
};
