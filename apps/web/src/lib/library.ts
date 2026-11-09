// Biblioteca maker: el catálogo (lo arma el backend desde sus tablas de
// medidas) y el nombre de cada pieza, como lo da `cad_model::library::describe`.

import { invoke } from "@tauri-apps/api/core";
import type { CatalogItem, LibraryCategory, LibraryPart } from "./cad";
import { t } from "./i18n";
import { tBackend } from "./i18nBackend";

let cache: Promise<CatalogItem[]> | undefined;

/** El catálogo (se pide una vez) */
export function libraryCatalog(): Promise<CatalogItem[]> {
  cache ??= invoke<CatalogItem[]>("cad_library_catalog").catch((e) => {
    cache = undefined;
    throw e;
  });
  return cache;
}

/** Categorías en el orden de las pestañas */
export const LIBRARY_CATEGORIES: { id: LibraryCategory; label: () => string }[] = [
  { id: "motors", label: () => t("Motores") },
  { id: "profiles", label: () => t("Perfiles") },
  { id: "boards", label: () => t("Placas") },
  { id: "magnets", label: () => t("Imanes") },
  { id: "fasteners", label: () => t("Tornillería") },
  { id: "motion", label: () => t("Movimiento") },
  { id: "modules", label: () => t("Módulos") },
];

const mm = (v: number) => String(+v.toFixed(2));

/** «NEMA 17 · 48 mm», «Perfil 2020 · 350 mm» (en español: es el nombre que se guarda) */
export function libraryName(items: CatalogItem[], p: LibraryPart): string {
  const it = items.find((i) => i.id === p.item);
  if (!it) return p.item;
  let out = it.name;
  const v = it.variants.find((x) => x.id === p.variant) ?? it.variants[0];
  if (v && v.label && it.variants.length > 1) out += ` · ${v.label}`;
  if (it.length && p.length !== undefined) out += ` · ${mm(p.length)} mm`;
  if (p.role === "holes") return `Agujeros para ${out}`;
  if (p.role === "mounts") return `Soportes para ${out}`;
  return out;
}

/** El nombre en el idioma de la interfaz (los nombres del catálogo vienen en español) */
export const itemLabel = (it: CatalogItem) => tBackend(it.name);
