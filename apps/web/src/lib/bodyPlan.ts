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
  /** Artrópodo: cabeza aparte (insecto) o fusionada (araña, cangrejo) */
  separate_head?: boolean;
  /** Artrópodo: segmentos del abdomen (0: sin abdomen) */
  abdomen?: number;
  /** Artrópodo: ancho del cuerpo */
  body_width?: number;
  fangs?: boolean;
  palps?: boolean;
  eye_stalks?: boolean;
  wing_pairs?: number;
  /** Artrópodo: un par de patas por segmento (ciempiés) */
  segmented?: boolean;
  /** Radial: hacia dónde van los brazos */
  radial_pose?: "spread" | "hanging" | "flat" | "up";
  /** Radial: segmentos del manto o columna (0: sin cabeza) */
  mantle?: number;
  /** Cuadrúpedo: patas traseras de salto plegadas en Z (rana, conejo) */
  jumper?: boolean;
}
