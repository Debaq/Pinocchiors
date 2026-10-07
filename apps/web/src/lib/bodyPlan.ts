/** Plan de cuerpo de las plantillas generadas (ver `BodyPlanDto` en apps/desktop) */

export type BodyShape = "biped" | "digitigrade" | "quadruped" | "radial" | "fish" | "arthropod" | "serpent" | "tree";

/** Forma de cuerpo + apéndices */
export interface BodyPlan {
  shape: BodyShape;
  neck: number;
  tail: number;
  trunk: number;
  ears: number;
  wings: number;
  limbs: number;
  limb_segments: number;
  fins: boolean;
  pincers: boolean;
  antennae: number;
  /** Los proyectos anteriores no los traen (el backend pone lo básico) */
  horns?: number;
  jaw?: boolean;
  tusks?: number;
  tentacles?: number;
  flukes?: boolean;
  leg_length?: number;
  feet?: "simple" | "plantigrade" | "digitigrade" | "unguligrade";
  neck_shape?: "rising" | "swan" | "upright" | "level";
  humps?: number;
  arm_length?: number;
  tail_length?: number;
  sprawl?: boolean;
  /** Extremidades que faltan (bits): 1 brazo o pata delantera izquierda, 2 derecha, 4 pierna o pata trasera izquierda, 8 derecha */
  missing?: number;
  /** Cantidad de cabezas y de colas (1 si no viene) */
  heads?: number;
  tails?: number;
}
