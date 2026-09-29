import { createSignal, type Component } from "solid-js";
import * as Icons from "../components/icons";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export type PipelineStepId =
  | "import"
  | "structure"
  | "repair"
  | "retopology"
  | "uv"
  | "skeleton"
  | "animate"
  | "print3d"
  | "scan"
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
    label: "Importar",
    icon: Icons.FilePlus,
  },
  {
    id: "structure",
    label: "Estructura",
    icon: Icons.TreeStructure,
  },
  {
    id: "repair",
    label: "Reparar",
    icon: Icons.Wrench,
  },
  {
    id: "retopology",
    label: "Retopología",
    icon: Icons.GridFour,
  },
  {
    id: "uv",
    label: "UV / Piel",
    icon: Icons.Checkerboard,
  },
  {
    id: "skeleton",
    label: "Esqueleto",
    icon: Icons.Bone,
  },
  {
    id: "animate",
    label: "Animar",
    icon: Icons.Play,
  },
  {
    id: "print3d",
    label: "Imprimir 3D",
    icon: Icons.Printer,
  },
  {
    id: "scan",
    label: "Escáner 3D",
    icon: Icons.Scan,
  },
  {
    id: "export",
    label: "Exportar",
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

export type WorkspaceId = "prepare" | "rig" | "print" | "scan";

export interface Workspace {
  id: WorkspaceId;
  label: string;
  sections: PipelineStepId[];
  /** Se abre aunque no haya modelo (el escáner lo crea) */
  withoutModel?: boolean;
}

export const WORKSPACES: Workspace[] = [
  { id: "prepare", label: "Preparar", sections: ["structure", "repair", "retopology", "uv"] },
  { id: "rig", label: "Rig y animación", sections: ["skeleton", "animate"] },
  { id: "print", label: "Imprimir 3D", sections: ["print3d"] },
  { id: "scan", label: "Orizon3D", sections: ["scan"], withoutModel: true },
];

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
    // "import" ya no es una sección (proyectos viejos)
    const next = step === "import" ? "structure" : step;
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
  const setCompleted = (steps: PipelineStepId[]) => setCompletedSteps(new Set(steps));

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
