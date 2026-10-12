import type { Catalog } from "../i18n";
import { enShell } from "./en.shell.ts";
import { en_layout1 } from "./en.layout1.ts";
import { en_layout2 } from "./en.layout2.ts";
import { en_ui_steps } from "./en.ui_steps.ts";
import { en_panels1 } from "./en.panels1.ts";
import { en_panels2 } from "./en.panels2.ts";
import { en_lib1 } from "./en.lib1.ts";
import { en_lib2 } from "./en.lib2.ts";
import { en_app } from "./en.app.ts";
import { enFixes } from "./en.fixes.ts";

/**
 * Inglés: clave = texto en español tal cual está en el código. Un archivo por
 * zona (en.<zona>.ts) para que se trabajen sin pisarse
 */
export const en: Catalog = { ...enShell, ...en_layout1, ...en_layout2, ...en_ui_steps, ...en_panels1, ...en_panels2, ...en_lib1, ...en_lib2, ...en_app, ...enFixes };
