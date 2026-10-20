/**
 * Preferencias de la app que se cambian en Configuración y se recuerdan entre
 * sesiones. Las que ya vivían en App (tema, grilla, guardado automático,
 * navegación con trackpad) siguen allí; acá van las nuevas.
 */

import { createPersisted } from "./ui-state";

/** Escalas de la interfaz que se ofrecen (1 = 100 %) */
export const UI_SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5] as const;

const [scale, setScale] = createPersisted<number>("settings.uiScale", 1);
const [fps, setFps] = createPersisted<boolean>("settings.showFps", true);
const [format, setFormat] = createPersisted<string>("settings.exportFormat", "glb");

/** Tamaño de la interfaz (reactivo); un valor guardado que ya no existe vale 1 */
export const uiScale = () => ((UI_SCALES as readonly number[]).includes(scale()) ? scale() : 1);
export const setUiScale = setScale;

/** Contador de FPS en la cabecera */
export const showFps = fps;
export const setShowFps = setFps;

/** Bajar la resolución del visor mientras hay movimiento (animar, arrastrar, orbitar) y restaurarla al parar */
const [adaptiveQualityRaw, setAdaptiveQualityRaw] = createPersisted<boolean>("settings.adaptiveQuality", true);
export const adaptiveQuality = adaptiveQualityRaw;
export const setAdaptiveQuality = setAdaptiveQualityRaw;

/** Formato con que arranca el panel Exportar en un proyecto nuevo */
export const defaultExportFormat = format;
export const setDefaultExportFormat = setFormat;

/** Cuándo se arma la malla de edición de un modelo grande */
export type MeshBuildMode = "now" | "background" | "lazy";
const MESH_BUILD_MODES: MeshBuildMode[] = ["now", "background", "lazy"];
const [meshBuildRaw, setMeshBuildRaw] = createPersisted<MeshBuildMode>("settings.meshBuild", "background");
export const meshBuild = (): MeshBuildMode => (MESH_BUILD_MODES.includes(meshBuildRaw()) ? meshBuildRaw() : "background");
export const setMeshBuild = setMeshBuildRaw;

/** Mostrar primero una vista previa liviana de los modelos muy pesados */
const [previewFirstRaw, setPreviewFirst] = createPersisted<boolean>("settings.previewFirst", true);
export const previewFirst = previewFirstRaw;
export const setPreviewFirstModel = setPreviewFirst;

/**
 * LOD al animar: con un modelo muy pesado (más de 100 000 caras) y esqueleto, el
 * visor dibuja una versión decimada (~50 000 caras) con los mismos pesos mientras
 * se anima. Solo es visual: exportar, guardar y renderizar usan la malla completa.
 */
const [animationLodRaw, setAnimationLodRaw] = createPersisted<boolean>("settings.animationLod", true);
export const animationLod = animationLodRaw;
export const setAnimationLodEnabled = setAnimationLodRaw;

/** Pedir confirmación, con los datos del archivo (peso, caras, materiales), antes de importar un modelo */
const [peekBeforeImportRaw, setPeekBeforeImport] = createPersisted<boolean>("settings.peekBeforeImport", false);
export const peekBeforeImport = peekBeforeImportRaw;
export const setPeekBeforeImportModel = setPeekBeforeImport;

/** Archivos abiertos hace poco (proyectos y modelos), el más nuevo primero */
const MAX_RECENT = 8;
const [recentRaw, setRecentRaw] = createPersisted<string[]>("files.recent", []);
export const recentFiles = (): string[] => (Array.isArray(recentRaw()) ? recentRaw() : []);
export const rememberRecent = (path: string) => setRecentRaw([path, ...recentFiles().filter((p) => p !== path)].slice(0, MAX_RECENT));
export const forgetRecent = (path: string) => setRecentRaw(recentFiles().filter((p) => p !== path));
export const clearRecent = () => setRecentRaw([]);

/** Impresoras propias (Fabricar): nombre, cama y boquilla; se recuerdan entre sesiones */
export interface SavedPrinter {
  name: string;
  build_volume: [number, number, number];
  nozzle: number;
  layer_height: number;
}
const [savedPrinters, setSavedPrinters] = createPersisted<SavedPrinter[]>("settings.printers", []);
export const customPrinters = savedPrinters;
/** Guarda (o reemplaza, por nombre) una impresora propia */
export const saveCustomPrinter = (printer: SavedPrinter) =>
  setSavedPrinters([...savedPrinters().filter((p) => p.name !== printer.name), printer]);
export const removeCustomPrinter = (name: string) => setSavedPrinters(savedPrinters().filter((p) => p.name !== name));

/** Materiales propios (Fabricar): densidad, voladizo, contracción por eje y factor de holgura */
export interface SavedMaterial {
  name: string;
  density: number;
  max_overhang_deg: number;
  shrink?: [number, number, number];
  clearance_factor?: number;
}
const [savedMaterials, setSavedMaterials] = createPersisted<SavedMaterial[]>("settings.materials", []);
export const customMaterials = savedMaterials;
/** Guarda (o reemplaza, por nombre) un material propio */
export const saveCustomMaterial = (material: SavedMaterial) =>
  setSavedMaterials([...savedMaterials().filter((m) => m.name !== material.name), material]);
export const removeCustomMaterial = (name: string) => setSavedMaterials(savedMaterials().filter((m) => m.name !== name));

const PREFIX = "pinocchio.ui.";
/** Lo que no es disposición de la interfaz: se conserva al restablecer */
const KEEP = ["settings.", "templates."];

/**
 * Olvida lo recordado de la disposición (anchos, paneles abiertos, pestañas,
 * alto de la línea de tiempo…) y deja las preferencias y las plantillas
 * favoritas. Devuelve cuántas claves borró; hay que recargar para verlo
 */
export function resetLayout(): number {
  let removed = 0;
  try {
    for (const key of Object.keys(localStorage)) {
      if (!key.startsWith(PREFIX)) continue;
      const name = key.slice(PREFIX.length);
      if (KEEP.some((k) => name.startsWith(k))) continue;
      localStorage.removeItem(key);
      removed++;
    }
  } catch {
    // Sin almacenamiento: no hay nada que olvidar
  }
  return removed;
}
