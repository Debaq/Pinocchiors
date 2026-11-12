// Los visores que se ven en la colaboración: cada uno dice cómo leer y poner
// su cámara y cómo pasar de pantalla a un punto 3D y de vuelta. Se registran
// con el nombre de su `data-collab-view` ("viewport" o "design").

import type { CameraPose } from "./cameraRig";

export interface CollabViewer {
  pose(): CameraPose;
  setPose(pose: CameraPose): void;
  /** Pantalla (-1..1) → punto 3D en metros, a la distancia del objetivo */
  rayPoint(ndc: [number, number]): [number, number, number];
  /** Punto 3D en metros → pantalla (-1..1); nada si no se ve */
  toNdc(point: [number, number, number]): [number, number] | undefined;
}

const views = new Map<string, CollabViewer>();

/** Registra un visor; devuelve cómo quitarlo */
export function registerCollabView(name: string, viewer: CollabViewer): () => void {
  views.set(name, viewer);
  return () => {
    if (views.get(name) === viewer) views.delete(name);
  };
}

export const collabView = (name: string) => views.get(name);
