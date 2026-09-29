import { Component, Show, For } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput } from "../ui";
import * as Icons from "../icons";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export interface RepairAnalysisConfig {
  checkSelfIntersections: boolean;
}

export interface RepairOptions {
  mergeDuplicates: boolean;
  removeDegenerates: boolean;
  fixNormals: boolean;
  fixNonManifold: boolean;
  orientOutward: boolean;
  removeSmallComponents: boolean;
  fillHoles: boolean;
  /** Máximo de aristas de un agujero a rellenar (0 = sin límite) */
  maxHoleEdges: number;
  /** Refinar y suavizar los parches de relleno */
  refineFill: boolean;
}

/** Espejo de `pinocchio_repair::MeshDiagnostics` + veredictos */
export interface MeshDiagnostics {
  num_vertices: number;
  num_faces: number;
  boundary_loops: number;
  boundary_edges: number;
  duplicate_vertices: number;
  unreferenced_vertices: number;
  invalid_faces: number;
  duplicate_faces: number;
  degenerate_faces: number;
  needle_faces: number;
  cap_faces: number;
  non_manifold_edges: number;
  non_manifold_vertices: number;
  inconsistent_edges: number;
  normals_consistent: boolean;
  normals_outward: boolean | null;
  connected_components: number;
  is_closed: boolean;
  is_manifold: boolean;
  self_intersections: number;
  area: number;
  volume: number;
  needs_repair: boolean;
  is_healthy: boolean;
}

