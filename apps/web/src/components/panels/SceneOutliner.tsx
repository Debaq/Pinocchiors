import { Component, For, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { clsx } from "clsx";
import type { SceneNode } from "../../lib/scene-tree";
import { matchesOutliner, outlinerQuery } from "../../lib/outlinerFilter";
import * as Icons from "../icons";
import * as SketchIcons from "../icons/sketch";
import * as DesignIcons from "../icons/design";
import { t } from "../../lib/i18n";

export interface SceneOutlinerProps {
  tree: SceneNode;
  onToggleVisibility?: (nodeId: string) => void;
  /** `add`: con Ctrl, ⌘ o Mayús (suma o quita de la selección) */
  onSelectNode?: (nodeId: string, add?: boolean) => void;
  onDeleteNode?: (nodeId: string) => void;
  /** El puntero entra en una fila (`null` al salir): se destaca en el visor lo que se va a elegir */
  onHoverNode?: (nodeId: string | null) => void;
  /** «+» de un grupo (`addHint`) */
  onAddNode?: (groupId: string) => void;
}

const nodeIcon = (type: SceneNode["type"]) => {
  switch (type) {
    case "scene": return Icons.CubeFocus;
    case "mesh": return Icons.Cube;
    case "wireframe": return Icons.GridFour;
    case "skeleton": return Icons.Bone;
    case "bone": return Icons.Bone;
    case "quadmesh": return Icons.GridFour;
    case "grid": return Icons.GridFour;
    case "weights": return Icons.Eye;
    case "node": return Icons.TreeStructure;
    case "group": return Icons.Stack;
    case "plane": return Icons.Square;
    case "axis": return SketchIcons.Line;
    case "point": return SketchIcons.Point;
    case "sketch": return SketchIcons.Rect;
    case "feature": return Icons.Wrench;
    case "folder": return Icons.Folder;
    case "part": return Icons.Cube;
    case "drawing": return DesignIcons.Drawing;
    case "drawing_sheet": return Icons.FrameCorners;
    case "drawing_view": return Icons.Square;
    case "drawing_section": return Icons.Scissors;
    case "drawing_detail": return Icons.MagnifyingGlass;
    case "drawing_dim": return Icons.Ruler;
    case "drawing_sketch": return SketchIcons.Spline;
    default: return Icons.Cube;
  }
};

const OutlinerNode: Component<{
  node: SceneNode;
  depth: number;
  onToggleVisibility?: (nodeId: string) => void;
  /** `add`: con Ctrl, ⌘ o Mayús (suma o quita de la selección) */
  onSelectNode?: (nodeId: string, add?: boolean) => void;
  onDeleteNode?: (nodeId: string) => void;
  onHoverNode?: (nodeId: string | null) => void;
  onAddNode?: (groupId: string) => void;
  isExpanded: (node: SceneNode) => boolean;
  onToggleExpanded: (node: SceneNode) => void;
}> = (props) => {
  const expanded = () => props.isExpanded(props.node);
  // Borrar pide un segundo clic (no se puede deshacer); se olvida a los 3 s
  const [confirming, setConfirming] = createSignal(false);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;
  const onDelete = (e: MouseEvent) => {
    e.stopPropagation();
    clearTimeout(confirmTimer);
    if (confirming()) {
      setConfirming(false);
      props.onDeleteNode?.(props.node.id);
    } else {
      setConfirming(true);
      confirmTimer = setTimeout(() => setConfirming(false), 3000);
    }
  };
  onCleanup(() => clearTimeout(confirmTimer));
  const hasChildren = () => props.node.children.length > 0 || !!props.node.content;
  const NodeIcon = nodeIcon(props.node.type);
  const isGroup = props.node.type === "group";

  return (
    <div>
      <div
        class={clsx(
          "flex items-center gap-1 px-1.5 py-0.5 rounded-sm cursor-pointer group",
          "hover:bg-surface/30",
          props.node.selected && "bg-accent/20 border-l-2 border-accent"
        )}
        style={{ "padding-left": `${props.depth * 12 + 4}px` }}
        data-outliner-row={props.node.id}
        data-selected-row={props.node.selected && !isGroup ? "" : undefined}
        title={props.node.hint}
        onPointerEnter={() => props.onHoverNode?.(props.node.id)}
        onPointerLeave={() => props.onHoverNode?.(null)}
        onClick={(e) => (isGroup ? props.onToggleExpanded(props.node) : props.onSelectNode?.(props.node.id, e.ctrlKey || e.metaKey || e.shiftKey))}
      >
        {/* Expand arrow */}
        <Show
          when={hasChildren()}
          fallback={<span class="w-3.5" />}
        >
          <button
            class="w-3.5 h-3.5 flex items-center justify-center text-text-muted hover:text-text"
            onClick={(e) => { e.stopPropagation(); props.onToggleExpanded(props.node); }}
          >
            <Show when={expanded()} fallback={<Icons.CaretRight size={10} />}>
              <Icons.CaretDown size={10} />
            </Show>
          </button>
        </Show>

        {/* Icon (las piezas, con su color) */}
        <Show
          when={props.node.color}
          fallback={
            <NodeIcon size={12} class={clsx(
              props.node.type === "bone" && props.node.selected ? "text-purple" : "text-text-muted"
            )} />
          }
        >
          <span class="w-3 h-3 shrink-0 rounded-sm border border-border" style={{ background: props.node.color }} />
        </Show>

        {/* Label */}
        <span class={clsx(
          "flex-1 truncate",
          isGroup ? "text-[10px] font-semibold uppercase tracking-wide text-text-muted" : "text-xs",
          !isGroup && (props.node.error ? "text-red" : props.node.selected ? "text-accent font-medium" : "text-text"),
          (!props.node.visible || props.node.muted || props.node.struck) && "opacity-50",
          props.node.muted && "italic",
          props.node.struck && "line-through"
        )}>
          {props.node.label}
        </span>
        <Show when={isGroup && props.node.addHint && props.onAddNode}>
          <button
            class="w-4 h-4 flex items-center justify-center rounded-sm text-text-muted hover:text-text hover:bg-surface"
            title={props.node.addHint}
            aria-label={props.node.addHint}
            onClick={(e) => {
              e.stopPropagation();
              props.onAddNode?.(props.node.id);
            }}
          >
            <Icons.Plus size={11} />
          </button>
        </Show>
        <Show when={isGroup}>
          <span class="text-[10px] text-text-dim pr-1">{props.node.count ?? props.node.children.length}</span>
        </Show>

        {/* Borrar */}
        <Show when={props.node.deletable && props.onDeleteNode}>
          <button
            class={clsx(
              "h-4 flex items-center justify-center gap-0.5 rounded-sm transition-opacity",
              confirming()
                ? "px-1 bg-red/20 text-red text-[10px] font-medium"
                : "w-4 opacity-0 group-hover:opacity-100 text-text-muted hover:text-red"
            )}
            title={confirming() ? t("Clic de nuevo para borrar (no se puede deshacer)") : t("Borrar {name}", { name: props.node.label })}
            onClick={onDelete}
          >
            <Icons.Trash size={12} />
            <Show when={confirming()}>{t("¿Borrar?")}</Show>
          </button>
        </Show>

        {/* Visibility toggle: las capas ocultas lo muestran siempre */}
        <Show when={!props.node.readonly}>
          <button
            class={clsx(
              "w-4 h-4 flex items-center justify-center",
              props.node.visible && "opacity-0 group-hover:opacity-100",
              "transition-opacity",
              props.node.visible ? "text-text-muted hover:text-text" : "text-text-muted/40 hover:text-text-muted"
            )}
            onClick={(e) => { e.stopPropagation(); props.onToggleVisibility?.(props.node.id); }}
          >
            <Show when={props.node.visible} fallback={<Icons.EyeSlash size={12} />}>
              <Icons.Eye size={12} />
            </Show>
          </button>
        </Show>
      </div>

      {/* Contenido propio (el árbol de operaciones) */}
      <Show when={expanded() && props.node.content}>
        <div style={{ "padding-left": `${props.depth * 12 + 16}px` }}>{props.node.content}</div>
      </Show>

      {/* Children */}
      <Show when={expanded() && props.node.children.length > 0}>
        <For each={props.node.children}>
          {(child) => (
            <OutlinerNode
              node={child}
              depth={props.depth + 1}
              onToggleVisibility={props.onToggleVisibility}
              onSelectNode={props.onSelectNode}
              onDeleteNode={props.onDeleteNode}
              onHoverNode={props.onHoverNode}
              onAddNode={props.onAddNode}
              isExpanded={props.isExpanded}
              onToggleExpanded={props.onToggleExpanded}
            />
          )}
        </For>
      </Show>
    </div>
  );
};

export const SceneOutliner: Component<SceneOutlinerProps> = (props) => {
  // El árbol se rehace con cada cambio de la vista: lo abierto/cerrado se
  // recuerda por id para no perderlo
  const [openState, setOpenState] = createSignal<Record<string, boolean>>({});
  const isExpanded = (node: SceneNode) => openState()[node.id] ?? node.expanded;
  const onToggleExpanded = (node: SceneNode) =>
    setOpenState((prev) => ({ ...prev, [node.id]: !isExpanded(node) }));

  // Al cambiar la selección (p. ej. clic en el visor) se abren los padres de
  // lo seleccionado y se lo trae a la vista
  const selectedPaths = (node: SceneNode, path: string[], out: string[][]) => {
    if (node.selected && node.type !== "group") out.push(path);
    for (const c of node.children) selectedPaths(c, [...path, node.id], out);
  };
  createEffect(
    on(
      () => {
        const out: string[][] = [];
        selectedPaths(props.tree, [], out);
        return out;
      },
      (paths, prev) => {
        const key = (ps: string[][]) => ps.map((p) => p.join("/")).join("|");
        if (prev && key(paths) === key(prev)) return;
        if (!paths.length) return;
        const ids = new Set(paths.flat().filter((id) => id !== props.tree.id));
        setOpenState((st) => {
          const next = { ...st };
          for (const id of ids) next[id] = true;
          return next;
        });
        requestAnimationFrame(() =>
          document.querySelector("[data-outliner] [data-selected-row]")?.scrollIntoView({ block: "nearest" })
        );
      }
    )
  );

  // Con búsqueda: quedan los que coinciden y sus padres, abiertos; los grupos
  // con contenido propio (operaciones) filtran adentro
  const filter = (node: SceneNode): SceneNode | null => {
    if (node.content) return node;
    const children = node.children.map(filter).filter((c): c is SceneNode => !!c);
    if (node.type !== "group" && matchesOutliner(node.label)) return { ...node, children: node.children };
    return children.length ? { ...node, children, expanded: true } : null;
  };
  const roots = () => {
    const list = props.tree.children;
    if (!outlinerQuery().trim()) return list;
    return list.map(filter).filter((c): c is SceneNode => !!c);
  };
  const expandedNow = (node: SceneNode) => (outlinerQuery().trim() ? node.expanded || isExpanded(node) : isExpanded(node));

  return (
    <div class="space-y-0.5" data-outliner>
      <Show when={outlinerQuery().trim() && roots().length === 0}>
        <p class="px-2 py-1 text-xs text-text-dim">{t("Nada coincide con «{q}»", { q: outlinerQuery().trim() })}</p>
      </Show>
      <For each={roots()}>
        {(node) => (
          <OutlinerNode
            node={node}
            depth={0}
            onToggleVisibility={props.onToggleVisibility}
            onSelectNode={props.onSelectNode}
            onDeleteNode={props.onDeleteNode}
            onHoverNode={props.onHoverNode}
            onAddNode={props.onAddNode}
            isExpanded={expandedNow}
            onToggleExpanded={onToggleExpanded}
          />
        )}
      </For>
    </div>
  );
};
