import { Component, Show, For } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, Select } from "../ui";
import * as Icons from "../icons";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export interface RepairAnalysisConfig {
  checkNonManifold: boolean;
  checkSelfIntersections: boolean;
}

export interface RepairOptions {
  mergeDuplicates: boolean;
  removeDegenerates: boolean;
  fixNormals: boolean;
  orientOutward: boolean;
  fillHoles: boolean;
  holeFillMethod: string;
  fixNonManifold: boolean;
}

export interface MeshDiagnostics {
  boundary_loops: number;
  boundary_edges: number;
  duplicate_vertices: number;
  degenerate_faces: number;
  zero_area_faces: number;
  needle_faces: number;
  cap_faces: number;
  non_manifold_edges: number;
  non_manifold_vertices: number;
  normals_consistent: boolean;
  connected_components: number;
  is_closed: boolean;
  self_intersections: number;
  needs_repair: boolean;
  is_healthy: boolean;
}

export interface RepairResult {
  vertices_merged: number;
  faces_removed: number;
  faces_flipped: number;
  holes_filled: number;
  faces_added: number;
  non_manifold_fixed: number;
}

export interface RepairPanelProps {
  onAnalyze?: () => void;
  onRepair?: () => void;
  onUndo?: () => void;
  diagnostics?: MeshDiagnostics;
  repairResult?: RepairResult;
  canAnalyze?: boolean;
  canRepair?: boolean;
  canUndo?: boolean;
  isProcessing?: boolean;
  analysisConfig: RepairAnalysisConfig;
  onAnalysisConfigChange?: (config: RepairAnalysisConfig) => void;
  repairOptions: RepairOptions;
  onRepairOptionsChange?: (options: RepairOptions) => void;
}

// ═══════════════════════════════════════════════════════════════════════════
// COMPONENT
// ═══════════════════════════════════════════════════════════════════════════