/** Espejo de `pinocchio_repair::RepairSummary` */
export interface RepairResult {
  invalid_faces_removed: number;
  vertices_merged: number;
  degenerate_fixed: number;
  duplicate_faces_removed: number;
  faces_removed: number;
  faces_flipped: number;
  non_manifold_fixed: number;
  components_removed: number;
  holes_filled: number;
  holes_skipped: number;
  faces_added: number;
  vertices_added: number;
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

const formatNumber = (n: number) => n.toLocaleString("es-CL", { maximumSignificantDigits: 4 });

export const RepairPanel: Component<RepairPanelProps> = (props) => {
  const d = () => props.diagnostics;

  const diagBadges = () => {
    const diag = d();
    if (!diag) return [];
    return [
      { label: "Agujeros", value: diag.boundary_loops, bad: diag.boundary_loops > 0 },
      { label: "Duplicados", value: diag.duplicate_vertices, bad: diag.duplicate_vertices > 0 },
      { label: "Degeneradas", value: diag.degenerate_faces, bad: diag.degenerate_faces > 0 },
      { label: "Caras repetidas", value: diag.duplicate_faces, bad: diag.duplicate_faces > 0 },
      { label: "Aristas non-manifold", value: diag.non_manifold_edges, bad: diag.non_manifold_edges > 0 },
      { label: "Vértices non-manifold", value: diag.non_manifold_vertices, bad: diag.non_manifold_vertices > 0 },
      { label: "Normales volteadas", value: diag.inconsistent_edges, bad: diag.inconsistent_edges > 0 },
      { label: "Inválidas", value: diag.invalid_faces, bad: diag.invalid_faces > 0 },
      { label: "Intersecciones", value: diag.self_intersections, bad: diag.self_intersections > 0 },
      { label: "Piezas", value: diag.connected_components, bad: false },
    ];
  };

  const yesNo = (value: boolean) => (
    <span class={value ? "text-green" : "text-red"}>{value ? "Sí" : "No"}</span>
  );

  const set = (patch: Partial<RepairOptions>) =>
    props.onRepairOptionsChange?.({ ...props.repairOptions, ...patch });

  const resultLines = () => {
    const r = props.repairResult;
    if (!r) return [];
    return [
      [r.vertices_merged, "vértices duplicados soldados"],
      [r.degenerate_fixed, "caras degeneradas corregidas"],
      [r.duplicate_faces_removed, "caras repetidas eliminadas"],
      [r.invalid_faces_removed, "caras inválidas eliminadas"],
      [r.non_manifold_fixed, "vértices separados (non-manifold)"],
      [r.faces_flipped, "caras reorientadas"],
      [r.components_removed, "piezas sueltas eliminadas"],
    ].filter(([n]) => (n as number) > 0) as [number, string][];
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
          label="Comprobar auto-intersecciones (lento)"
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
      <Show when={d()}>
        <div class="space-y-2">
          <div
            class={clsx(
              "flex items-center gap-2 px-3 py-2 rounded-md border text-xs font-medium",
              d()!.is_healthy
                ? "bg-green/10 border-green/30 text-green"
                : d()!.needs_repair
                  ? "bg-red/10 border-red/30 text-red"
                  : "bg-yellow/10 border-yellow/30 text-yellow"
            )}
          >
            <Show when={d()!.is_healthy} fallback={<Icons.X size={14} />}>
              <Icons.Check size={14} />
            </Show>
            {d()!.is_healthy
              ? "Malla saludable"
              : d()!.needs_repair
                ? "Requiere reparación"
                : "Sin defectos reparables"}
          </div>

          <div class="grid grid-cols-2 gap-1.5">
            <For each={diagBadges()}>
              {(badge) => (
                <div
                  class={clsx(
                    "flex justify-between items-center gap-2 px-2 py-1.5 rounded text-xs border",
                    badge.bad
                      ? "bg-red/5 border-red/20 text-red"
                      : "bg-surface/20 border-border text-text-muted"
                  )}
                >
                  <span class="truncate">{badge.label}</span>
                  <span class="font-mono font-semibold">{badge.value}</span>
                </div>
              )}
            </For>
          </div>

          <div class="space-y-1 px-1">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Cerrada (estanca)</span>
              {yesNo(d()!.is_closed)}
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Manifold</span>
              {yesNo(d()!.is_manifold)}
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Normales consistentes</span>
              {yesNo(d()!.normals_consistent)}
            </div>
            <Show when={d()!.normals_outward !== null}>
              <div class="flex justify-between text-xs">
                <span class="text-text-muted">Normales hacia afuera</span>
                {yesNo(d()!.normals_outward!)}
              </div>
            </Show>
            <Show when={d()!.is_closed}>
              <div class="flex justify-between text-xs">
                <span class="text-text-muted">Volumen</span>
                <span class="font-mono text-text-muted">{formatNumber(d()!.volume)}</span>
              </div>
            </Show>
            <Show when={d()!.needle_faces + d()!.cap_faces > 0}>
              <div
                class="flex justify-between text-xs"
                title="Triángulos muy alargados. No son un defecto: la retopología los mejora."
              >
                <span class="text-text-muted">Triángulos alargados (calidad)</span>
                <span class="font-mono text-text-muted">{d()!.needle_faces + d()!.cap_faces}</span>
              </div>
            </Show>
            <Show when={d()!.self_intersections > 0}>
              <p class="text-xs text-text-muted leading-relaxed pt-1">
                Las auto-intersecciones (cuerpos solapados) no se corrigen aquí; la mayoría de los
                laminadores las toleran.
              </p>
            </Show>
          </div>
        </div>
      </Show>

      {/* Opciones de reparación */}
      <Show when={d()?.needs_repair}>
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Reparación</h4>

          <div class="space-y-2">
            <Checkbox
              checked={props.repairOptions.mergeDuplicates}
              onChange={(v) => set({ mergeDuplicates: v })}
              label="Soldar costuras y duplicados"
            />
            <Checkbox
              checked={props.repairOptions.removeDegenerates}
              onChange={(v) => set({ removeDegenerates: v })}
              label="Corregir caras degeneradas y repetidas"
            />
            <Checkbox
              checked={props.repairOptions.fixNonManifold}
              onChange={(v) => set({ fixNonManifold: v })}
              label="Separar geometría non-manifold"
            />
            <Checkbox
              checked={props.repairOptions.fixNormals}
              onChange={(v) => set({ fixNormals: v })}
              label="Unificar orientación de normales"
            />
            <Checkbox
              checked={props.repairOptions.orientOutward}
              onChange={(v) => set({ orientOutward: v })}
              label="Normales hacia afuera"
            />
            <Checkbox
              checked={props.repairOptions.removeSmallComponents}
              onChange={(v) => set({ removeSmallComponents: v })}
              label="Eliminar piezas sueltas diminutas"
            />
            <Checkbox
              checked={props.repairOptions.fillHoles}
              onChange={(v) => set({ fillHoles: v })}
              label="Rellenar agujeros"
            />
          </div>

          <Show when={props.repairOptions.fillHoles}>
            <div class="space-y-2 pl-6">
              <Checkbox
                checked={props.repairOptions.refineFill}
                onChange={(v) => set({ refineFill: v })}
                label="Relleno suave (sigue la curvatura)"
              />
              <NumberInput
                label="Tamaño máx. de agujero"
                value={props.repairOptions.maxHoleEdges}
                onChange={(v) => set({ maxHoleEdges: Math.max(0, Math.round(v)) })}
                min={0}
                step={50}
                suffix={props.repairOptions.maxHoleEdges === 0 ? "sin límite" : "aristas"}
              />
            </div>
          </Show>

          <Button
            onClick={props.onRepair}
            disabled={!props.canRepair || props.isProcessing}
            loading={props.isProcessing}
            variant="primary"
            fullWidth
            size="sm"
            icon={<Icons.Wrench size={14} />}
          >
            Reparar
          </Button>
        </div>
      </Show>

      {/* Resultado de reparación */}
      <Show when={props.repairResult}>
        <div class="space-y-2 p-3 rounded-md bg-green/5 border border-green/20">
          <p class="text-xs font-semibold text-green">Reparación completada</p>
          <div class="space-y-1">
            <For each={resultLines()}>
              {([n, text]) => (
                <p class="text-xs text-text-muted">
                  {n} {text}
                </p>
              )}
            </For>
            <Show when={props.repairResult!.holes_filled > 0}>
              <p class="text-xs text-text-muted">
                {props.repairResult!.holes_filled} agujeros rellenados (+{props.repairResult!.faces_added} caras)
              </p>
            </Show>
            <Show when={props.repairResult!.holes_skipped > 0}>
              <p class="text-xs text-yellow">
                {props.repairResult!.holes_skipped} bordes sin rellenar (láminas abiertas, rendijas o
                agujeros sobre el tamaño máximo)
              </p>
            </Show>
            <Show when={resultLines().length === 0 && props.repairResult!.holes_filled === 0}>
              <p class="text-xs text-text-muted">No hubo nada que cambiar.</p>
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
