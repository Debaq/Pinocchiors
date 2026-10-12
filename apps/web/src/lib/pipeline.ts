import { createSignal, type Component } from "solid-js";
import * as Icons from "../components/icons";
import { t } from "./i18n.ts";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export type PipelineStepId =
  | "import"
  | "structure"
  | "repair"
  | "remesh"
  | "uv"
  | "materials"
  | "skeleton"
  | "animate"
  | "print3d"
  | "render"
  | "scan"
  | "design"
  | "design_part"
  | "design_inspect"
  | "design_scan"
  | "export";

export interface PipelineStep {
  id: PipelineStepId;
  label: string;
  icon: Component<{ size?: number | string }>;
  placeholder?: boolean;
  placeholderDescription?: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// STEP DEFINITIONS
// ═══════════════════════════════════════════════════════════════════════════

export const PIPELINE_STEPS: PipelineStep[] = [
  {
    id: "import",
    get label() { return t("Importar"); },
    icon: Icons.FilePlus,
  },
  {
    id: "structure",
    get label() { return t("Estructura"); },
    icon: Icons.TreeStructure,
  },
  {
    id: "repair",
    get label() { return t("Reparar"); },
    icon: Icons.Wrench,
  },
  {
    id: "remesh",
    get label() { return t("Remallar"); },
    icon: Icons.GridFour,
  },
  {
    id: "uv",
    get label() { return t("UV / Piel"); },
    icon: Icons.Checkerboard,
  },
  {
    id: "materials",
    get label() { return t("Materiales"); },
    icon: Icons.PaintBrush,
  },
  {
    id: "skeleton",
    get label() { return t("Esqueleto"); },
    icon: Icons.Bone,
  },
  {
    id: "animate",
    get label() { return t("Animar"); },
    icon: Icons.Play,
  },
  {
    id: "print3d",
    get label() { return t("Fabricar"); },
    icon: Icons.Hammer,
  },
  {
    id: "render",
    get label() { return t("Render"); },
    icon: Icons.FrameCorners,
  },
  {
    id: "scan",
    get label() { return t("Escáner 3D"); },
    icon: Icons.Scan,
  },
  {
    id: "design",
    get label() { return t("Diseño"); },
    icon: Icons.Ruler,
  },
  {
    id: "design_part",
    get label() { return t("Pieza"); },
    icon: Icons.Cube,
  },
  {
    id: "design_inspect",
    get label() { return t("Inspección"); },
    icon: Icons.MagnifyingGlass,
  },
  {
    id: "design_scan",
    get label() { return t("Desde el escaneo"); },
    icon: Icons.Scan,
  },
  {
    id: "export",
    get label() { return t("Exportar"); },
    icon: Icons.Export,
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// ESPACIOS DE TRABAJO
// Como en Blender: no hay un camino obligatorio. Cada espacio agrupa sus
// secciones (las pestañas del panel de propiedades) y se puede saltar entre
// ellos en cualquier momento. Importar y exportar abren y cierran el trabajo
// (menú Archivo); transformar el modelo está siempre en la pestaña Objeto.
// ═══════════════════════════════════════════════════════════════════════════

export type WorkspaceId = "prepare" | "materials" | "rig" | "print" | "render" | "scan" | "design";

export interface Workspace {
  id: WorkspaceId;
  label: string;
  sections: PipelineStepId[];
  /** Se abre aunque no haya modelo */
  withoutModel?: boolean;
}

export const WORKSPACES: Workspace[] = [
  { id: "prepare", get label() { return t("Preparar"); }, sections: ["structure", "repair", "remesh", "uv"] },
  { id: "materials", get label() { return t("Materiales"); }, sections: ["materials"] },
  // Sin modelo se puede armar y animar un esqueleto solo, y exportarlo
  { id: "rig", get label() { return t("Rig y animación"); }, sections: ["skeleton", "animate"], withoutModel: true },
  { id: "print", get label() { return t("Fabricar"); }, sections: ["print3d"] },
  { id: "render", get label() { return t("Render"); }, sections: ["render"] },
  { id: "scan", label: "Orizon3D", sections: ["scan"], withoutModel: true },
  // CAD paramétrico: se diseña desde cero o calcando el modelo cargado
  { id: "design", get label() { return t("Diseñar"); }, sections: ["design", "design_part", "design_inspect", "design_scan"], withoutModel: true },
];

/**
 * Secciones de proyectos viejos: "import" ya no es una sección y
 * "retopology" ahora es un modo de Remallar.
 */
export function legacyStep(step: PipelineStepId | "retopology"): PipelineStepId {
  if (step === "import") return "structure";
  if (step === "retopology") return "remesh";
  return step;
}

export const stepInfo = (id: PipelineStepId) => PIPELINE_STEPS.find((s) => s.id === id) ?? PIPELINE_STEPS[0];

/** Espacio al que pertenece una sección (`undefined` para exportar) */
export const workspaceOf = (step: PipelineStepId): Workspace | undefined =>
  WORKSPACES.find((w) => w.sections.includes(step));

// ═══════════════════════════════════════════════════════════════════════════
// STORE
// ═══════════════════════════════════════════════════════════════════════════

export function createPipelineStore() {
  const [activeStep, setActive] = createSignal<PipelineStepId>("structure");
  const [completedSteps, setCompletedSteps] = createSignal<Set<PipelineStepId>>(new Set());
  // Última sección abierta de cada espacio: volver a él lleva ahí
  const lastSection: Partial<Record<WorkspaceId, PipelineStepId>> = {};

  const setActiveStep = (step: PipelineStepId) => {
    const next = legacyStep(step);
    const ws = workspaceOf(next);
    if (ws) lastSection[ws.id] = next;
    setActive(next);
  };

  const markCompleted = (stepId: PipelineStepId) => {
    setCompletedSteps((prev) => {
      const next = new Set(prev);
      next.add(stepId);
      return next;
    });
  };

  /** Reemplaza los pasos hechos (al abrir un proyecto) */
  const setCompleted = (steps: PipelineStepId[]) => setCompletedSteps(new Set(steps.map(legacyStep)));

  const openWorkspace = (id: WorkspaceId) => {
    const ws = WORKSPACES.find((w) => w.id === id)!;
    setActiveStep(lastSection[id] ?? ws.sections[0]);
  };

  return {
    activeStep,
    setActiveStep,
    /** Espacio activo (`undefined` mientras se exporta) */
    workspace: () => workspaceOf(activeStep()),
    openWorkspace,
    completedSteps,
    markCompleted,
    setCompleted,
  };
}
