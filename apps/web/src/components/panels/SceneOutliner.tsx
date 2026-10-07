import { Component, For, Show, createSignal, onCleanup } from "solid-js";
import { clsx } from "clsx";
import type { SceneNode } from "../../lib/scene-tree";
import * as Icons from "../icons";
import * as SketchIcons from "../icons/sketch";

export interface SceneOutlinerProps {
  tree: SceneNode;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
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
    default: return Icons.Cube;
  }
};

const OutlinerNode: Component<{
  node: SceneNode;
  depth: number;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
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
  const hasChildren = () => props.node.children.length > 0;
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
        title={props.node.hint}
        onClick={() => (isGroup ? props.onToggleExpanded(props.node) : props.onSelectNode?.(props.node.id))}
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
          (!props.node.visible || props.node.muted) && "opacity-50",
          props.node.muted && "italic"
        )}>
          {props.node.label}
        </span>
        <Show when={isGroup}>
          <span class="text-[10px] text-text-dim pr-1">{props.node.children.length}</span>
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
            title={confirming() ? "Clic de nuevo para borrar (no se puede deshacer)" : `Borrar ${props.node.label}`}
            onClick={onDelete}
          >
            <Icons.Trash size={12} />
            <Show when={confirming()}>¿Borrar?</Show>
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

      {/* Children */}
      <Show when={expanded() && hasChildren()}>
        <For each={props.node.children}>
          {(child) => (
            <OutlinerNode
              node={child}
              depth={props.depth + 1}
              onToggleVisibility={props.onToggleVisibility}
              onSelectNode={props.onSelectNode}
              onDeleteNode={props.onDeleteNode}
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

  return (
    <div class="space-y-0.5" data-outliner>
      <For each={props.tree.children}>
        {(node) => (
          <OutlinerNode
            node={node}
            depth={0}
            onToggleVisibility={props.onToggleVisibility}
            onSelectNode={props.onSelectNode}
            onDeleteNode={props.onDeleteNode}
            isExpanded={isExpanded}
            onToggleExpanded={onToggleExpanded}
          />
        )}
      </For>
    </div>
  );
};
