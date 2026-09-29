import { Component, For, Show, createSignal, onCleanup } from "solid-js";
import { clsx } from "clsx";
import type { SceneNode } from "../../lib/scene-tree";
import * as Icons from "../icons";

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
    default: return Icons.Cube;
  }
};

const OutlinerNode: Component<{
  node: SceneNode;
  depth: number;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
}> = (props) => {
  const [expanded, setExpanded] = createSignal(props.node.expanded);
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

  return (
    <div>
      <div
        class={clsx(
          "flex items-center gap-1 px-1.5 py-0.5 rounded-sm cursor-pointer group",
          "hover:bg-surface/30",
          props.node.selected && "bg-accent/20 border-l-2 border-accent"
        )}
        style={{ "padding-left": `${props.depth * 12 + 4}px` }}
        onClick={() => props.onSelectNode?.(props.node.id)}
      >
        {/* Expand arrow */}
        <Show
          when={hasChildren()}
          fallback={<span class="w-3.5" />}
        >
          <button
            class="w-3.5 h-3.5 flex items-center justify-center text-text-muted hover:text-text"
            onClick={(e) => { e.stopPropagation(); setExpanded(!expanded()); }}
          >
            <Show when={expanded()} fallback={<Icons.CaretRight size={10} />}>
              <Icons.CaretDown size={10} />
            </Show>
          </button>
        </Show>

        {/* Icon */}
        <NodeIcon size={12} class={clsx(
          props.node.type === "bone" && props.node.selected ? "text-purple" : "text-text-muted"
        )} />

        {/* Label */}
        <span class={clsx(
          "text-xs flex-1 truncate",
          props.node.selected ? "text-accent font-medium" : "text-text",
          !props.node.visible && "opacity-50"
        )}>
          {props.node.label}
        </span>

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
            />
          )}
        </For>
      </Show>
    </div>
  );
};

export const SceneOutliner: Component<SceneOutlinerProps> = (props) => {
  return (
    <div class="space-y-0.5">
      <For each={props.tree.children}>
        {(node) => (
          <OutlinerNode
            node={node}
            depth={0}
            onToggleVisibility={props.onToggleVisibility}
            onSelectNode={props.onSelectNode}
            onDeleteNode={props.onDeleteNode}
          />
        )}
      </For>
    </div>
  );
};
