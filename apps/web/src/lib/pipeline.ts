import { createSignal, type Component } from "solid-js";
import * as Icons from "../components/icons";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export type PipelineStepId =
  | "import"
  | "repair"
  | "retopology"
  | "uv"
  | "skeleton"
  | "animate"
  | "print3d"
  | "export";

export type StepStatus = "locked" | "available" | "active" | "completed";

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
    placeholder: true,
    placeholderDescription:
      "Animación procedural y edición de poses: crear ciclos de caminata, idle y acciones personalizadas sobre el esqueleto.",
  },
  {
    id: "print3d",
    label: "Imprimir 3D",
    icon: Icons.Printer,
  },
  {
    id: "export",
    label: "Exportar",
    icon: Icons.Export,
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// STORE
// ═══════════════════════════════════════════════════════════════════════════

export interface PipelineState {
  meshLoaded: boolean;
  retopologyDone: boolean;
  autorigDone: boolean;
}

export function createPipelineStore() {
  const [activeStep, setActiveStep] = createSignal<PipelineStepId>("import");
  const [completedSteps, setCompletedSteps] = createSignal<Set<PipelineStepId>>(new Set());

  const markCompleted = (stepId: PipelineStepId) => {
    setCompletedSteps((prev) => {
      const next = new Set(prev);
      next.add(stepId);
      return next;
    });
  };

  const getStepStatus = (stepId: PipelineStepId, meshLoaded: boolean): StepStatus => {
    if (activeStep() === stepId) return "active";
    if (completedSteps().has(stepId)) return "completed";
    // Import siempre disponible
    if (stepId === "import") return "available";
    // El resto requiere modelo cargado
    if (!meshLoaded) return "locked";
    return "available";
  };

  const navigateTo = (stepId: PipelineStepId, meshLoaded: boolean) => {
    const status = getStepStatus(stepId, meshLoaded);
    if (status === "locked") return;
    setActiveStep(stepId);
  };

  return {
    activeStep,
    setActiveStep,
    completedSteps,
    markCompleted,
    getStepStatus,
    navigateTo,
  };
}