export const RepairPanel: Component<RepairPanelProps> = (props) => {
  const diagBadges = () => {
    const d = props.diagnostics;
    if (!d) return [];
    return [
      { label: "Agujeros", value: d.boundary_loops, bad: d.boundary_loops > 0 },
      { label: "Duplicados", value: d.duplicate_vertices, bad: d.duplicate_vertices > 0 },
      { label: "Degeneradas", value: d.degenerate_faces, bad: d.degenerate_faces > 0 },
      { label: "Non-manifold", value: d.non_manifold_edges, bad: d.non_manifold_edges > 0 },
      { label: "Intersecciones", value: d.self_intersections, bad: d.self_intersections > 0 },
      { label: "Componentes", value: d.connected_components, bad: false },
    ];
  };

  return (
    <div class="space-y-4">
      {/* Análisis */}
      <div class="space-y-3">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Análisis</h4>

        <Checkbox
          checked={props.analysisConfig.checkSelfIntersections}
          onChange={(v) =>
            props.onAnalysisConfigChange?.({ ...props.analysisConfig, checkSelfIntersections: v })
          }
          label="Comprobar auto-intersecciones"
        />

        <Button
          onClick={props.onAnalyze}
          disabled={!props.canAnalyze || props.isProcessing}
          loading={props.isProcessing}
          variant="default"
          fullWidth
          size="sm"
          icon={<Icons.MagicWand size={14} />}
        >
          Analizar malla
        </Button>
      </div>

      {/* Resultados del análisis */}
      <Show when={props.diagnostics}>
        <div class="space-y-2">
          {/* Health indicator */}
          <div
            class={clsx(
              "flex items-center gap-2 px-3 py-2 rounded-md border text-xs font-medium",
              props.diagnostics!.is_healthy
                ? "bg-green/10 border-green/30 text-green"
                : "bg-red/10 border-red/30 text-red"
            )}
          >
            <Show when={props.diagnostics!.is_healthy} fallback={<Icons.X size={14} />}>
              <Icons.Check size={14} />
            </Show>
            {props.diagnostics!.is_healthy ? "Malla saludable" : "Requiere reparación"}
          </div>

          {/* Badges grid */}
          <div class="grid grid-cols-2 gap-1.5">
            <For each={diagBadges()}>
              {(badge) => (
                <div
                  class={clsx(
                    "flex justify-between items-center px-2 py-1.5 rounded text-xs border",
                    badge.bad
                      ? "bg-red/5 border-red/20 text-red"
                      : "bg-current/20 border-border text-text-muted"
                  )}
                >
                  <span>{badge.label}</span>
                  <span class="font-mono font-semibold">{badge.value}</span>
                </div>
              )}
            </For>
          </div>

          {/* Extra info */}
          <div class="space-y-1 px-1">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Cerrada</span>
              <span class={props.diagnostics!.is_closed ? "text-green" : "text-red"}>
                {props.diagnostics!.is_closed ? "Sí" : "No"}
              </span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Normales consistentes</span>
              <span class={props.diagnostics!.normals_consistent ? "text-green" : "text-red"}>
                {props.diagnostics!.normals_consistent ? "Sí" : "No"}
              </span>
            </div>
          </div>
        </div>
      </Show>

      {/* Opciones de reparación */}
      <Show when={props.diagnostics && props.diagnostics!.needs_repair}>
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Reparación</h4>

          <div class="space-y-2">
            <Checkbox
              checked={props.repairOptions.mergeDuplicates}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, mergeDuplicates: v })}
              label="Fusionar duplicados"
            />
            <Checkbox
              checked={props.repairOptions.removeDegenerates}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, removeDegenerates: v })}
              label="Eliminar degeneradas"
            />
            <Checkbox
              checked={props.repairOptions.fixNormals}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, fixNormals: v })}
              label="Corregir normales"
            />
            <Checkbox
              checked={props.repairOptions.orientOutward}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, orientOutward: v })}
              label="Orientar normales"
            />
            <Checkbox
              checked={props.repairOptions.fillHoles}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, fillHoles: v })}
              label="Rellenar agujeros"
            />
            <Checkbox
              checked={props.repairOptions.fixNonManifold}
              onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, fixNonManifold: v })}
              label="Corregir non-manifold"
            />
          </div>

          <Select
            label="Método de relleno"
            options={[
              { value: "ear_clipping", label: "Ear Clipping (rápido)" },
              { value: "liepa", label: "Liepa (alta calidad)" },
            ]}
            value={props.repairOptions.holeFillMethod}
            onChange={(v) => props.onRepairOptionsChange?.({ ...props.repairOptions, holeFillMethod: v })}
          />

          <Button
            onClick={props.onRepair}
            disabled={!props.canRepair || props.isProcessing}
            loading={props.isProcessing}
            variant="primary"
            fullWidth
            size="sm"
            icon={<Icons.Wrench size={14} />}
          >
            Reparar todo
          </Button>
        </div>
      </Show>

      {/* Resultado de reparación */}
      <Show when={props.repairResult}>
        <div class="space-y-2 p-3 rounded-md bg-green/5 border border-green/20">
          <p class="text-xs font-semibold text-green">Reparación completada</p>
          <div class="space-y-1">
            <Show when={props.repairResult!.vertices_merged > 0}>
              <p class="text-xs text-text-muted">
                {props.repairResult!.vertices_merged} vértices fusionados
              </p>
            </Show>
            <Show when={props.repairResult!.faces_removed > 0}>
              <p class="text-xs text-text-muted">
                {props.repairResult!.faces_removed} caras eliminadas
              </p>
            </Show>
            <Show when={props.repairResult!.faces_flipped > 0}>
              <p class="text-xs text-text-muted">
                {props.repairResult!.faces_flipped} normales invertidas
              </p>
            </Show>
            <Show when={props.repairResult!.holes_filled > 0}>
              <p class="text-xs text-text-muted">
                {props.repairResult!.holes_filled} agujeros rellenados ({props.repairResult!.faces_added} caras)
              </p>
            </Show>
          </div>
        </div>

        <Show when={props.canUndo}>
          <Button
            onClick={props.onUndo}
            variant="danger"
            fullWidth
            size="sm"
            icon={<Icons.ArrowCounterClockwise size={14} />}
          >
            Deshacer reparación
          </Button>
        </Show>
      </Show>
    </div>
  );
};
