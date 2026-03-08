import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import type { SceneNode } from "../../lib/scene-tree";
import * as Icons from "../icons";

export interface SceneOutlinerProps {
  tree: SceneNode;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;
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
}> = (props) => {
  const [expanded, setExpanded] = createSignal(props.node.expanded);
  const hasChildren = () => props.node.children.length > 0;
  const NodeIcon = nodeIcon(props.node.type);

  return (
    <div>
      <div
        class={clsx(
          "flex items-center gap-1 px-1.5 py-0.5 rounded-sm cursor-pointer group",
          "hover:bg-current/30",
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
          props.node.selected ? "text-accent font-medium" : "text-text"
        )}>
          {props.node.label}
        </span>

        {/* Visibility toggle */}
        <button
          class={clsx(
            "w-4 h-4 flex items-center justify-center opacity-0 group-hover:opacity-100",
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
      <div class="flex items-center gap-1.5 px-2 pb-1.5">
        <Icons.SidebarSimple size={12} class="text-text-muted" />
        <span class="text-xs font-semibold text-text-muted uppercase tracking-wider">Outliner</span>
      </div>
      <div class="max-h-48 overflow-y-auto">
        <For each={props.tree.children}>
          {(node) => (
            <OutlinerNode
              node={node}
              depth={0}
              onToggleVisibility={props.onToggleVisibility}
              onSelectNode={props.onSelectNode}
            />
          )}
        </For>
      </div>
    </div>
  );
};
