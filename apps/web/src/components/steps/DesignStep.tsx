import { matchesOutliner, outlinerQuery } from "../../lib/outlinerFilter";
import { Component, For, Index, Match, Show, Switch, createEffect, createMemo, createResource, createSignal, on, onCleanup, untrack, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import {
  BASE_PLANES,
  CONSTRAINT_LABELS,
  FEATURE_LABELS,
  MATERIALS,
  MATE_LABELS,
  METRIC_HOLES,
  INCH_HOLES,
  BOLT_HEADS,
  type BoltHead,
  designMass,
  partColor,
  partHidden,
  partMass,
  partMaterial,
  samePart,
  OP_LABELS,
  PLANE_LABELS,
  constraintIds,
  editsAsSketch,
  sketchAnchored,
  constraintValue,
  isReference,
  filletCorner,
  removeEntity,
  ensureOrigin,
  addPoint,
  addEntity,
  fillPattern,
  mirrorEntities,
  toggleSplineHandles,
  linesAt,
  offsetEntities,
  moveRegionPoints,
  offsetPlane,
  type AxisSpec,
  type FrameAxis,
  type BendRelief,
  type BodyOp,
  type CornerRelief,
  type CadStore,
  type ChamferSecond,
  type Configuration,
  nextVersionName,
  type Detection,
  type EdgeRef,
  type Extent,
  type FaceRef,
  type Feature,
  type FeatureKind,
  type FeatureState,
  folderRange,
  type Material,
  type MissingRef,
  type Motion,
  type SweepOrient,
  type BlendSide,
  type P2,
  type P3,
  type ScanDab,
  FIT_COLORS,
  type MoldOptions,
  type ReconstructProposal,
  type AxisDef,
  type Connector,
  type Mate,
  type MateKind,
  type Relation,
  type PartId,
  type CadDocument,
  type PlaneDef,
  type PointSpec,
  type RefPick,
  type CurveDef,
  type Plane,
  type Sketch3d,
  type Constraint3d,
  type PartView,
  type PatternKind,
  type PrimitiveShape,
  type DriveLength,
  type GearBending,
  type GearInfo,
  type GearMaterial,
  type GearStrength,
  type GearStrengthInput,
  type ToothForm,
  type ToothFormInfo,
  type GearKind,
  type GearPart,
  type GearSpec,
  type RegionSelection,
  type PlaneSpec,
  type ScanPick,
  type Sketch,
  type SketchImage,
  type SketchLayer,
  breakLinks,
  type SketchEntity,
  type Geometry,
  type SketchConstraint,
  type SketchSuggestion,
  type ThreadAxis,
  type ThreadLink,
  type ThreadWant,
  coaxialThreads,
  threadCandidates,
  threadLabel,
  threadName,
  threadOfFace,
  boltLengthFor,
  BOLT_LENGTHS,
  NUT_HEIGHT,
  isScanReference,
} from "../../lib/cad";
import type { CadUi, Hit3d } from "../../lib/cadUi";
import type { DesignActions } from "../../lib/designActions";
import { regionContains } from "../../lib/CadViewer";
import { flipSketchY, insertClip, rotateSketch, rotation, scaling, selectionCenter, translation } from "../../lib/sketchTransform";
import { constraintSuggestions } from "../../lib/sketchSuggest";
import { connectedChain } from "../../lib/sketchCheck";
import { convertToBSpline, fitSplineToPoints, removeSplinePoint, simplifySpline, splineCurvature, toggleSplineHandle } from "../../lib/sketchSplines";
import { bsplineToThrough } from "../../lib/sketchTrim";
import { dropPattern, editPattern, makePattern, patternAt, patternLabel, toggleSkip, type PatternSpec, type SketchPatternRecord } from "../../lib/sketchPatterns";
import { ENTITY_NAMES, selectByKind, sketchIssues, type SelectKind } from "../../lib/sketchCheck";
import { copyText } from "../../lib/clipboard";
import { chamferCorner, closeContour, joinEntities, mergeOverlaps, offsetChain, orderChain, removeDuplicates, repairSketch, splitEqual, type OffsetCaps } from "../../lib/sketchEdit";
import { GLYPHS, isDimension } from "../../lib/sketchGlyphs";
import { sampleCurve } from "../../lib/sketchFormula";
import { addShapes, parseCoordinates, parseDxf, sketchToDxf, sketchToSvg } from "../../lib/sketchFiles";
import { addLayer, createBlock, explodeBlock, refreshBlock, removeLayer, setLayer } from "../../lib/sketchBlocks";
import { partKey } from "../../lib/objects";
import { axisToVertical, faceToFloor, faceToFront, pointToOrigin, type Mat4 } from "../../lib/scanAlign";
import { Button, Checkbox, IconButton, NumberInput, Select, Slider, Tooltip } from "../ui";
import * as Icons from "../icons";
import { t as tl, locale, tn } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { changeDesignTabs } from "../layout/DesignTabs";
import { instanceStudio } from "../../lib/designTabs";
import { jointKey, jointsOf } from "../../lib/assemblyDrag";
import { planetFeatures, planetaryAssembly, ringFeature, type PlanetaryPlan } from "../../lib/planetary";

export interface DesignStepProps {
  store: CadStore;
  ui: CadUi;
  /** Acciones de Diseñar (las comparte con la barra del visor) */
  actions: DesignActions;
  /** Pestaña del panel */
  section: "design" | "design_part" | "design_inspect" | "design_scan";
  /** Hay un modelo cargado que se puede usar como escaneo de referencia */
  /** Objetos que pueden servir de escaneo (importados, con malla) */
  scans: { id: number; name: string }[];
  /** El que se usa (el elegido o, si no hay, el que decide la app) */
  scanId?: number;
  /** Mueve el escaneo (matriz del CAD por columnas, ver lib/scanAlign.ts) */
  onAlignScan: (matrix: Mat4, description: string) => Promise<void>;
}

// ─── Controles chicos ─────────────────────────────────────────────────────

/** Número que se aplica al confirmar (Enter o salir del campo), no con cada tecla */
/** Acoplamiento hecho a mano: elegir el tipo, las uniones y la razón (los engranes enlazados se detectan solos) */
const RelationAdder: Component<{ mates: Mate[]; onAdd: (r: Relation) => void }> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [type, setType] = createSignal<Relation["type"]>("gear");
  const [first, setFirst] = createSignal("");
  const [second, setSecond] = createSignal("");
  const [amount, setAmount] = createSignal(-1);
  const [phase, setPhase] = createSignal(0);
  const kinds = (): [MateKind[], MateKind[]] =>
    type() === "gear" ? [["revolute"], ["revolute"]] : type() === "rack_pinion" ? [["revolute"], ["slider"]] : [["cylindrical"], []];
  const options = (ks: MateKind[]) => props.mates.filter((m) => ks.includes(m.kind)).map((m) => ({ value: String(m.id), label: m.name }));
  const valid = () => !!first() && (type() === "screw" || (!!second() && second() !== first())) && amount() !== 0;
  const add = () => {
    const [a, b] = [Number(first()), Number(second())];
    const t = type();
    props.onAdd(
      t === "gear" ? { type: "gear", a, b, ratio: amount(), phase: phase() }
      : t === "rack_pinion" ? { type: "rack_pinion", revolute: a, slider: b, per_turn: amount(), phase: phase() }
      : { type: "screw", cylindrical: a, pitch: amount(), phase: phase() }
    );
    setOpen(false);
  };
  return (
    <Show when={open()} fallback={<Button size="sm" disabled={props.mates.length < 1} onClick={() => setOpen(true)}>{tl("Agregar acoplamiento")}</Button>}>
      <div class="space-y-1 rounded border border-border p-1.5" data-relation-adder>
        <Select
          options={[
            { value: "gear", label: tl("Engranes (giro ↔ giro)") },
            { value: "rack_pinion", label: tl("Piñón y cremallera (giro ↔ deslizamiento)") },
            { value: "screw", label: tl("Tornillo (giro ↔ avance)") },
          ]}
          value={type()}
          onChange={(v) => { setType(v as Relation["type"]); setFirst(""); setSecond(""); setAmount(v === "gear" ? -1 : 1); }}
        />
        <Select options={[{ value: "", label: tl("Unión…") }, ...options(kinds()[0])]} value={first()} onChange={setFirst} />
        <Show when={type() !== "screw"}>
          <Select options={[{ value: "", label: tl("Otra unión…") }, ...options(kinds()[1])]} value={second()} onChange={setSecond} />
        </Show>
        <Num
          label={type() === "gear" ? tl("Razón (− si giran al revés)") : type() === "rack_pinion" ? tl("mm por vuelta") : tl("Paso")}
          value={amount()}
          step={type() === "gear" ? 0.1 : 1}
          onCommit={setAmount}
        />
        <Num label={tl("Fase")} value={phase()} onCommit={setPhase} />
        <div class="flex gap-1.5">
          <Button size="sm" disabled={!valid()} onClick={add}>{tl("Agregar")}</Button>
          <Button size="sm" variant="ghost" onClick={() => setOpen(false)}>{tl("Cancelar")}</Button>
        </div>
      </div>
    </Show>
  );
};

const Num: Component<{ value: number; onCommit: (v: number) => void; label?: string; suffix?: string; step?: number; min?: number }> = (props) => (
  <label class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
    <Show when={props.label}>
      <span class={clsx("text-text-muted", (props.label?.length ?? 0) <= 2 ? "w-3 shrink-0" : "min-w-[4rem] leading-tight")}>{props.label}</span>
    </Show>
    <input
      type="number"
      value={Number.isFinite(props.value) ? +props.value.toFixed(6) : 0}
      step={props.step ?? 1}
      min={props.min}
      class="flex-1 min-w-[4.5rem] px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent [appearance:textfield]"
      onChange={(e) => {
        const v = parseFloat(e.currentTarget.value);
        if (!Number.isNaN(v) && v !== props.value) props.onCommit(v);
      }}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
    />
    <Show when={props.suffix}>
      <span class="text-text-dim shrink-0">{props.suffix}</span>
    </Show>
  </label>
);

/**
 * Campo que acepta un número o una fórmula (`ancho / 2`). Con fórmula muestra
 * "fx" y el valor calculado al pasar el mouse. `onCommit` devuelve un error
 * si la fórmula no se puede calcular (y el texto queda para corregirlo).
 */
export const Formula: Component<{
  value: number;
  expr?: string;
  error?: string | null;
  label?: string;
  suffix?: string;
  onCommit: (text: string) => Promise<string | undefined> | string | undefined;
}> = (props) => {
  const [err, setErr] = createSignal<string>();
  const shown = () => props.expr ?? (Number.isFinite(props.value) ? String(+props.value.toFixed(6)) : "0");
  const problem = () => err() ?? props.error ?? undefined;
  return (
    <label class="flex items-center gap-2 text-xs" title={problem() ?? (props.expr ? `= ${+props.value.toFixed(6)}` : tl("Acepta fórmulas: ancho / 2, max(a, b)…"))}>
      <Show when={props.label}>
        <span class={clsx("text-text-muted shrink-0", (props.label?.length ?? 0) <= 2 ? "w-3" : "w-24")}>{props.label}</span>
      </Show>
      <div class="relative flex-1 min-w-0">
        <input
          type="text"
          value={shown()}
          data-wheel-number={props.expr ? undefined : ""}
          class={clsx(
            "w-full min-w-0 px-2 py-1 rounded bg-surface/40 border text-xs text-text font-mono outline-none focus:border-accent",
            problem() ? "border-error" : props.expr ? "border-accent/60 pr-6" : "border-border",
          )}
          onChange={async (e) => {
            const text = e.currentTarget.value.trim();
            if (!text || text === shown()) return setErr(undefined);
            setErr((await props.onCommit(text)) ?? undefined);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            e.stopPropagation();
          }}
        />
        <Show when={props.expr}>
          <span class="absolute right-1.5 top-1/2 -translate-y-1/2 text-[9px] font-semibold italic text-accent pointer-events-none">fx</span>
        </Show>
      </div>
      <Show when={props.suffix}>
        <span class="text-text-dim shrink-0">{props.suffix}</span>
      </Show>
    </label>
  );
};

const Row: Component<{ label: string; children: JSX.Element }> = (props) => (
  <div class="flex items-center gap-2 text-xs">
    <span class="text-text-muted w-24 shrink-0">{props.label}</span>
    <div class="flex-1 min-w-0">{props.children}</div>
  </div>
);

const Section: Component<{ title: string; children: JSX.Element; right?: JSX.Element; class?: string }> = (props) => (
  <div class={clsx("space-y-2", props.class)}>
    <div class="flex items-center justify-between">
      <h4 class="text-[11px] font-semibold uppercase tracking-wide text-text-dim">{props.title}</h4>
      {props.right}
    </div>
    {props.children}
  </div>
);

const OP_OPTIONS = (Object.keys(OP_LABELS) as BodyOp[]).map((v) => ({ value: v, get label() { return OP_LABELS[v]; } }));
const RELIEF_OPTIONS = [
  { value: "rectangle", get label() { return tl("Ranura recta"); } },
  { value: "obround", get label() { return tl("Ranura con fondo redondo"); } },
  { value: "none", get label() { return tl("Sin alivio"); } },
];
const CORNER_OPTIONS = [
  { value: "none", get label() { return tl("Muesca del doblez"); } },
  { value: "round", get label() { return tl("Redonda"); } },
  { value: "square", get label() { return tl("Cuadrada"); } },
];
const AXIS_DIRS: Record<string, P3> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

const BLEND_SIDE_OPTIONS = [
  { value: "auto", get label() { return tl("Según la geometría"); } },
  { value: "convex", get label() { return tl("Canto (saca material)"); } },
  { value: "concave", get label() { return tl("Rincón (agrega material)"); } },
];

const ORIENT_OPTIONS = [
  { value: "follow", get label() { return tl("Sigue el camino"); } },
  { value: "frenet", get label() { return tl("Frenet (gira con la curva)"); } },
  { value: "fixed", get label() { return tl("Fija (no gira)"); } },
];

/** Número con `d` decimales; lo que redondea a cero se muestra sin signo (no "-0,00") */
const fmt = (v: number, d = 2) => (Math.abs(v) < 0.5 * 10 ** -d ? 0 : v).toLocaleString(locale(), { maximumFractionDigits: d, minimumFractionDigits: d });

export function pickSummary(picks: { kind: string }[]): string {
  const count = (k: string) => picks.filter((p) => p.kind === k).length;
  const parts: [number, string, string][] = [
    [count("region"), "{n} región", "{n} regiones"],
    [count("face"), "{n} cara", "{n} caras"],
    [count("edge"), "{n} arista", "{n} aristas"],
    [count("vertex"), "{n} vértice", "{n} vértices"],
    [count("plane") + count("refplane"), "{n} plano", "{n} planos"],
  ];
  return parts.filter(([n]) => n > 0).map(([n, one, many]) => tn(n, one, many)).join(", ");
}

function planeLabel(p: PlaneSpec): string {
  if (p.type === "face") return tl("Cara del sólido");
  if (p.type === "custom") return tl("Plano propio");
  if (p.type === "reference") return tl("Plano de referencia");
  if (p.type === "frame") return tl("Plano de un sistema de coordenadas");
  return PLANE_LABELS[p.type];
}

// ─── Panel ────────────────────────────────────────────────────────────────

export const DesignStep: Component<DesignStepProps> = (props) => {
  const store = props.store;
  const ui = props.ui;
  const [scanResult, setScanResult] = createSignal<{ pick: ScanPick; triangle: number }>();
  const [sliceBase, setSliceBase] = createSignal<"xy" | "xz" | "yz">("xy");
  const [sliceOffset, setSliceOffset] = createSignal(0);
  const [brushRadius, setBrushRadius] = createSignal(5);
  const [mold, setMold] = createSignal<MoldOptions>({ axis: 2, at: 0.5, wall: 8, gap: 0.2, keys: true, sprue: 3 });
  const setMoldField = <K extends keyof MoldOptions>(k: K, v: MoldOptions[K]) => setMold({ ...mold(), [k]: v });
  const makeMold = async () => {
    try {
      await store.scanMold(mold());
    } catch (e) {
      say(String(e));
    }
  };
  const [detections, setDetections] = createSignal<Detection[]>();

  const actions = props.actions;
  const { say } = actions;

  // ─── Escaneo → CAD ────────────────────────────────────────────────────

  const pickScan = (shape: "plane" | "cylinder" | "cone" | "torus") => {
    setScanResult(undefined);
    ui.setPick({
      kind: "scan",
      shape,
      prompt:
        shape === "plane"
          ? tl("Clic sobre una zona plana del modelo")
          : shape === "cone"
            ? tl("Clic sobre una pared cónica")
            : shape === "torus"
              ? tl("Clic sobre un redondeo o una pieza en anillo (toro)")
              : tl("Clic sobre una pared cilíndrica (agujero o tetón)"),
      done: (pick, triangle) => {
        ui.setPick({ kind: "none" });
        setScanResult({ pick, triangle });
      },
    });
  };

  const addScan = async (feature: Parameters<CadStore["scanAdd"]>[0]) => {
    try {
      await store.scanAdd(feature);
      setScanResult(undefined);
      ui.setScanHighlight([]);
    } catch (e) {
      say(String(e));
    }
  };

  /** Comandos que arman operaciones desde el escaneo (revolución, recubrimiento) */
  const runScan = async (go: () => Promise<unknown>) => {
    try {
      await go();
      setScanResult(undefined);
      ui.setScanHighlight([]);
    } catch (e) {
      say(String(e));
    }
  };

  // Alinear el escaneo: una cara, un eje o un punto y se mueve el escaneo
  const align = (how: "floor" | "front" | "axis" | "origin") => {
    setScanResult(undefined);
    const run = (m: Mat4 | null, description: string, otherwise: string) => {
      ui.setScanHighlight([]);
      if (!m) return say(otherwise);
      void props.onAlignScan(m, description).catch((e) => say(String(e)));
    };
    if (how === "origin") {
      return ui.setPick({ kind: "scan_point", prompt: tl("Clic en el punto del escaneo que va al origen"), done: (p) => run(pointToOrigin(p), tl("Escaneo: punto al origen"), "") });
    }
    const shape = how === "axis" ? "cylinder" : "plane";
    ui.setPick({
      kind: "scan",
      shape,
      prompt:
        how === "floor"
          ? tl("Clic en la cara del escaneo que va apoyada en el piso")
          : how === "front"
            ? tl("Clic en la cara del escaneo que mira al frente")
            : tl("Clic en un cilindro o agujero del escaneo: su eje queda vertical"),
      done: (pick) => {
        ui.setPick({ kind: "none" });
        if (pick.kind === "cylinder") return run(axisToVertical(pick.origin, pick.direction), "Escaneo: eje vertical", "");
        if (pick.kind !== "plane") return;
        if (how === "floor") return run(faceToFloor(pick.plane), tl("Escaneo: cara al piso"), "");
        run(faceToFront(pick.plane), tl("Escaneo: cara al frente"), tl("Esa cara es horizontal: elegir una cara de costado"));
      },
    });
  };

  // Referencias ligadas al escaneo: quedan en el árbol y lo siguen
  const reference = (what: "plane" | "points" | "axis" | "point") => {
    setScanResult(undefined);
    if (what === "plane" || what === "axis") {
      return ui.setPick({
        kind: "scan",
        shape: what === "plane" ? "plane" : "cylinder",
        prompt: what === "plane" ? tl("Clic en una zona plana del escaneo") : tl("Clic en un cilindro o agujero del escaneo"),
        done: (_, triangle, at) => {
          ui.setPick({ kind: "none" });
          void addScan(what === "plane" ? { kind: "ref_plane", triangle, at } : { kind: "ref_axis", triangle, at });
        },
      });
    }
    if (what === "point") {
      return ui.setPick({ kind: "scan_point", prompt: tl("Clic en el punto del escaneo"), done: (at, triangle) => void addScan({ kind: "ref_point", triangle, at }) });
    }
    // Tres clics seguidos
    const points: [number, P3][] = [];
    const next = () =>
      ui.setPick({
        kind: "scan_point",
        prompt: tl("Punto {v1} de 3 sobre el escaneo", { v1: points.length + 1 }),
        done: (at, triangle) => {
          points.push([triangle, at]);
          if (points.length < 3) next();
          else void addScan({ kind: "ref_plane_points", points });
        },
      });
    next();
  };

  /** Curva sobre la piel: puntos con clic y camino más corto entre ellos */
  const [pathPts, setPathPts] = createSignal<[number, P3][] | null>(null);
  const nextPathPoint = () =>
    ui.setPick({
      kind: "scan_point",
      prompt: tl("Clic en los puntos del camino sobre el escaneo ({v1} puestos); después «Terminar»", { v1: pathPts()?.length ?? 0 }),
      done: (at, triangle) => {
        setPathPts([...(pathPts() ?? []), [triangle, at]]);
        nextPathPoint();
      },
    });
  const startPath = () => {
    setPathPts([]);
    nextPathPoint();
  };
  const endPath = (closed: boolean | null) => {
    const pts = pathPts();
    setPathPts(null);
    ui.setPick({ kind: "none" });
    if (closed === null) return;
    if (!pts || pts.length < (closed ? 3 : 2)) return say(tl("Hacen falta al menos dos puntos (tres si es cerrada)"));
    void addScan({ kind: "path_curve", points: pts, closed });
  };
  /** Curvas del sketch elegido (o del último) llevadas a la piel del escaneo */
  const projectToSkin = () => {
    const sel = store.doc()?.features.find((f) => f.id === store.selected());
    const sketch = sel?.kind.type === "sketch" ? sel : [...(store.doc()?.features ?? [])].reverse().find((f) => f.kind.type === "sketch");
    if (!sketch) return say(tl("Primero un sketch con las curvas, en un plano que mire al escaneo"));
    void store.addFeature({ type: "curve", def: { type: "scan_projected", sketch: sketch.id, entities: [], flip: false, offset: 0 } });
  };

  /** Superficie libre de una zona suave; sola, con espesor o cerrada contra una cara plana (2.º clic) */
  const [freeThickness, setFreeThickness] = createSignal(0);
  const [freeClose, setFreeClose] = createSignal(false);
  const freeSurface = () =>
    ui.setPick({
      kind: "scan_point",
      prompt: tl("Clic en la zona suave del escaneo (libre, sin forma geométrica) que se vuelve superficie"),
      done: (at, triangle) => {
        const make = (close: [number, P3] | null) =>
          void addScan({ kind: "free_surface", triangle, at, angle: 25, thickness: freeThickness(), close });
        if (!freeClose()) return make(null);
        ui.setPick({ kind: "scan_point", prompt: tl("Clic en la cara plana del escaneo contra la que se cierra"), done: (a2, t2) => make([t2, a2]) });
      },
    });

  /** Asistente de reconstrucción: propuesta, pasos marcados y desviación al final */
  const [proposal, setProposal] = createSignal<ReconstructProposal>();
  const [marked, setMarked] = createSignal<number[]>([]);
  const [assistBusy, setAssistBusy] = createSignal(false);
  const propose = async () => {
    setAssistBusy(true);
    try {
      const p = await store.scanPropose();
      setProposal(p);
      setMarked(p.steps.filter((s) => s.on).map((s) => s.id));
    } catch (e) {
      say(String(e));
    } finally {
      setAssistBusy(false);
    }
  };
  const buildProposal = async () => {
    setAssistBusy(true);
    try {
      await store.scanApply(marked());
      setProposal(undefined);
      ui.setScanHighlight([]);
      // Al final, qué tanto se parece lo creado al escaneo
      ui.setFit(null);
      ui.setDeviation(await store.deviation(0.2));
    } catch (e) {
      say(String(e));
    } finally {
      setAssistBusy(false);
    }
  };

  /** Pincel: lo que se pinta sobre el escaneo queda como parche (carcasa de 2 mm, pieza nueva) */
  const paintPatch = () =>
    ui.setPick({
      kind: "scan_brush",
      prompt: tl("Arrastrar sobre el escaneo para pintar el parche"),
      radius: brushRadius,
      done: async (points) => {
        try {
          const dabs = await invoke<ScanDab[]>("cad_scan_dabs", { points });
          if (!dabs.length) return say(tl("El pincel no tocó el escaneo"));
          ui.setScanHighlight([]);
          void store.addFeature({ type: "scan_body", offset: 0.2, thickness: 2, faces: 20000, op: "new", paint: { radius: brushRadius(), dabs } });
        } catch (e) {
          say(String(e));
        }
      },
    });

  const detect = async () => {
    try {
      setDetections(await store.scanDetect());
    } catch (e) {
      say(String(e));
    }
  };

  // ─── Render ───────────────────────────────────────────────────────────

  return (
    <div class="space-y-5">
      <Show when={store.status()?.available !== false} fallback={<NoKernel />}>
        <Show
          when={store.doc()}
          fallback={
            <p class="text-xs text-text-dim">{tl("Preparando el diseño…")}</p>
          }
        >
          <Switch>
            <Match when={props.section === "design"}>
              <Show when={ui.session()} fallback={
                <Show when={!ui.assemblyMode()} fallback={<AssemblyPanel store={store} ui={ui} />}>
                  <ParametersSection store={store} />
                  <ConfigurationsSection store={store} />
                  <VersionsSection store={store} />
                </Show>
              }>
                <SketchPanel ui={ui} store={store} />
              </Show>
            </Match>
            <Match when={props.section === "design_part"}>
              <PartsSection store={store} ui={ui} />
              <Show when={!store.result()?.parts.length}>
                <p class="text-xs text-text-dim">{tl("Todavía no hay piezas: crea un sólido con la barra de arriba del visor.")}</p>
              </Show>
            </Match>
            <Match when={props.section === "design_inspect"}>
              <BodySection store={store} ui={ui} />
              <Show when={!store.result()?.body}>
                <p class="text-xs text-text-dim">{tl("Todavía no hay sólido para inspeccionar.")}</p>
              </Show>
            </Match>
            <Match when={props.section === "design_scan"}>
              <Show
                when={props.scanId !== undefined}
                fallback={<p class="text-xs text-text-dim">{tl("Importa un modelo (un escaneo, un STL u OBJ) para tomar de él planos, cilindros y contornos.")}</p>}
              >
                <Section title={tl("Desde el escaneo")}>
                  <p class="text-[11px] text-text-dim leading-relaxed">
                    {tl("El escaneo se ve translúcido detrás del diseño. Elegir una zona ajusta un plano o un cilindro y lo convierte en operaciones.")}
                  </p>
                  <Show when={props.scans.length > 1}>
                    <label class="flex items-center gap-2 text-xs text-text-muted" aria-label={tl("Escaneo de referencia")}>
                      <span class="shrink-0">{tl("Escaneo")}</span>
                      <Select
                        options={props.scans.map((o) => ({ value: String(o.id), label: o.name }))}
                        value={String(props.scanId)}
                        onChange={(v) => void store.commit((d) => { d.scan_object = Number(v); })}
                      />
                    </label>
                  </Show>
                  <div class="space-y-1.5" aria-label={tl("Alinear el escaneo")}>
                    <span class="text-xs text-text-muted">{tl("Alinear el escaneo (antes de diseñar sobre él)")}</span>
                    <div class="grid grid-cols-2 gap-1.5">
                      <Button size="sm" variant="ghost" title={tl("La cara elegida queda apoyada en el piso")} onClick={() => align("floor")}>
                        {tl("Cara al piso")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("Gira en torno al eje vertical hasta que la cara mire al frente")} onClick={() => align("front")}>
                        {tl("Cara al frente")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("El eje del cilindro queda vertical y pasa por el origen")} onClick={() => align("axis")}>
                        {tl("Eje vertical")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("Corre el escaneo para que el punto quede en el origen")} onClick={() => align("origin")}>
                        {tl("Punto al origen")}
                      </Button>
                    </div>
                  </div>
                  <div class="space-y-1.5" aria-label={tl("Referencias del escaneo")}>
                    <span class="text-xs text-text-muted">{tl("Referencias ligadas (siguen al escaneo)")}</span>
                    <div class="grid grid-cols-2 gap-1.5">
                      <Button size="sm" variant="ghost" title={tl("Plano ajustado a una zona plana")} onClick={() => reference("plane")}>
                        {tl("Plano de zona")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("Plano por tres puntos del escaneo")} onClick={() => reference("points")}>
                        {tl("Plano 3 puntos")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("Eje de un cilindro o agujero")} onClick={() => reference("axis")}>
                        {tl("Eje de cilindro")}
                      </Button>
                      <Button size="sm" variant="ghost" title={tl("Un punto de la superficie")} onClick={() => reference("point")}>
                        {tl("Punto")}
                      </Button>
                    </div>
                  </div>
                  <div class="space-y-1.5" aria-label={tl("Escaneo como cuerpo")}>
                    <span class="text-xs text-text-muted">{tl("El escaneo como cuerpo (encajes, cunas, moldes)")}</span>
                    <Button
                      size="sm"
                      variant="ghost"
                      fullWidth
                      title={tl("El escaneo cerrado como sólido: restarlo de una pieza deja el hueco donde calza, con la holgura que se elija")}
                      onClick={() => {
                        // Con piezas, lo habitual es dejar el hueco donde calza el escaneo
                        const cut = !!store.result()?.body;
                        void store.addFeature({ type: "scan_body", offset: cut ? 0.2 : 0, faces: 20000, op: cut ? "cut" : "new" });
                      }}
                    >
                      {tl("Escaneo como cuerpo")}
                    </Button>
                    <div class="grid grid-cols-2 gap-1.5">
                      <Button
                        size="sm"
                        variant="ghost"
                        title={tl("Pieza que copia la piel del escaneo con un espesor (carcasa, funda)")}
                        onClick={() => actions.startScanShell(false)}
                      >
                        {tl("Carcasa")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        title={tl("Solo lo que cae bajo el contorno de un sketch, con espesor (férula, almohadilla, tapa que calza)")}
                        onClick={() => actions.startScanShell(true)}
                      >
                        {tl("Parche desde sketch")}
                      </Button>
                    </div>
                    <div class="flex items-end gap-1.5">
                      <Num label={tl("Radio del pincel")} value={brushRadius()} onCommit={(v) => v > 0 && setBrushRadius(v)} suffix="mm" />
                      <Button
                        size="sm"
                        variant="ghost"
                        title={tl("Pintar con el pincel la zona del escaneo que será el parche (el borde queda perpendicular a la piel)")}
                        onClick={paintPatch}
                      >
                        {tl("Pincel")}
                      </Button>
                    </div>
                    <div class="space-y-1.5" aria-label={tl("Molde")}>
                      <span class="text-xs text-text-muted">{tl("Molde de dos mitades")}</span>
                      <div class="grid grid-cols-2 gap-1.5">
                        <Select
                          options={[
                            { value: "2", label: tl("Parte en Z") },
                            { value: "1", label: tl("Parte en Y") },
                            { value: "0", label: tl("Parte en X") },
                          ]}
                          value={String(mold().axis)}
                          onChange={(v) => setMoldField("axis", Number(v))}
                        />
                        <Num label={tl("Partir en")} value={mold().at * 100} onCommit={(v) => v > 0 && v < 100 && setMoldField("at", v / 100)} suffix="%" />
                        <Num label={tl("Pared")} value={mold().wall} onCommit={(v) => v > 0 && setMoldField("wall", v)} suffix="mm" />
                        <Num label={tl("Holgura")} value={mold().gap} onCommit={(v) => setMoldField("gap", v)} suffix="mm" />
                        <Num label={tl("Bebedero (radio)")} value={mold().sprue} onCommit={(v) => v >= 0 && setMoldField("sprue", v)} suffix="mm" />
                        <label class="flex items-center gap-1.5 text-xs text-text-muted">
                          <input type="checkbox" checked={mold().keys} onChange={(e) => setMoldField("keys", e.currentTarget.checked)} />
                          {tl("Llaves de registro")}
                        </label>
                      </div>
                      <Button
                        size="sm"
                        variant="ghost"
                        fullWidth
                        title={tl("Dos cajas alrededor del escaneo partidas por un plano, con la cavidad del escaneo, llaves de registro y bebedero")}
                        onClick={() => void makeMold()}
                      >
                        {tl("Crear molde")}
                      </Button>
                    </div>
                    <p class="text-[11px] text-text-dim leading-relaxed">
                      {tl("Hasta la piel: extrusión «Hasta el escaneo», «Cara al escaneo» (Modificar) y nervio «Hasta el escaneo».")}
                    </p>
                  </div>
                  <span class="block text-xs text-text-muted">{tl("Tomar del escaneo")}</span>
                  <div class="grid grid-cols-2 gap-1.5">
                    <Button size="sm" onClick={() => pickScan("plane")}>
                      {tl("Cara plana")}
                    </Button>
                    <Button size="sm" onClick={() => pickScan("cylinder")}>
                      {tl("Cilindro / agujero")}
                    </Button>
                    <Button size="sm" onClick={() => pickScan("cone")}>
                      {tl("Cono")}
                    </Button>
                    <Button size="sm" onClick={() => pickScan("torus")}>
                      {tl("Toro")}
                    </Button>
                  </div>
                  <div class="space-y-1.5" aria-label={tl("Superficie libre")}>
                    <span class="text-xs text-text-muted">{tl("Superficie libre (zonas sin forma geométrica)")}</span>
                    <div class="flex items-end gap-1.5">
                      <Num label={tl("Espesor")} value={freeThickness()} step={0.5} suffix="mm" onCommit={setFreeThickness} />
                      <Button size="sm" variant="ghost" title={tl("Ajusta una superficie suave a la zona bajo el clic: sola, con espesor o cerrada contra una cara plana")} onClick={freeSurface}>
                        {tl("Elegir la zona")}
                      </Button>
                    </div>
                    <Checkbox small label={tl("Cerrar contra una cara plana (sólido)")} checked={freeClose()} onChange={setFreeClose} />
                  </div>
                  <div class="space-y-1.5 rounded-md border border-border p-2" aria-label={tl("Asistente de reconstrucción")}>
                    <span class="text-xs text-text-muted">{tl("Asistente de reconstrucción")}</span>
                    <Button size="sm" fullWidth disabled={assistBusy()} onClick={() => void propose()} title={tl("Detecta las zonas del escaneo y propone un árbol de operaciones: la base, agujeros, tetones, conos y toros")}>
                      {assistBusy() ? tl("Pensando…") : tl("Proponer un árbol")}
                    </Button>
                    <Show when={proposal()}>
                      {(p) => (
                        <div class="space-y-1">
                          <p class="text-[11px] text-text-dim">{tl("Explica el {v1} % del escaneo", { v1: fmt(p().covered * 100, 0) })}</p>
                          <For each={p().notes}>{(n) => <p class="text-[11px] text-warning">{n}</p>}</For>
                          <For each={p().steps}>
                            {(s) => (
                              <div class="flex items-start gap-1.5">
                                <input
                                  type="checkbox"
                                  class="mt-0.5"
                                  disabled={!s.creates}
                                  checked={marked().includes(s.id)}
                                  onChange={(e) => setMarked(e.currentTarget.checked ? [...marked(), s.id] : marked().filter((x) => x !== s.id))}
                                />
                                <button class="flex-1 text-left" title={s.detail} onClick={() => ui.setScanHighlight(s.faces)}>
                                  <span class={s.creates ? "text-xs text-text" : "text-xs text-text-dim"}>{s.label}</span>
                                  <span class="block text-[10px] text-text-dim">{s.detail}</span>
                                </button>
                              </div>
                            )}
                          </For>
                          <Button size="sm" variant="primary" fullWidth disabled={assistBusy() || marked().length === 0} onClick={() => void buildProposal()}>
                            {tl("Crear lo marcado")}
                          </Button>
                        </div>
                      )}
                    </Show>
                  </div>
                  <div class="space-y-1.5" aria-label={tl("Curvas sobre la piel")}>
                    <span class="text-xs text-text-muted">{tl("Curvas sobre la piel (caminos, bordes, nervios)")}</span>
                    <Show
                      when={pathPts()}
                      fallback={
                        <div class="grid grid-cols-2 gap-1.5">
                          <Button size="sm" variant="ghost" title={tl("Clic en varios puntos del escaneo: la curva sigue el camino más corto por la piel")} onClick={startPath}>
                            {tl("Camino por puntos")}
                          </Button>
                          <Button size="sm" variant="ghost" title={tl("Lleva las curvas de un sketch a la piel del escaneo, a lo largo de su normal")} onClick={projectToSkin}>
                            {tl("Sketch a la piel")}
                          </Button>
                        </div>
                      }
                    >
                      <div class="grid grid-cols-3 gap-1.5">
                        <Button size="sm" onClick={() => endPath(false)}>
                          {tl("Terminar")}
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => endPath(true)}>
                          {tl("Cerrada")}
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => endPath(null)}>
                          {tl("Cancelar")}
                        </Button>
                      </div>
                    </Show>
                  </div>
                  <Show when={scanResult()}>
                    {(r) => <ScanResultCard result={r()} onAdd={addScan} onTurned={(o, d) => void runScan(() => store.scanTurned(o, d))} onSkin={(d, n) => void runScan(() => store.scanSkin(d, n))} />}
                  </Show>
                  <div class="space-y-1.5">
                    <span class="text-xs text-text-muted">{tl("Corte del modelo por un plano")}</span>
                    <div class="flex gap-1.5">
                      <Select
                        options={(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] }))}
                        value={sliceBase()}
                        onChange={(v) => setSliceBase(v as "xy" | "xz" | "yz")}
                      />
                    </div>
                    <Num label={tl("A distancia")} value={sliceOffset()} onCommit={setSliceOffset} suffix="mm" />
                    <Button size="sm" fullWidth onClick={() => void addScan({ kind: "slice", plane: offsetPlane(sliceBase(), sliceOffset()) })}>
                      {tl("Sketch con el corte")}
                    </Button>
                  </div>
                  <Button size="sm" variant="ghost" fullWidth onClick={() => void detect()}>
                    {tl("Detectar planos y cilindros")}
                  </Button>
                  <DeviationCard store={store} ui={ui} />
                  <Show when={detections()}>
                    {(list) => (
                      <div class="max-h-48 overflow-y-auto space-y-0.5">
                        <For each={list()} fallback={<p class="text-xs text-text-dim">{tl("No se encontraron zonas")}</p>}>
                          {(d) => (
                            <div class="group flex items-center gap-1 rounded hover:bg-surface">
                              <button class="flex-1 flex justify-between gap-2 px-2 py-1 text-xs text-left" onClick={() => ui.setScanHighlight(d.faces)}>
                                <span class="text-text">
                                  {d.shape.type === "plane"
                                    ? tl("Plano")
                                    : d.shape.type === "cylinder"
                                      ? tl("Cilindro r {v1}", { v1: fmt(d.shape.radius) })
                                      : d.shape.type === "cone"
                                        ? tl("Cono {v1}°", { v1: fmt((d.shape.half_angle * 180) / Math.PI, 1) })
                                        : d.shape.type === "torus"
                                          ? tl("Toro R {v1} r {v2}", { v1: fmt(d.shape.major), v2: fmt(d.shape.minor) })
                                          : d.shape.type === "fillet"
                                            ? tl("Redondeo r {v1} · {v2}°", { v1: fmt(d.shape.radius), v2: fmt(d.shape.angle, 0) })
                                            : tl("Esfera r {v1}", { v1: fmt(d.shape.radius) })}
                                </span>
                                <span class="text-text-dim font-mono">{fmt(d.area, 0)} mm²</span>
                              </button>
                              <Show when={d.shape.type !== "sphere" && d.shape.type !== "fillet"}>
                                <button
                                  class="hidden group-hover:block px-1.5 text-[11px] text-accent hover:text-accent-hover"
                                  title={d.shape.type === "plane" ? tl("Agregar el contorno como sketch") : d.shape.type === "cone" ? tl("Agregar el cono") : d.shape.type === "torus" ? tl("Agregar el toro") : tl("Agregar el cilindro")}
                                  onClick={() =>
                                    void addScan(
                                      d.shape.type === "plane"
                                        ? { kind: "plane_outline", triangle: d.faces[0], extrude: false }
                                        : { kind: d.shape.type as "cylinder" | "cone" | "torus", triangle: d.faces[0] },
                                    )
                                  }
                                >
                                  {tl("Agregar")}
                                </button>
                              </Show>
                            </div>
                          )}
                        </For>
                      </div>
                    )}
                  </Show>
                </Section>
              </Show>

            </Match>
          </Switch>
        </Show>
      </Show>
    </div>
  );
};

const NoKernel: Component = () => (
  <div class="space-y-2">
    <p class="text-xs text-warning">{tl("Esta compilación no incluye el núcleo CAD.")}</p>
    <p class="text-[11px] text-text-dim leading-relaxed">
      {tl("El diseño paramétrico usa OpenCASCADE. Las versiones publicadas lo traen incluido; al compilar a mano hay que instalarlo (en Arch: pacman -S opencascade).")}
    </p>
  </div>
);

// ─── Parámetros ───────────────────────────────────────────────────────────

const NAME_RE = /^[\p{L}_][\p{L}\p{N}_]*$/u;

const ParametersSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const params = () => store.doc()?.parameters ?? [];
  const [problem, setProblem] = createSignal<string>();
  const add = () => {
    let n = params().length + 1;
    while (params().some((p) => p.name === `p${n}`)) n++;
    void store.addParameter(`p${n}`, "10");
  };
  return (
    <Section
      title={tl("Parámetros")}
      right={
        <IconButton aria-label={tl("Agregar parámetro")} size="sm" variant="ghost" onClick={add}>
          <Icons.Plus size={12} />
        </IconButton>
      }
    >
      <Show
        when={params().length > 0}
        fallback={
          <p class="text-[11px] text-text-dim leading-relaxed">
            {tl("Medidas con nombre para usar en cualquier campo: escribir")} <span class="font-mono">{tl("ancho / 2")}</span> {tl("en una cota o una distancia la deja atada al parámetro.")}
          </p>
        }
      >
        <div class="space-y-1">
          <For each={params()}>
            {(p, i) => {
              const res = () => store.parameterResult(p.name);
              return (
                <div class="flex items-center gap-1.5">
                  <input
                    value={p.name}
                    class="w-20 shrink-0 px-1.5 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => {
                      const name = e.currentTarget.value.trim();
                      if (name === p.name) return;
                      if (!NAME_RE.test(name)) return setProblem(tl("«{name}» no sirve como nombre (letras, números y _)", { name }));
                      if (params().some((q, j) => j !== i() && q.name === name)) return setProblem(tl("ya hay un parámetro «{name}»", { name }));
                      setProblem(undefined);
                      void store.updateParameter(i(), { name });
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class="text-text-dim text-xs">=</span>
                  <input
                    value={p.expr}
                    class={clsx(
                      "flex-1 min-w-0 px-1.5 py-1 rounded bg-surface/40 border text-xs text-text font-mono outline-none focus:border-accent",
                      res()?.error ? "border-error" : "border-border",
                    )}
                    title={res()?.error ?? undefined}
                    onChange={(e) => {
                      const expr = e.currentTarget.value.trim();
                      if (expr && expr !== p.expr) void store.updateParameter(i(), { expr });
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class={clsx("w-14 text-right text-xs font-mono truncate", res()?.error ? "text-error" : "text-text-muted")} title={res()?.error ?? undefined}>
                    {res()?.error ? "error" : res()?.value != null ? fmt(res()!.value!, 3).replace(/,?0+$/, "") : ""}
                  </span>
                  <IconButton aria-label={tl("Quitar parámetro")} size="sm" variant="ghost" onClick={() => void store.removeParameter(i())}>
                    <Icons.X size={10} />
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <Show when={problem()}>
        <p class="text-[11px] text-error">{problem()}</p>
      </Show>
    </Section>
  );
};

/**
 * Configuraciones: variantes de la misma pieza (M3, M4, M5…). Cada una
 * reemplaza parámetros y suprime operaciones; la elegida es la que se ve y
 * se exporta.
 */
/** Fecha corta: "8 oct 2026, 14:05" */
const versionDate = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString(locale(), { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
};

/** Versiones con nombre: instantáneas del diseño para volver a verlas, compararlas o restaurarlas */
const VersionsSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const versions = () => store.committed()?.versions ?? [];
  const [name, setName] = createSignal("");
  const [note, setNote] = createSignal("");
  const save = () => {
    void store.saveVersion(name() || nextVersionName(versions()), note());
    setName("");
    setNote("");
  };
  const input = "min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent";
  return (
    <Section title={tl("Versiones")}>
      <div class="space-y-1.5" data-versions>
        <div class="flex gap-1">
          <input
            aria-label={tl("Nombre de la versión")}
            placeholder={nextVersionName(versions())}
            value={name()}
            class={clsx(input, "flex-1")}
            onInput={(e) => setName(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
          />
          <Button size="sm" onClick={save} disabled={store.viewing() !== undefined}>
            {tl("Guardar versión")}
          </Button>
        </div>
        <input
          aria-label={tl("Nota de la versión")}
          placeholder={tl("Nota (opcional)")}
          value={note()}
          class={clsx(input, "w-full")}
          onInput={(e) => setNote(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        <Show
          when={versions().length > 0}
          fallback={
            <p class="text-[11px] text-text-dim leading-relaxed">
              {tl("Guarda cómo está el diseño (\"v1 enviada a imprimir\") para verlo después, compararlo con el de ahora o volver a él.")}
            </p>
          }
        >
          <For each={versions().map((v, i) => ({ v, i })).reverse()}>
            {(row) => (
              <div
                data-version={row.v.name}
                class={clsx(
                  "rounded border px-1.5 py-1 space-y-0.5",
                  store.viewing() === row.i ? "border-warning/70 bg-warning/5" : "border-border",
                )}
              >
                <div class="flex items-center gap-1">
                  <input
                    aria-label={tl("Nombre de {name}", { name: row.v.name })}
                    value={row.v.name}
                    class={clsx(input, "flex-1 border-transparent bg-transparent font-medium")}
                    onChange={(e) => void store.renameVersion(row.i, e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class="text-[10px] text-text-dim shrink-0">{versionDate(row.v.created)}</span>
                </div>
                <Show when={row.v.note}>
                  <p class="text-[11px] text-text-muted px-1.5">{row.v.note}</p>
                </Show>
                <div class="flex items-center gap-0.5 -mx-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`Ver ${row.v.name}`}
                    onClick={() => void store.viewVersion(store.viewing() === row.i ? undefined : row.i)}
                  >
                    {store.viewing() === row.i ? tl("Volver") : tl("Ver")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={tl("Comparar con {name}", { name: row.v.name })}
                    onClick={() => void store.compareVersion(store.comparison()?.version === row.i ? undefined : row.i)}
                  >
                    {store.comparison()?.version === row.i ? tl("Ocultar") : tl("Comparar")}
                  </Button>
                  <Button size="sm" variant="ghost" aria-label={tl("Restaurar {name}", { name: row.v.name })} onClick={() => void store.restoreVersion(row.i)}>
                    {tl("Restaurar")}
                  </Button>
                  <span class="flex-1" />
                  <IconButton aria-label={tl("Quitar {name}", { name: row.v.name })} size="sm" variant="ghost" onClick={() => void store.deleteVersion(row.i)}>
                    <Icons.X size={10} />
                  </IconButton>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </Section>
  );
};

const ConfigurationsSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const configs = () => store.doc()?.configurations ?? [];
  const active = () => store.doc()?.active_configuration ?? null;
  const params = () => store.doc()?.parameters ?? [];
  const features = () => store.doc()?.features ?? [];
  const add = () => {
    let n = configs().length + 1;
    while (configs().some((c) => c.name === `Variante ${n}`)) n++;
    const index = configs().length;
    void store.commit((d) => {
      d.configurations = [...(d.configurations ?? []), { name: tl("Variante {n}", { n }), values: {}, suppressed: [] }];
      d.active_configuration = index;
    });
  };
  const edit = (index: number, change: (c: Configuration) => void) =>
    void store.commit((d) => {
      const c = d.configurations?.[index];
      if (c) change(c);
    });
  return (
    <Section
      title={tl("Configuraciones")}
      right={
        <IconButton aria-label={tl("Agregar configuración")} size="sm" variant="ghost" onClick={add}>
          <Icons.Plus size={12} />
        </IconButton>
      }
    >
      <Show
        when={configs().length > 0}
        fallback={
          <p class="text-[11px] text-text-dim leading-relaxed">
            {tl("Variantes de la misma pieza (M3, M4, M5; con o sin un agujero): cada una cambia parámetros y apaga operaciones. Se pueden exportar todas juntas.")}
          </p>
        }
      >
        <div class="space-y-1" data-configs>
          <For each={[{ name: tl("Base"), index: null as number | null }, ...configs().map((c, i) => ({ name: c.name, index: i as number | null }))]}>
            {(row) => (
              <div class="flex items-center gap-1.5">
                <button
                  class={clsx(
                    "w-3 h-3 shrink-0 rounded-full border",
                    active() === row.index ? "bg-accent border-accent" : "border-border hover:border-accent",
                  )}
                  aria-label={tl("Usar {name}", { name: row.name })}
                  onClick={() => void store.commit((d) => (d.active_configuration = row.index))}
                />
                <Show when={row.index !== null} fallback={<span class="flex-1 text-xs text-text-muted">{tl("Base")}</span>}>
                  <input
                    value={row.name}
                    class="flex-1 min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                    onChange={(e) => {
                      const name = e.currentTarget.value.trim();
                      if (name && name !== row.name) edit(row.index!, (c) => (c.name = name));
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <IconButton
                    aria-label={tl("Quitar {name}", { name: row.name })}
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      void store.commit((d) => {
                        d.configurations = (d.configurations ?? []).filter((_, i) => i !== row.index);
                        const a = d.active_configuration;
                        if (a === row.index) d.active_configuration = null;
                        else if (a != null && a > row.index!) d.active_configuration = a - 1;
                      })
                    }
                  >
                    <Icons.X size={10} />
                  </IconButton>
                </Show>
              </div>
            )}
          </For>
        </div>
        {/* La elegida: qué cambia respecto de lo de base */}
        <Show when={active() !== null && configs()[active()!]}>
          {(c) => (
            <div class="space-y-1.5 rounded-md border border-border p-2" data-config-editor>
              <Show when={params().length} fallback={<p class="text-[11px] text-text-dim">{tl("Sin parámetros: crear alguno arriba para variar medidas.")}</p>}>
                <For each={params()}>
                  {(p) => (
                    <label class="flex items-center gap-1.5 text-xs text-text-muted">
                      <span class="w-20 shrink-0 font-mono truncate">{p.name}</span>
                      <input
                        value={c().values?.[p.name] ?? ""}
                        placeholder={p.expr}
                        aria-label={tl("{param} en {config}", { param: p.name, config: c().name })}
                        class="flex-1 min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent placeholder:text-text-dim"
                        onChange={(e) => {
                          const v = e.currentTarget.value.trim();
                          edit(active()!, (x) => {
                            x.values = { ...(x.values ?? {}) };
                            if (v) x.values[p.name] = v;
                            else delete x.values[p.name];
                          });
                        }}
                        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                      />
                    </label>
                  )}
                </For>
              </Show>
              <p class="text-[11px] text-text-dim">{tl("Vacío: el valor de base. Operaciones apagadas en esta variante:")}</p>
              <div class="max-h-40 overflow-y-auto space-y-0.5">
                <For each={features()}>
                  {(f) => (
                    <Checkbox
                      small
                      label={f.name}
                      checked={(c().suppressed ?? []).includes(f.id)}
                      onChange={(on) =>
                        edit(active()!, (x) => {
                          const s = new Set(x.suppressed ?? []);
                          if (on) s.add(f.id);
                          else s.delete(f.id);
                          x.suppressed = [...s];
                        })
                      }
                    />
                  )}
                </For>
              </div>
            </div>
          )}
        </Show>
      </Show>
    </Section>
  );
};

// ─── Árbol ────────────────────────────────────────────────────────────────

/** Duración corta: "120 ms", "1,4 s" */
const formatMs = (ms: number) => (ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toLocaleString(locale(), { maximumFractionDigits: 1 })} s`);

/** Mensaje de error o advertencia de una operación (nada si está bien) */
const problem = (st: FeatureState | undefined) => (st?.state === "error" || st?.state === "warning" ? st.message : undefined);

/** Cómo se marca una operación en el árbol: con problemas o un sketch suelto en rojo, un sketch vacío en azul */
function featureTone(f: Feature, st: FeatureState | undefined): { tone: "error" | "empty"; why: string } | undefined {
  const message = problem(st);
  if (message) return { tone: "error", why: message };
  const k = f.kind;
  if (k.type !== "sketch" && k.type !== "surface_sketch") return undefined;
  if (k.sketch.entities.length === 0 && !(k.sketch.texts ?? []).length) return { tone: "empty", why: tl("Sketch vacío") };
  if (k.type === "sketch" && k.plane.type === "custom") return { tone: "error", why: tl("El plano del sketch no está ligado a ninguna referencia") };
  if (!sketchAnchored(k.sketch)) return { tone: "error", why: tl("El dibujo no está ligado a ninguna referencia (origen, punto fijo o arista del modelo): se puede mover entero") };
  return undefined;
}

/**
 * Arrastre vertical dentro del árbol: devuelve el hueco bajo el puntero (0 =
 * antes de la primera fila, n = después de la última). Un movimiento de menos
 * de 4 px es un clic.
 */
type TreeDrag = { kind: "feature"; id: number; from: number; to: number } | { kind: "rollback"; to: number };

/** Árbol de operaciones (va en el grupo Operaciones del Outliner) */
export const FeatureTree: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const doc = () => store.doc();
  const count = () => doc()?.features.length ?? 0;
  const rollback = () => doc()?.rollback ?? null;
  /** Sketches con una región o una curva elegida en el visor: su fila se marca como elegida */
  const pickedSketches = createMemo(() => new Set(props.ui.picks().flatMap((p) => (p.kind === "region" || p.kind === "sketch" ? [p.sketch] : []))));
  /** Hueco donde está la barra de retroceso (al final si no hay) */
  const barAt = () => rollback() ?? count();
  let list: HTMLDivElement | undefined;
  const [drag, setDrag] = createSignal<TreeDrag>();
  // Tras arrastrar una fila, el click que sigue no la abre
  let swallowClick = false;
  const gapAt = (y: number) => {
    // Las filas de una carpeta plegada no se ven: cuentan como su cabecera
    let ref = -Infinity;
    let n = 0;
    for (const el of list?.querySelectorAll<HTMLElement>("[data-feature-row], [data-folder]") ?? []) {
      const b = el.getBoundingClientRect();
      const visible = !("hidden" in el.dataset);
      if (visible) ref = b.top + b.height / 2;
      if ("featureRow" in el.dataset && ref < y) n++;
    }
    return n;
  };
  /** Sigue el puntero hasta soltar; `start` decide qué se arrastra al pasar el umbral */
  const track = (e: PointerEvent, start: () => TreeDrag, drop: (d: TreeDrag) => void) => {
    if (e.button !== 0) return;
    const y0 = e.clientY;
    let moving = false;
    const move = (ev: PointerEvent) => {
      if (!moving && Math.abs(ev.clientY - y0) < 4) return;
      moving = true;
      setDrag({ ...start(), to: gapAt(ev.clientY) });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const d = drag();
      setDrag(undefined);
      if (moving && d) {
        swallowClick = true;
        setTimeout(() => (swallowClick = false), 0);
        drop(d);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const dropFeature = (d: TreeDrag) => {
    if (d.kind !== "feature") return;
    if (!store.moveFeatureTo(d.id, d.to)) props.ui.setMessage(tl("No se puede: quedaría antes de lo que necesita o después de lo que la usa"));
  };
  const dropBar = (d: TreeDrag) => void store.setRollback(d.to >= count() ? null : d.to);
  // Carpetas: tramo [desde, hasta] de cada una y en cuál está cada fila
  const folders = createMemo(() => {
    const d = doc();
    return d ? (d.folders ?? []).map((f, index) => ({ f, index, range: folderRange(d, f) })).filter((x) => x.range) : [];
  });
  const folderAt = (i: number) => folders().find((x) => x.range![0] <= i && i <= x.range![1]);
  // Mayús+clic: tramo de filas desde la elegida (para agrupar)
  const [rangeTo, setRangeTo] = createSignal<number>();
  const selRange = (): [number, number] | null => {
    const d = doc();
    const a = d?.features.findIndex((f) => f.id === store.selected()) ?? -1;
    if (a < 0) return null;
    const b = rangeTo() !== undefined ? d!.features.findIndex((f) => f.id === rangeTo()) : a;
    return b < 0 ? [a, a] : a <= b ? [a, b] : [b, a];
  };
  const inRange = (i: number) => {
    const r = selRange();
    return rangeTo() !== undefined && r !== null && r[0] <= i && i <= r[1];
  };
  // Copiar y pegar operaciones: Ctrl+C con la elegida (o el tramo con Mayús) y Ctrl+V
  const selectedIds = () => {
    const r = selRange();
    return r ? doc()!.features.slice(r[0], r[1] + 1).map((f) => f.id) : [];
  };
  const copyOps = () => {
    const n = store.copyFeatures(selectedIds());
    props.ui.setMessage(n === 1 ? tl("Operación copiada: Ctrl+V para pegar") : n ? tl("{n} operaciones copiadas: Ctrl+V para pegar", { n }) : tl("Elegir en el árbol lo que se copia"));
  };
  const pasteOps = async (list?: Feature[]) => {
    const ids = await store.pasteFeatures(list);
    if (ids.length) props.ui.setMessage((ids.length === 1 ? tl("Pegada una operación (lo que usaban de afuera queda igual)") : tl("Pegadas {n} operaciones (lo que usaban de afuera queda igual)", { n: ids.length })));
  };
  const onTreeKey = (e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || props.ui.session()) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    const key = e.key.toLowerCase();
    if (key === "c" && store.selected() !== undefined) {
      e.preventDefault();
      copyOps();
    } else if (key === "v" && store.featureClip()) {
      e.preventDefault();
      void pasteOps();
    }
  };
  window.addEventListener("keydown", onTreeKey);
  onCleanup(() => window.removeEventListener("keydown", onTreeKey));
  const [renaming, setRenaming] = createSignal<number>();
  /** Dónde se dibuja la línea del hueco mientras se arrastra */
  const dropGap = () => {
    const d = drag();
    if (!d) return null;
    if (d.kind === "feature" && (d.to === d.from || d.to === d.from + 1)) return null;
    return d.to;
  };
  // La barra: en su hueco, o donde se la está llevando
  const barGap = () => {
    const d = drag();
    return d?.kind === "rollback" ? d.to : barAt();
  };
  const Gap = (p: { index: number }) => (
    <>
      <Show when={dropGap() === p.index && drag()?.kind === "feature"}>
        <div class="h-0.5 rounded bg-accent mx-1" />
      </Show>
      <Show when={barGap() === p.index}>
        <div
          aria-label={tl("Barra de retroceso")}
          data-rollback-bar
          class="group/bar relative py-1 cursor-ns-resize touch-none"
          title={rollback() === null ? tl("Arrastrar hacia arriba para ver el diseño en un paso anterior") : tl("Barra de retroceso: lo de abajo no se calcula y lo nuevo se agrega acá")}
          onPointerDown={(e) => {
            e.preventDefault();
            track(e, () => ({ kind: "rollback", to: barAt() }), dropBar);
          }}
        >
          <div
            class={clsx(
              "h-1 rounded mx-1 transition-colors",
              rollback() !== null || drag()?.kind === "rollback" ? "bg-warning" : "bg-border group-hover/bar:bg-warning/60",
            )}
          />
        </div>
      </Show>
    </>
  );
  return (
    <div data-feature-tree>
      <Show when={count() > 0} fallback={<p class="px-1 text-xs text-text-dim">{tl("Todavía no hay operaciones")}</p>}>
        <div class="space-y-0.5" ref={list}>
          {/* Por id: las filas no se rehacen cuando el documento se clona (un
              clic entre mousedown y mouseup se perdería) */}
          <For each={doc()?.features.map((x) => x.id) ?? []}>
            {(id, i) => {
              const f = () => doc()?.features[i()] ?? ({ id, name: "", suppressed: false, kind: { type: "sketch" } } as unknown as Feature);
              const state = () => store.stateOf(f().id);
              const tone = createMemo(() => featureTone(f(), state()));
              const rolled = () => (rollback() !== null && i() >= rollback()!) || state()?.state === "rolled_back";
              const sketchHidden = () => editsAsSketch(f().kind) && props.ui.hiddenSketches().includes(f().id);
              const folder = () => folderAt(i());
              // Plegada en su carpeta, o no coincide con el buscador del Outliner (que abre las carpetas)
              const hidden = () =>
                outlinerQuery().trim() ? !matchesOutliner(f().name) : !!folder()?.f.collapsed && folder()!.range![0] !== undefined;
              return (
                <>
                  <Show when={folder() && folder()!.range![0] === i() && folder()}>
                    {(fo) => (
                      <div
                        data-folder
                        class="group flex items-center gap-1.5 px-1 py-1 rounded text-xs text-text hover:bg-surface cursor-pointer select-none"
                        onClick={() => void store.updateFolder(fo().index, { collapsed: !fo().f.collapsed })}
                      >
                        <Icons.CaretDown size={10} class={clsx("shrink-0 transition-transform", fo().f.collapsed && "-rotate-90")} />
                        <Icons.Folder size={13} class="shrink-0 text-text-muted" />
                        <Show
                          when={renaming() === fo().index}
                          fallback={
                            <span class="flex-1 truncate" onDblClick={(e) => (e.stopPropagation(), setRenaming(fo().index))}>
                              {fo().f.name}
                            </span>
                          }
                        >
                          <input
                            aria-label={tl("Nombre de la carpeta")}
                            class="flex-1 min-w-0 px-1 rounded bg-surface/40 border border-accent text-xs text-text outline-none"
                            value={fo().f.name}
                            ref={(el) => setTimeout(() => el.select())}
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                            onBlur={(e) => {
                              const name = e.currentTarget.value.trim();
                              setRenaming(undefined);
                              if (name && name !== fo().f.name) void store.updateFolder(fo().index, { name });
                            }}
                          />
                        </Show>
                        {/* Plegada: avisa si algo de adentro tiene problemas */}
                        <Show
                          when={
                            fo().f.collapsed &&
                            doc()!
                              .features.slice(fo().range![0], fo().range![1] + 1)
                              .some((x) => featureTone(x, store.stateOf(x.id))?.tone === "error")
                          }
                        >
                          <Icons.Warning size={12} class="text-error shrink-0" aria-label={tl("Algo de la carpeta tiene problemas")} />
                        </Show>
                        <span class="hidden group-hover:flex" onClick={(e) => e.stopPropagation()}>
                          <IconButton aria-label={tl("Desagrupar")} size="sm" variant="ghost" onClick={() => void store.updateFolder(fo().index, null)}>
                            <Icons.X size={10} />
                          </IconButton>
                        </span>
                      </div>
                    )}
                  </Show>
                  <Gap index={i()} />
                  <div
                    data-feature-row
                    data-hidden={hidden() ? "" : undefined}
                    class={clsx(
                      "group flex items-center gap-1.5 px-2 py-1 rounded text-xs cursor-pointer select-none",
                      folder() && "ml-3",
                      hidden() && "hidden",
                      store.selected() === f().id || inRange(i()) || pickedSketches().has(f().id) ? "bg-accent/20 text-text" : "hover:bg-surface text-text-muted",
                      (f().suppressed || rolled()) && "opacity-50",
                      drag()?.kind === "feature" && (drag() as { id: number }).id === f().id && "ring-1 ring-accent",
                    )}
                    onPointerDown={(e) => track(e, () => ({ kind: "feature", id: f().id, from: i(), to: i() }), dropFeature)}
                    onClick={(e) => {
                      if (swallowClick) return;
                      // Mayús: tramo desde la elegida, sin abrir el diálogo
                      if (e.shiftKey && store.selected() !== undefined && store.selected() !== f().id) return setRangeTo(f().id);
                      setRangeTo(undefined);
                      store.select(store.selected() === f().id ? undefined : f().id);
                    }}
                    onDblClick={() => editsAsSketch(f().kind) && void store.settled().then(() => props.ui.editSketch(f().id))}
                    onMouseEnter={() => props.ui.setHoverFeature(f().id)}
                    onMouseLeave={() => props.ui.hoverFeature() === f().id && props.ui.setHoverFeature(undefined)}
                    title={tone()?.why}
                  >
                    <Show
                      when={problem(state())}
                      fallback={<span class="w-3.5 text-center text-text-dim">{FEATURE_LABELS[f().kind.type][0]}</span>}
                    >
                      <Icons.Warning size={14} aria-label={state()?.state === "warning" ? tl("Advertencia") : tl("Error")} class="shrink-0 text-error" />
                    </Show>
                    <span
                      data-tone={tone()?.tone}
                      data-sketch-hidden={sketchHidden() ? "" : undefined}
                      class={clsx(
                        "flex-1 truncate",
                        f().suppressed && "line-through",
                        tone()?.tone === "error" && "text-error",
                        tone()?.tone === "empty" && "text-sketch-free",
                        // Sketch oculto en el visor: atenuado (los que se ven, con su color)
                        sketchHidden() && "italic text-text-dim",
                      )}
                    >
                      {f().name}
                    </span>
                    <Show when={sketchHidden()}>
                      <Icons.EyeSlash size={12} class="shrink-0 text-text-dim group-hover:hidden" aria-label={tl("Oculto en el visor")} />
                    </Show>
                    {/* Las lentas dicen cuánto tardan (para saber qué frena el diseño) */}
                    <Show when={(state()?.ms ?? 0) >= 100}>
                      <span class="text-[10px] font-mono text-text-dim group-hover:hidden" title={tl("Lo que tardó la última vez que se calculó")}>
                        {formatMs(state()!.ms!)}
                      </span>
                    </Show>
                    <span class="hidden group-hover:flex items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
                      <IconButton
                        aria-label={tl("Duplicar")}
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          const ids = inRange(i()) ? selectedIds() : [f().id];
                          void pasteOps(doc()!.features.filter((x) => ids.includes(x.id)));
                        }}
                      >
                        <Icons.Copy size={11} />
                      </IconButton>
                      <IconButton aria-label={tl("Subir")} size="sm" variant="ghost" onClick={() => !store.moveFeature(f().id, -1) && props.ui.setMessage(tl("No se puede: quedaría antes de lo que necesita"))}>
                        <Icons.CaretDown size={10} class="rotate-180" />
                      </IconButton>
                      <IconButton aria-label={tl("Bajar")} size="sm" variant="ghost" onClick={() => !store.moveFeature(f().id, 1) && props.ui.setMessage(tl("No se puede: otra operación la necesita antes"))}>
                        <Icons.CaretDown size={10} />
                      </IconButton>
                      <Show
                        when={["sketch", "plane", "axis", "point", "frame"].includes(f().kind.type)}
                        fallback={
                          <IconButton
                            aria-label={f().suppressed ? tl("Activar") : tl("Suprimir")}
                            size="sm"
                            variant="ghost"
                            onClick={() => void store.updateFeature(f().id, (x) => (x.suppressed = !x.suppressed))}
                          >
                            {f().suppressed ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                          </IconButton>
                        }
                      >
                        <IconButton
                          aria-label={props.ui.hiddenSketches().includes(f().id) ? tl("Mostrar en el visor") : tl("Ocultar en el visor")}
                          size="sm"
                          variant="ghost"
                          onClick={() => props.ui.toggleSketchVisible(f().id)}
                        >
                          {props.ui.hiddenSketches().includes(f().id) ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                        </IconButton>
                      </Show>
                      <IconButton
                        aria-label={tl("Borrar")}
                        size="sm"
                        variant="ghost"
                        onClick={() => !store.removeFeature(f().id) && props.ui.setMessage(tl("Otras operaciones dependen de esta: borrarlas primero"))}
                      >
                        <Icons.Trash size={12} />
                      </IconButton>
                    </span>
                  </div>
                </>
              );
            }}
          </For>
          <Gap index={count()} />
        </div>
        <div class="flex flex-wrap gap-1.5">
          <Show when={store.selected() !== undefined}>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                const i = doc()!.features.findIndex((f) => f.id === store.selected());
                void store.setRollback(i + 1);
              }}
            >
              {tl("Retroceder hasta acá")}
            </Button>
            <Tooltip content={tl("Mayús+clic en otra fila elige un tramo")}>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const r = selRange();
                  if (!r) return;
                  if (!store.addFolder(r[0], r[1])) return props.ui.setMessage(tl("Se pisa con otra carpeta"));
                  setRangeTo(undefined);
                }}
              >
                {tl("Agrupar en carpeta")}
              </Button>
            </Tooltip>
          </Show>
          <Show when={rollback() !== null}>
            <Button size="sm" variant="ghost" onClick={() => void store.setRollback(null)}>
              {tl("Calcular todo")}
            </Button>
          </Show>
        </div>
      </Show>
    </div>
  );
};

// ─── Edición de una operación ─────────────────────────────────────────────

export const FeatureEditor: Component<{
  featureId: number;
  store: CadStore;
  ui: CadUi;
  sketches: Feature[];
  tools: Feature[];
}> = (props) => {
  // Al cerrarse el diálogo la operación puede desaparecer antes que el editor:
  // se queda con la última
  const f = createMemo<Feature>((prev) => props.store.doc()?.features.find((x) => x.id === props.featureId) ?? prev!);
  const update = (mutate: (k: FeatureKind) => void) => void props.store.updateFeature(f().id, (x) => mutate(x.kind));
  /** Campo numérico vinculable: `path` relativo a la operación (p. ej. "kind.radius") */
  const field = (label: string, rel: string, raw: number, write: (k: FeatureKind, v: number) => void, suffix?: string) => {
    const path = () => `${f().id}.${rel}`;
    const res = () => props.store.bindingResult(path());
    // El valor de ahora de la operación (del borrador con el diálogo abierto), leído por su
    // ruta: `raw` es el del momento en que se armó el campo y no se actualiza (el campo
    // volvía a mostrar el viejo y la rueda bajaba de a uno desde ahí)
    const live = () => {
      let v: unknown = f();
      for (const k of rel.split(".")) v = v == null ? undefined : (v as Record<string, unknown>)[k];
      return typeof v === "number" ? v : raw;
    };
    return (
      <Formula
        label={label}
        suffix={suffix}
        value={res()?.value ?? live()}
        expr={props.store.bindingOf(path())}
        error={res()?.error}
        onCommit={(text) => props.store.setField(f().id, path(), text, (x, v) => write(x.kind, v))}
      />
    );
  };
  const state = () => props.store.stateOf(f().id);
  /** Posiciones de las referencias de un campo que no se encontraron */
  const lost = (field: MissingRef["field"]) => {
    const st = state();
    return st?.state === "error" || st?.state === "warning" ? (st.missing ?? []).filter((m) => m.field === field).map((m) => m.index) : [];
  };
  const sketchOptions = () => props.sketches.map((s) => ({ value: String(s.id), label: s.name }));
  const planeSpecSelect = (value: PlaneSpec, set: (p: PlaneSpec) => void, lostFace = false) => {
    const current = refValue(value) ?? value.type;
    return (
      <Select
        options={[
          ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
          ...refOptions(props.store, "plane", f().id),
          ...(value.type === "face" ? [{ value: "face", label: lostFace ? tl("Cara del sólido · no encontrada") : tl("Cara del sólido") }] : []),
          ...(value.type === "custom" ? [{ value: "custom", label: tl("Plano propio") }] : []),
        ]}
        value={current}
        onChange={(v) => {
          if (v === current) return;
          if (v === "xy" || v === "xz" || v === "yz") set({ type: v });
          else if (isRefValue(v)) set(planeOfValue(v));
        }}
      />
    );
  };
  const opSelect = (value: BodyOp, set: (o: BodyOp) => void) => <Select options={OP_OPTIONS} value={value} onChange={(v) => set(v as BodyOp)} />;
  // Diálogo abierto: los cambios se ven en el visor y quedan al aceptar
  const open = () => props.store.draft()?.feature === f().id;

  return (
    <Section
      title={FEATURE_LABELS[f().kind.type]}
      class={clsx(open() && "rounded-md border border-accent/50 bg-accent/5 p-2")}
      right={
        <Show when={open()}>
          <div class="flex items-center gap-1">
            <Show when={props.store.busy()}>
              <span class="text-[11px] text-text-dim">{tl("Calculando…")}</span>
            </Show>
            <Tooltip content="Aceptar (Enter)">
              <IconButton aria-label={tl("Aceptar")} size="sm" variant="primary" onClick={() => void props.store.acceptDraft()}>
                <Icons.Check size={12} />
              </IconButton>
            </Tooltip>
            <Tooltip content="Cancelar (Esc)">
              <IconButton aria-label={tl("Cancelar")} size="sm" onClick={() => void props.store.cancelDraft()}>
                <Icons.X size={12} />
              </IconButton>
            </Tooltip>
          </div>
        </Show>
      }
    >
      <div class="space-y-2">
        <input
          class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          value={f().name}
          onChange={(e) => void props.store.updateFeature(f().id, (x) => (x.name = e.currentTarget.value))}
        />
        <Show when={problem(state())}>
          {(msg) => <p class={clsx("text-xs", state()?.state === "warning" ? "text-warning" : "text-error")}>{msg()}</p>}
        </Show>
        <Switch>
          <Match when={f().kind.type === "sketch" && (f().kind as Extract<FeatureKind, { type: "sketch" }>)}>
            {(k) => {
              const view = () => props.store.sketchView(f().id);
              /**
               * Cambia la horizontal del sketch y gira el dibujo al revés lo mismo
               * que gira el plano: queda en su lugar del espacio
               */
              const setHorizontal = async (axis: AxisSpec | null) => {
                const id = f().id;
                const normal = view()?.plane.normal;
                let turn = 0;
                try {
                  const [from, to] = await Promise.all([props.store.sketchX(id, k().sketch.x_axis ?? null), props.store.sketchX(id, axis)]);
                  if (normal) {
                    const c = [from[1] * to[2] - from[2] * to[1], from[2] * to[0] - from[0] * to[2], from[0] * to[1] - from[1] * to[0]];
                    const sin = c[0] * normal[0] + c[1] * normal[1] + c[2] * normal[2];
                    const cos = from[0] * to[0] + from[1] * to[1] + from[2] * to[2];
                    turn = (Math.atan2(sin, cos) * 180) / Math.PI;
                  }
                } catch (err) {
                  return props.ui.setMessage(String(err));
                }
                void props.store.commit((d) => {
                  const ft = d.features.find((x) => x.id === id);
                  if (ft?.kind.type !== "sketch") return;
                  ft.kind.sketch.x_axis = axis;
                  rotateSketch(ft.kind.sketch, -turn);
                  // Las regiones elegidas por otras operaciones giran igual
                  moveRegionPoints(d, id, rotation([0, 0], -turn));
                });
              };
              return (
                <>
                  <Row label={tl("Plano")}>{planeSpecSelect(k().plane, (p) => update((x) => x.type === "sketch" && (x.plane = p)), lost("plane").length > 0)}</Row>
                  {field(tl("Desplazamiento"), "kind.offset", k().offset, (x, v) => x.type === "sketch" && (x.offset = v), "mm")}
                  <Row label={tl("Horizontal")}>
                    <Select
                      options={[
                        { value: "plane", label: tl("La del plano") },
                        ...(k().sketch.x_axis && !["x", "y", "z", "reference"].includes(k().sketch.x_axis!.type)
                          ? [{ value: "picked", label: k().sketch.x_axis!.type === "edge" ? tl("Arista del sólido") : tl("Línea de otro sketch") }]
                          : []),
                        { value: "x", label: tl("Eje X") },
                        { value: "y", label: tl("Eje Y") },
                        { value: "z", label: tl("Eje Z") },
                        ...refOptions(props.store, "axis"),
                      ]}
                      value={(() => {
                        const a = k().sketch.x_axis;
                        if (!a) return "plane";
                        if (refValue(a)) return refValue(a)!;
                        return ["x", "y", "z"].includes(a.type) ? a.type : "picked";
                      })()}
                      onChange={(v) => {
                        if (v === "picked") return;
                        void setHorizontal(v === "plane" ? null : isRefValue(v) ? axisOfValue(v) : { type: v as "x" | "y" | "z" });
                      }}
                    />
                  </Row>
                  <Button
                    size="sm"
                    fullWidth
                    title={tl("El eje x del sketch sigue a esa arista (o línea de otro sketch) proyectada al plano; el dibujo queda en su lugar")}
                    variant={(props.ui.pick() as { owner?: string }).owner === `${f().id}:horizontal` ? "primary" : "default"}
                    onClick={() =>
                      props.ui.setPick({
                        kind: "axis",
                        owner: `${f().id}:horizontal`,
                        sketch: -1,
                        prompt: tl("Clic en la arista del sólido que da la dirección horizontal del sketch"),
                        done: (axis) => {
                          // Una línea del mismo sketch no puede orientarlo (dependería de sí mismo)
                          if (axis.type === "sketch_line" && axis.sketch === f().id) return;
                          void setHorizontal(axis);
                        },
                      })
                    }
                  >
                    {tl("Horizontal según una arista")}
                  </Button>
                  <Checkbox
                    small
                    label={tl("Invertir la normal")}
                    checked={!!k().sketch.flip_normal}
                    onChange={(c) =>
                      void props.store.commit((d) => {
                        const ft = d.features.find((x) => x.id === f().id);
                        if (ft?.kind.type !== "sketch" || !!ft.kind.sketch.flip_normal === c) return;
                        // El dibujo se refleja para quedar en el mismo lugar visto del otro lado (y las regiones elegidas con él)
                        flipSketchY(ft.kind.sketch);
                        ft.kind.sketch.flip_normal = c || undefined;
                        moveRegionPoints(d, ft.id, (p) => [p[0], -p[1] || 0]);
                      })
                    }
                  />
                  <p class="text-[11px] text-text-dim">
                    {k().sketch.entities.length} {tl("entidades ·")} {k().sketch.constraints.length} {tl("restricciones ·")} {view()?.regions.length ?? 0} {tl("regiones")}
                  </p>
                  <Button size="sm" variant="primary" fullWidth icon={<Icons.PaintBrush size={14} />} onClick={() => props.ui.editSketch(f().id)}>
                    {tl("Editar sketch")}
                  </Button>
                  {/* El sketch entero como JSON (puntos, entidades, restricciones, plano): para reportar un error y reproducirlo */}
                  <Button
                    size="sm"
                    fullWidth
                    icon={<Icons.Copy size={14} />}
                    data-copy-sketch
                    onClick={async () => {
                      // Si se está editando, lo de la sesión (lo último que se hizo)
                      const s = props.ui.session();
                      const kind = s?.feature === f().id ? { ...k(), sketch: s.sketch } : k();
                      const code = JSON.stringify({ pinocchio: "sketch", name: f().name, kind });
                      props.ui.setMessage((await copyText(code)) ? tl("Sketch «{name}» copiado como código ({length} caracteres)", { name: f().name, length: code.length }) : tl("No se pudo copiar al portapapeles"));
                    }}
                  >
                    {tl("Copiar código")}
                  </Button>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "extrude" && (f().kind as Extract<FeatureKind, { type: "extrude" }>)}>
            {(k) => {
              // "Hasta una cara" elegido y todavía sin cara
              const [upTo, setUpTo] = createSignal(false);
              const extentType = () => (upTo() ? "up_to_face" : k().extent.type);
              const setExtent = (e: Extent) => update((x) => x.type === "extrude" && (x.extent = e));
              return (
                <>
                  <Row label={tl("Sketch")}>
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "extrude" && (x.sketch = +v))} />
                  </Row>
                  <Row label={tl("Hasta")}>
                    <Select
                      options={[
                        { value: "blind", label: tl("Distancia") },
                        { value: "symmetric", label: tl("Simétrica") },
                        { value: "two_sides", label: tl("Dos direcciones") },
                        { value: "through_all", label: tl("Atravesar todo") },
                        { value: "up_to_next", label: tl("Hasta la siguiente") },
                        { value: "up_to_face", label: tl("Hasta una cara") },
                        { value: "up_to_vertex", label: tl("Hasta un punto") },
                        { value: "up_to_scan", label: tl("Hasta el escaneo") },
                      ]}
                      value={extentType()}
                      onChange={(v) => {
                        if (v === extentType()) return;
                        const d = "distance" in k().extent ? (k().extent as { distance: number }).distance : 10;
                        if (v === "blind" || v === "symmetric") setExtent({ type: v, distance: d });
                        else if (v === "two_sides") setExtent({ type: "two_sides", distance: d, second: d / 2 });
                        else if (v === "through_all" || v === "up_to_next") setExtent({ type: v });
                        else if (v === "up_to_scan") setExtent({ type: "up_to_scan", offset: 0 });
                        else if (v === "up_to_vertex") setExtent({ type: "up_to_vertex", point: { type: "at", point: [0, 0, 10] } });
                        else setUpTo(true);
                      }}
                    />
                  </Row>
                  <Show when={k().extent.type === "up_to_face" || upTo()}>
                    <SelectionBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:hasta`}
                      kind="face"
                      label={tl("Hasta la cara")}
                      lost={lost("extent")}
                      refs={k().extent.type === "up_to_face" ? [(k().extent as { face: FaceRef }).face] : []}
                      onChange={(refs) => {
                        if (refs[0]) setExtent({ type: "up_to_face", face: refs[0] as FaceRef, offset: (k().extent as { offset?: number }).offset });
                        setUpTo(false);
                      }}
                    />
                    <Show when={k().extent.type === "up_to_face"}>
                      {field(
                        tl("Antes de la cara"),
                        "kind.extent.offset",
                        (k().extent as { offset?: number }).offset ?? 0,
                        (x, v) => x.type === "extrude" && x.extent.type === "up_to_face" && (x.extent.offset = v || undefined),
                        "mm",
                      )}
                    </Show>
                  </Show>
                  <Show when={k().extent.type === "up_to_vertex" && (k().extent as Extract<Extent, { type: "up_to_vertex" }>)}>
                    {(e) => (
                      <>
                        <PointField
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:hasta-punto`}
                          label={tl("Hasta el punto")}
                          value={e().point}
                          except={f().id}
                          onChange={(p) => setExtent({ type: "up_to_vertex", point: p })}
                        />
                        <p class="text-[11px] text-text-dim leading-relaxed">
                          {tl("Llega hasta la altura del punto medida sobre la dirección de la extrusión; si el punto está detrás del plano, va hacia ese lado.")}
                        </p>
                      </>
                    )}
                  </Show>
                  <Show when={k().extent.type === "blind" || k().extent.type === "symmetric" || k().extent.type === "two_sides"}>
                    {field(
                      tl("Distancia"),
                      "kind.extent.distance",
                      (k().extent as { distance: number }).distance,
                      (x, v) => x.type === "extrude" && "distance" in x.extent && (x.extent.distance = v),
                      "mm",
                    )}
                  </Show>
                  <Show when={k().extent.type === "up_to_scan"}>
                    {field(
                      tl("Separación"),
                      "kind.extent.offset",
                      (k().extent as { offset: number }).offset,
                      (x, v) => x.type === "extrude" && x.extent.type === "up_to_scan" && (x.extent.offset = v),
                      "mm",
                    )}
                    <p class="text-[11px] text-text-dim leading-relaxed">
                      {tl("Llega hasta la piel del escaneo, aunque sea curva, y queda a esa distancia. Va hacia el lado donde está el escaneo.")}
                    </p>
                  </Show>
                  <Show when={k().extent.type === "two_sides"}>
                    {field(
                      tl("Hacia atrás"),
                      "kind.extent.second",
                      (k().extent as { second: number }).second,
                      (x, v) => x.type === "extrude" && x.extent.type === "two_sides" && (x.extent.second = v),
                      "mm",
                    )}
                  </Show>
                  {field(tl("Desde el plano a"), "kind.start", k().start ?? 0, (x, v) => x.type === "extrude" && (x.start = v || undefined), "mm")}
                  <Checkbox
                    small
                    label={tl("Dirección propia")}
                    checked={!!k().direction}
                    onChange={(c) => update((x) => x.type === "extrude" && (x.direction = c ? { type: "z" } : null))}
                  />
                  <Show when={k().direction}>
                    {(d) => (
                      <AxisField
                        store={props.store}
                        ui={props.ui}
                        owner={`${f().id}:direccion`}
                        label={tl("A lo largo de")}
                        value={d()}
                        except={f().id}
                        onChange={(a) => update((x) => x.type === "extrude" && (x.direction = a))}
                      />
                    )}
                  </Show>
                  {field(tl("Desmolde"), "kind.draft", k().draft ?? 0, (x, v) => x.type === "extrude" && (x.draft = v), "°")}
                  <Checkbox
                    small
                    label={tl("Delgada (solo una pared)")}
                    checked={k().thin != null}
                    onChange={(c) => update((x) => x.type === "extrude" && (x.thin = c ? 1 : null))}
                  />
                  <Show when={k().thin != null}>
                    {field(tl("Espesor"), "kind.thin", k().thin ?? 1, (x, v) => x.type === "extrude" && (x.thin = v), "mm")}
                  </Show>
                  <RegionBox
                    ui={props.ui}
                    store={props.store}
                    owner={`${f().id}:regiones`}
                    lost={lost("regions")}
                    sketch={k().sketch}
                    value={k().regions}
                    onChange={(r) => update((x) => x.type === "extrude" && (x.regions = r))}
                  />
                  <Checkbox small label={tl("Hacia el otro lado")} checked={k().reverse} onChange={(c) => update((x) => x.type === "extrude" && (x.reverse = c))} />
                  <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "extrude" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "revolve" && (f().kind as Extract<FeatureKind, { type: "revolve" }>)}>
            {(k) => {
              // «Hasta una cara» elegido y todavía sin cara
              const [upToFace, setUpToFace] = createSignal(false);
              const sketch = () => props.sketches.find((s) => s.id === k().sketch);
              const lines = () => {
                const s = sketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "line") : [];
              };
              const axisValue = () => {
                const a = k().axis;
                return a.type === "sketch_line" ? `line:${a.line}` : (refValue(a) ?? a.type);
              };
              return (
                <>
                  <Row label={tl("Sketch")}>
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "revolve" && (x.sketch = +v))} />
                  </Row>
                  <Row label={tl("Eje")}>
                    <Select
                      options={[
                        ...(k().axis.type === "edge" ? [{ value: "edge", label: tl("Arista del sólido") }] : []),
                        { value: "x", label: "X" },
                        { value: "y", label: "Y" },
                        { value: "z", label: "Z" },
                        ...lines().map((l) => ({ value: `line:${l.id}`, label: l.construction ? tl("Línea {id} (construcción)", { id: l.id }) : tl("Línea {id}", { id: l.id }) })),
                        ...refOptions(props.store, "axis"),
                      ]}
                      value={axisValue()}
                      onChange={(v) =>
                        update((x) => {
                          if (x.type !== "revolve") return;
                          x.axis = v.startsWith("line:")
                            ? { type: "sketch_line", sketch: x.sketch, line: +v.slice(5) }
                            : isRefValue(v)
                              ? axisOfValue(v)
                              : { type: v as "x" | "y" | "z" };
                        })
                      }
                    />
                  </Row>
                  <Button
                    size="sm"
                    fullWidth
                    variant={(props.ui.pick() as { owner?: string }).owner === `${f().id}:eje` ? "primary" : "default"}
                    onClick={() =>
                      props.ui.setPick({
                        kind: "axis",
                        owner: `${f().id}:eje`,
                        sketch: k().sketch,
                        prompt: tl("Clic en la línea del sketch (o la arista del sólido) que hace de eje"),
                        done: (axis) => update((x) => x.type === "revolve" && (x.axis = axis)),
                      })
                    }
                  >
                    {tl("Elegir el eje en el visor")}
                  </Button>
                  <RegionBox
                    ui={props.ui}
                    store={props.store}
                    owner={`${f().id}:regiones`}
                    lost={lost("regions")}
                    sketch={k().sketch}
                    value={k().regions}
                    onChange={(r) => update((x) => x.type === "revolve" && (x.regions = r))}
                  />
                  <Row label={tl("Hacia")}>
                    <Select
                      options={[
                        { value: "one", label: tl("Un lado") },
                        { value: "symmetric", label: tl("Simétrica") },
                        { value: "two_angles", label: tl("Dos ángulos") },
                        { value: "up_to_face", label: tl("Hasta una cara") },
                      ]}
                      value={upToFace() ? "up_to_face" : (k().extent?.type ?? "one")}
                      onChange={(v) => {
                        setUpToFace(v === "up_to_face" && k().extent?.type !== "up_to_face");
                        if (v === "up_to_face") return;
                        update(
                          (x) =>
                            x.type === "revolve" &&
                            (x.extent = v === "two_angles" ? { type: "two_angles", second: 45 } : v === "symmetric" ? { type: "symmetric" } : undefined),
                        );
                      }}
                    />
                  </Row>
                  <Show when={upToFace() || k().extent?.type === "up_to_face"}>
                    <SelectionBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:hasta`}
                      kind="face"
                      label={tl("Hasta la cara")}
                      lost={lost("extent")}
                      refs={k().extent?.type === "up_to_face" ? [(k().extent as { face: FaceRef }).face] : []}
                      onChange={(refs) => {
                        if (refs[0]) update((x) => x.type === "revolve" && (x.extent = { type: "up_to_face", face: refs[0] as FaceRef }));
                        setUpToFace(false);
                      }}
                    />
                    <p class="text-[11px] text-text-dim">{tl("Gira hacia el lado del signo del ángulo hasta encontrar la cara (plana o curva).")}</p>
                  </Show>
                  {field(tl("Ángulo"), "kind.angle", k().angle, (x, v) => x.type === "revolve" && (x.angle = v), "°")}
                  <Checkbox
                    small
                    label={tl("Delgada (solo una pared)")}
                    checked={k().thin != null}
                    onChange={(c) => update((x) => x.type === "revolve" && (x.thin = c ? 1 : null))}
                  />
                  <Show when={k().thin != null}>
                    {field(tl("Espesor"), "kind.thin", k().thin ?? 1, (x, v) => x.type === "revolve" && (x.thin = v), "mm")}
                  </Show>
                  <Show when={k().extent?.type === "two_angles"}>
                    {field(
                      tl("Hacia atrás"),
                      "kind.extent.second",
                      (k().extent as { second: number }).second,
                      (x, v) => x.type === "revolve" && x.extent?.type === "two_angles" && (x.extent.second = v),
                      "°",
                    )}
                  </Show>
                  <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "revolve" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "primitive" && (f().kind as Extract<FeatureKind, { type: "primitive" }>)}>
            {(k) => {
              const fields: Record<string, [string, string][]> = {
                box: [["dx", "Ancho (X)"], ["dy", tl("Fondo (Y)")], ["dz", "Alto (Z)"]],
                cylinder: [["radius", tl("Radio")], ["height", tl("Alto")]],
                cone: [["r1", tl("Radio abajo")], ["r2", tl("Radio arriba")], ["height", tl("Alto")]],
                sphere: [["radius", tl("Radio")]],
                torus: [["major", tl("Radio mayor")], ["minor", tl("Radio menor")]],
                bolt: [["length", tl("Largo")]],
                nut: [],
                washer: [],
                gear: [],
              };
              type Standard = Extract<PrimitiveShape, { type: "bolt" | "nut" | "washer" }>;
              const standard = () => (["bolt", "nut", "washer"].includes(k().shape.type) ? (k().shape as Standard) : undefined);
              const threaded = () => k().shape.type === "bolt" || k().shape.type === "nut";
              const linked = () => threaded() && !!k().link;
              /** Asociar a otra rosca: la medida y un largo (o una posición) que calzan; soltar deja la pieza donde está */
              const setLink = (link: ThreadLink | null, src: ThreadAxis | null) => {
                const placed = props.store.result()?.threads?.find((t) => t.feature === f().id)?.placed;
                update((x) => {
                  if (x.type !== "primitive" || (x.shape.type !== "bolt" && x.shape.type !== "nut")) return;
                  if (!link || !src) {
                    if (placed) [x.origin, x.z, x.x] = placed;
                    x.link = null;
                    return;
                  }
                  const m = METRIC_HOLES.find((h) => Math.abs(h.nominal - src.spec.nominal) < 1e-6);
                  if (m) x.shape.size = m.size;
                  const size = x.shape.size;
                  let offset = 0;
                  if (x.shape.type === "bolt") {
                    // Ciego: el más largo que entra; pasante o tuerca: el que llega al otro lado
                    x.shape.length = src.blind
                      ? ([...BOLT_LENGTHS].reverse().find((l) => l <= src.length + 1e-6) ?? BOLT_LENGTHS[0])
                      : boltLengthFor(size, src.length, false);
                  } else if (src.placed) {
                    // Tuerca en un tornillo: cerca de la punta
                    offset = Math.max(0, +(src.length - (NUT_HEIGHT[size] ?? src.spec.nominal) - 2 * src.spec.pitch).toFixed(2));
                  }
                  x.shape.modeled = src.modeled || undefined;
                  // Por la boca más cercana a donde estaba la pieza (p. ej. el borde elegido al crearla)
                  let flip: boolean | undefined;
                  if (placed) {
                    const d = (a: P3, b: P3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
                    const far: P3 = [0, 1, 2].map((i) => src.mouth[i] - src.out[i] * src.length) as P3;
                    // (solo si ya estaba apoyada en la otra boca, mirando hacia afuera de ella)
                    const back = placed[1][0] * src.out[0] + placed[1][1] * src.out[1] + placed[1][2] * src.out[2] < -0.9;
                    flip = (back && d(placed[0], far) < Math.min(d(placed[0], src.mouth), src.spec.nominal)) || undefined;
                  }
                  x.link = { ...link, offset, flip };
                });
              };
              return (
                <>
                  <Show when={k().shape.type === "gear" && (k().shape as Extract<PrimitiveShape, { type: "gear" }>)}>
                    {(g) => <GearFields store={props.store} ui={props.ui} feature={f().id} shape={g()} field={field} update={update} />}
                  </Show>
                  {/* Pieza estándar: medida, cabeza y rosca */}
                  <Show when={threaded()}>
                    <ThreadLinkBox
                      store={props.store}
                      ui={props.ui}
                      feature={f().id}
                      value={k().link}
                      want={k().shape.type === "bolt" ? "internal" : "external"}
                      what={k().shape.type === "bolt" ? tl("el tornillo") : tl("la tuerca")}
                      askAlways
                      onChange={setLink}
                    />
                  </Show>
                  <Show when={standard()}>
                    {(st) => (
                      <>
                        <Show when={!linked()}>
                          <Row label={tl("Medida")}>
                            <Select
                              options={METRIC_HOLES.map((m) => ({ value: m.size, label: m.size }))}
                              value={st().size}
                              onChange={(v) => update((x) => x.type === "primitive" && "size" in x.shape && (x.shape.size = v))}
                            />
                          </Row>
                        </Show>
                        <Show when={st().type === "bolt" && (st() as Extract<PrimitiveShape, { type: "bolt" }>)}>
                          {(b) => (
                            <Row label={tl("Cabeza")}>
                              <Select
                                options={BOLT_HEADS}
                                value={b().head ?? "socket"}
                                onChange={(v) => update((x) => x.type === "primitive" && x.shape.type === "bolt" && (x.shape.head = v as BoltHead))}
                              />
                            </Row>
                          )}
                        </Show>
                      </>
                    )}
                  </Show>
                  <For each={fields[k().shape.type]}>
                    {([key, label]) => (
                      field(label, `kind.shape.${key}`, (k().shape as unknown as Record<string, number>)[key], (x, v) => {
                        if (x.type === "primitive") (x.shape as unknown as Record<string, number>)[key] = v;
                      }, "mm")
                    )}
                  </For>
                  <Show when={k().shape.type === "bolt" || k().shape.type === "nut"}>
                    <Tooltip content={tl("El filete de verdad (para imprimir); sin marcar, un cilindro liso, más rápido")}>
                      <Checkbox
                        small
                        label={tl("Rosca modelada")}
                        checked={!!(standard() as { modeled?: boolean } | undefined)?.modeled}
                        onChange={(c) =>
                          update((x) => {
                            if (x.type === "primitive" && (x.shape.type === "bolt" || x.shape.type === "nut")) x.shape.modeled = c || undefined;
                          })
                        }
                      />
                    </Tooltip>
                  </Show>
                  <Show when={k().shape.type === "box" && (k().shape as Extract<PrimitiveShape, { type: "box" }>)}>
                    {(bx) => (
                      <Row label={tl("Origen en")}>
                        <Select
                          options={[
                            { value: "center", label: tl("El centro") },
                            { value: "base", label: tl("El centro de la base") },
                            { value: "corner", label: tl("Una esquina") },
                          ]}
                          value={bx().centered_z ? "center" : bx().centered ? "base" : "corner"}
                          onChange={(v) =>
                            update((x) => {
                              if (x.type !== "primitive" || x.shape.type !== "box") return;
                              x.shape.centered = v !== "corner";
                              x.shape.centered_z = v === "center";
                            })
                          }
                        />
                      </Row>
                    )}
                  </Show>
                  <Show when={linked() && k().link}>
                    {(l) => (
                      <>
                        {field(
                          k().shape.type === "bolt" ? tl("Hundido") : tl("Hasta"),
                          "kind.link.offset",
                          l().offset ?? 0,
                          (x, v) => x.type === "primitive" && x.link && (x.link.offset = v),
                          "mm",
                        )}
                        <Checkbox small label={tl("Por la otra boca")} checked={!!l().flip} onChange={(c) => update((x) => x.type === "primitive" && !!x.link && (x.link.flip = c || undefined))} />
                        <p class="text-[11px] text-text-dim">
                          {k().shape.type === "bolt"
                            ? tl("Va en el eje de la rosca, con la cabeza en la boca; «Hundido» lo mete más (negativo: queda afuera).")
                            : tl("Va en el eje de la rosca; «Hasta» es cuánto avanza desde la boca.")}
                        </p>
                      </>
                    )}
                  </Show>
                  <Show when={!linked()}>
                  <div class="space-y-1">
                    <span class="text-xs text-text-muted">{tl("Posición (mm)")}</span>
                    <div class="grid grid-cols-3 gap-1">
                      <For each={["X", "Y", "Z"]}>
                        {(axis, i) => field(axis, `kind.origin.${i()}`, k().origin[i()], (x, v) => x.type === "primitive" && (x.origin[i()] = v))}
                      </For>
                    </div>
                  </div>
                  <Row label={tl("Eje")}>
                    <Select
                      options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                      value={Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - k().z[i]) < 1e-9))?.[0] ?? "z"}
                      onChange={(v) =>
                        update((x) => {
                          if (x.type !== "primitive") return;
                          x.z = AXIS_DIRS[v];
                          x.x = v === "x" ? [0, 1, 0] : [1, 0, 0];
                        })
                      }
                    />
                  </Row>
                  </Show>
                  <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "primitive" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "fillet" || f().kind.type === "chamfer") && f().kind}>
            {(k) => {
              // El tipo no cambia; los valores se leen de `k()` (el campo se rehace al cambiar)
              const kind = k() as Extract<FeatureKind, { type: "fillet" | "chamfer" }>;
              const value = () => {
                const x = k() as typeof kind;
                return x.type === "fillet" ? x.radius : x.distance;
              };
              return (
                <>
                  <Show when={kind.type === "chamfer"}>
                    <Row label={tl("Tipo")}>
                      <Select
                        options={[
                          { value: "equal", label: tl("Igual a los dos lados") },
                          { value: "distance", label: tl("Dos distancias") },
                          { value: "angle", label: tl("Distancia y ángulo") },
                        ]}
                        value={(k() as { second?: ChamferSecond | null }).second?.type ?? "equal"}
                        onChange={(v) =>
                          update((x) => {
                            if (x.type !== "chamfer" || v === (x.second?.type ?? "equal")) return;
                            const flip = x.second?.flip ?? false;
                            x.second = v === "distance" ? { type: "distance", distance: x.distance * 2, flip } : v === "angle" ? { type: "angle", degrees: 45, flip } : null;
                          })
                        }
                      />
                    </Row>
                  </Show>
                  {field(kind.type === "fillet" ? ((k() as { radius2?: number | null }).radius2 != null ? tl("Radio al comienzo") : tl("Radio")) : tl("Distancia"), kind.type === "fillet" ? "kind.radius" : "kind.distance", value(), (x, v) => {
                    if (x.type === "fillet") x.radius = v;
                    else if (x.type === "chamfer") x.distance = v;
                  }, "mm")}
                  <Show when={kind.type === "fillet"}>
                    <Checkbox
                      small
                      label={tl("Radio variable")}
                      checked={(k() as { radius2?: number | null }).radius2 != null}
                      onChange={(c) => update((x) => x.type === "fillet" && (x.radius2 = c ? x.radius * 2 : null))}
                    />
                    <Show when={(k() as { radius2?: number | null }).radius2 != null}>
                      {field(tl("Radio al final"), "kind.radius2", (k() as { radius2: number }).radius2, (x, v) => x.type === "fillet" && (x.radius2 = v), "mm")}
                    </Show>
                    <div class="space-y-1" aria-label={tl("Radios en el medio")}>
                      <For each={(k() as { points?: { at: number; radius: number }[] }).points ?? []}>
                        {(pt, i) => (
                          <div class="flex items-center gap-1" data-fillet-point={i()}>
                            <Num label="En %" value={pt.at * 100} onCommit={(v) => update((x) => x.type === "fillet" && !!x.points && (x.points[i()].at = v / 100))} />
                            <Num label="R" value={pt.radius} onCommit={(v) => update((x) => x.type === "fillet" && !!x.points && (x.points[i()].radius = v))} />
                            <IconButton
                              aria-label={tl("Quitar el radio {v1}", { v1: i() + 1 })}
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                update((x) => {
                                  if (x.type !== "fillet" || !x.points) return;
                                  x.points.splice(i(), 1);
                                  if (!x.points.length) x.points = undefined;
                                })
                              }
                            >
                              <Icons.Trash size={11} />
                            </IconButton>
                          </div>
                        )}
                      </For>
                      <Button
                        size="sm"
                        fullWidth
                        onClick={() =>
                          update((x) => {
                            if (x.type !== "fillet") return;
                            // En el medio del tramo más largo que quede libre
                            const ats = [0, ...(x.points ?? []).map((p) => p.at), 1].sort((a, b) => a - b);
                            let at = 0.5;
                            let gap = 0;
                            for (let j = 1; j < ats.length; j++)
                              if (ats[j] - ats[j - 1] > gap) [gap, at] = [ats[j] - ats[j - 1], (ats[j] + ats[j - 1]) / 2];
                            x.points = [...(x.points ?? []), { at, radius: x.radius * 2 }].sort((a, b) => a.at - b.at);
                          })
                        }
                      >
                        {tl("Agregar radio en el medio")}
                      </Button>
                    </div>
                  </Show>
                  <Show when={kind.type === "chamfer" && (k() as { second?: ChamferSecond | null }).second}>
                    {(sec) => (
                      <>
                        <Show
                          when={sec().type === "angle"}
                          fallback={field(tl("Otra distancia"), "kind.second.distance", (sec() as { distance: number }).distance, (x, v) => x.type === "chamfer" && x.second?.type === "distance" && (x.second.distance = v), "mm")}
                        >
                          {field(tl("Ángulo"), "kind.second.degrees", (sec() as { degrees: number }).degrees, (x, v) => x.type === "chamfer" && x.second?.type === "angle" && (x.second.degrees = v), "°")}
                        </Show>
                        <Checkbox small label={tl("Medir desde la otra cara")} checked={sec().flip} onChange={(c) => update((x) => x.type === "chamfer" && !!x.second && (x.second.flip = c))} />
                      </>
                    )}
                  </Show>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:aristas`}
                    kind="edges"
                    label={tl("Aristas")}
                    live={f().id}
                    lost={lost("edges")}
                    refs={(k() as { edges: EdgeRef[] }).edges}
                    onChange={(refs) =>
                      update((x) => {
                        if (x.type === "fillet" || x.type === "chamfer") x.edges = refs as EdgeRef[];
                      })
                    }
                  />
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "shell" || f().kind.type === "draft") && f().kind}>
            {(k) => {
              const kind = k() as Extract<FeatureKind, { type: "shell" | "draft" }>;
              return (
                <>
                  <Show
                    when={kind.type === "draft" && (k() as Extract<FeatureKind, { type: "draft" }>)}
                    fallback={
                      field(tl("Grosor de pared"), "kind.thickness", (k() as { thickness: number }).thickness, (x, v) => x.type === "shell" && (x.thickness = v), "mm")
                    }
                  >
                    {(d) => (
                      <>
                        {field(tl("Ángulo"), "kind.angle", d().angle, (x, v) => x.type === "draft" && (x.angle = v), "°")}
                        <Row label={tl("Plano neutro")}>{planeSpecSelect(d().neutral, (p) => update((x) => x.type === "draft" && (x.neutral = p)), lost("neutral").length > 0)}</Row>
                      </>
                    )}
                  </Show>
                  <Show when={kind.type === "shell"}>
                    <SelectionBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:otras`}
                      kind="faces"
                      optional
                      label={tl("Caras con otro espesor")}
                      refs={((k() as { overrides?: { face: FaceRef }[] }).overrides ?? []).map((o) => o.face)}
                      onChange={(refs) =>
                        update((x) => {
                          if (x.type !== "shell") return;
                          const old = x.overrides ?? [];
                          const same = (a: FaceRef, b: FaceRef) => JSON.stringify(a) === JSON.stringify(b);
                          const next = (refs as FaceRef[]).map((face) => old.find((o) => same(o.face, face)) ?? { face, thickness: x.thickness * 2 });
                          x.overrides = next.length ? next : undefined;
                        })
                      }
                    />
                    <For each={(k() as { overrides?: { face: FaceRef; thickness: number }[] }).overrides ?? []}>
                      {(o, i) =>
                        field(tl("Espesor cara {v1}", { v1: i() + 1 }), `kind.overrides.${i()}.thickness`, o.thickness, (x, v) => x.type === "shell" && !!x.overrides && (x.overrides[i()].thickness = v), "mm")
                      }
                    </For>
                  </Show>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:caras`}
                    kind="faces"
                    label={kind.type === "shell" ? tl("Caras abiertas") : tl("Caras")}
                    lost={lost("faces")}
                    refs={(k() as { faces: FaceRef[] }).faces}
                    onChange={(refs) =>
                      update((x) => {
                        if (x.type === "shell" || x.type === "draft") x.faces = refs as FaceRef[];
                      })
                    }
                  />
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "pattern" && (f().kind as Extract<FeatureKind, { type: "pattern" }>)}>
            {(k) => (
              <>
                <ToolChecklist tools={props.tools.filter((t) => t.id !== f().id)} value={k().features} onChange={(ids) => update((x) => x.type === "pattern" && (x.features = ids))} />
                <Show when={k().pattern.type === "table" && (k().pattern as Extract<PatternKind, { type: "table" }>)}>
                  {(t) => (
                    <div class="space-y-1" aria-label={tl("Tabla de copias")}>
                      <span class="text-xs text-text-muted">{tl("Desplazamiento de cada copia (mm)")}</span>
                      <For each={t().offsets}>
                        {(row, i) => (
                          <div class="flex items-center gap-1" data-table-row={i()}>
                            <For each={[0, 1, 2]}>
                              {(c) => (
                                <NumberInput
                                  value={row[c]}
                                  step={1}
                                  label={["X", "Y", "Z"][c]}
                                  onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "table" && (x.pattern.offsets[i()][c] = v))}
                                />
                              )}
                            </For>
                            <IconButton
                              aria-label={tl("Quitar la fila {v1}", { v1: i() + 1 })}
                              size="sm"
                              variant="ghost"
                              onClick={() => update((x) => x.type === "pattern" && x.pattern.type === "table" && x.pattern.offsets.splice(i(), 1))}
                            >
                              <Icons.Trash size={11} />
                            </IconButton>
                          </div>
                        )}
                      </For>
                      <Button
                        size="sm"
                        fullWidth
                        onClick={() =>
                          update((x) => {
                            if (x.type !== "pattern" || x.pattern.type !== "table") return;
                            const last = x.pattern.offsets[x.pattern.offsets.length - 1] ?? [0, 0, 0];
                            x.pattern.offsets.push([last[0] + 10, last[1], last[2]]);
                          })
                        }
                      >
                        {tl("Agregar fila")}
                      </Button>
                    </div>
                  )}
                </Show>
                <Show when={k().pattern.type === "fill" && (k().pattern as Extract<PatternKind, { type: "fill" }>)}>
                  {(fl) => (
                    <>
                      <Row label={tl("Región")}>
                        <Select
                          options={sketchOptions()}
                          value={String(fl().sketch)}
                          onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && ((x.pattern.sketch = +v), (x.pattern.regions = { type: "all" })))}
                        />
                      </Row>
                      <RegionBox
                        ui={props.ui}
                        store={props.store}
                        owner={`${f().id}:relleno`}
                        lost={lost("regions")}
                        sketch={fl().sketch}
                        value={fl().regions}
                        onChange={(r) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.regions = r))}
                      />
                      {field(tl("Separación"), "kind.pattern.spacing", fl().spacing, (x, v) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.spacing = v), "mm")}
                      {field(tl("Margen al borde"), "kind.pattern.margin", fl().margin, (x, v) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.margin = v), "mm")}
                      <Checkbox small label={tl("Grilla hexagonal")} checked={fl().hex} onChange={(c) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.hex = c))} />
                      <p class="text-[11px] text-text-dim">
                        {tl("Copias a la separación elegida dentro de la región, con el centro a no menos del margen del borde. La grilla pasa por la operación original.")}
                      </p>
                    </>
                  )}
                </Show>
                <Show when={k().pattern.type === "points" && (k().pattern as Extract<PatternKind, { type: "points" }>)}>
                  {(pt) => {
                    const loose = () => {
                      const s = props.sketches.find((x) => x.id === pt().sketch);
                      return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "point" && !e.construction).length : 0;
                    };
                    return (
                      <>
                        <Row label={tl("Puntos de")}>
                          <Select
                            options={sketchOptions()}
                            value={String(pt().sketch)}
                            onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "points" && ((x.pattern.sketch = +v), (x.pattern.points = [])))}
                          />
                        </Row>
                        <p class="text-[11px] text-text-dim">
                          {pt().points.length ? `${pt().points.length} puntos elegidos` : `${loose()} puntos sueltos`}{tl(": una copia en cada uno, desde el más cercano a lo que se repite (ahí queda el original).")}
                        </p>
                      </>
                    );
                  }}
                </Show>
                <Show when={k().pattern.type === "surface" && (k().pattern as Extract<PatternKind, { type: "surface" }>)}>
                  {(sf) => (
                    <>
                      {field(tl("Copias en u"), "kind.pattern.count_u", sf().count_u, (x, v) => x.type === "pattern" && x.pattern.type === "surface" && (x.pattern.count_u = Math.max(1, Math.round(v))))}
                      {field(tl("Copias en v"), "kind.pattern.count_v", sf().count_v, (x, v) => x.type === "pattern" && x.pattern.type === "surface" && (x.pattern.count_v = Math.max(1, Math.round(v))))}
                      <p class="text-[11px] text-text-dim">{tl("Las copias se reparten por toda la cara curva a partir de la original y quedan apoyadas en su normal (u: alrededor o a lo ancho, v: a lo largo).")}</p>
                    </>
                  )}
                </Show>
                <Show when={k().pattern.type !== "table" && k().pattern.type !== "fill" && k().pattern.type !== "points" && k().pattern.type !== "surface"}>
                <Show
                  when={k().pattern.type !== "curve" && k().pattern.type !== "chain"}
                  fallback={
                    <>
                      <Row label={tl("A lo largo de")}>
                        <Select
                          options={[...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                          value={(() => {
                            const p = (k().pattern as Extract<PatternKind, { type: "curve" | "chain" }>).path;
                            return p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                          })()}
                          onChange={(v) =>
                            update(
                              (x) =>
                                x.type === "pattern" &&
                                (x.pattern.type === "curve" || x.pattern.type === "chain") &&
                                (x.pattern.path = v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                            )
                          }
                        />
                      </Row>
                      <Show when={k().pattern.type === "chain"}>
                        {field(tl("Separación"), "kind.pattern.spacing", (k().pattern as { spacing: number }).spacing, (x, v) => x.type === "pattern" && x.pattern.type === "chain" && (x.pattern.spacing = v), "mm")}
                        {field(tl("Cantidad (0 = todo el camino)"), "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && x.pattern.type === "chain" && (x.pattern.count = Math.max(0, Math.round(v))))}
                        <p class="text-[11px] text-text-dim">{tl("Cada eslabón queda a esa distancia medida sobre el camino y gira con él. Lo que se copia va en el comienzo del camino.")}</p>
                      </Show>
                      <Show when={k().pattern.type === "curve"}>
                      {field(tl("Cantidad"), "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
                      <p class="text-[11px] text-text-dim">{tl("Las copias se reparten de punta a punta del camino, sin girar.")}</p>
                      </Show>
                    </>
                  }
                >
                <Show
                  when={k().pattern.type === "linear" && (k().pattern as Extract<PatternKind, { type: "linear" }>)}
                  fallback={
                    <>
                      <Row label={tl("Eje")}>
                        <Select
                          options={[...["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() })), ...refOptions(props.store, "axis")]}
                          value={(() => {
                            const a = (k().pattern as { axis: AxisSpec }).axis;
                            return (refValue(a) ?? a.type);
                          })()}
                          onChange={(v) =>
                            update(
                              (x) =>
                                x.type === "pattern" &&
                                (x.pattern.type === "circular" || x.pattern.type === "spiral") &&
                                (x.pattern.axis = isRefValue(v) ? axisOfValue(v) : { type: v as "x" | "y" | "z" }),
                            )
                          }
                        />
                      </Row>
                      {field(tl("Cantidad"), "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
                      <Show when={k().pattern.type === "spiral"}>
                        {field(tl("Giro por copia"), "kind.pattern.angle", (k().pattern as { angle: number }).angle, (x, v) => x.type === "pattern" && x.pattern.type === "spiral" && (x.pattern.angle = v), "°")}
                        {field(tl("Alejamiento por copia"), "kind.pattern.step", (k().pattern as { step: number }).step, (x, v) => x.type === "pattern" && x.pattern.type === "spiral" && (x.pattern.step = v), "mm")}
                        <p class="text-[11px] text-text-dim">{tl("Cada copia gira y se aleja del eje (o se acerca, si es negativo) respecto de la anterior.")}</p>
                      </Show>
                      <Show when={k().pattern.type === "circular"}>
                      {field(tl("Ángulo total"), "kind.pattern.angle", (k().pattern as { angle: number }).angle, (x, v) => x.type === "pattern" && x.pattern.type === "circular" && (x.pattern.angle = v), "°")}
                      <Checkbox
                        small
                        label={tl("También en anillos más grandes")}
                        checked={!!(k().pattern as Extract<PatternKind, { type: "circular" }>).rings}
                        onChange={(c) =>
                          update((x) => {
                            if (x.type !== "pattern" || x.pattern.type !== "circular") return;
                            x.pattern.rings = c ? { count: 2, spacing: 10 } : null;
                          })
                        }
                      />
                      <Show when={(k().pattern as Extract<PatternKind, { type: "circular" }>).rings}>
                        {(r) => (
                          <>
                            {field(
                              tl("Anillos"),
                              "kind.pattern.rings.count",
                              r().count,
                              (x, v) => x.type === "pattern" && x.pattern.type === "circular" && x.pattern.rings && (x.pattern.rings.count = Math.max(1, Math.round(v))),
                            )}
                            {field(
                              tl("Separación entre anillos"),
                              "kind.pattern.rings.spacing",
                              r().spacing,
                              (x, v) => x.type === "pattern" && x.pattern.type === "circular" && x.pattern.rings && (x.pattern.rings.spacing = v),
                              "mm",
                            )}
                          </>
                        )}
                      </Show>
                      </Show>
                    </>
                  }
                >
                  {(lin) => (
                    <>
                      <Row label={tl("Dirección")}>
                        <Select
                          options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                          value={Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - lin().direction[i]) < 1e-9))?.[0] ?? "x"}
                          onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "linear" && (x.pattern.direction = AXIS_DIRS[v]))}
                        />
                      </Row>
                      {field(tl("Cantidad"), "kind.pattern.count", lin().count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
                      {field(tl("Separación"), "kind.pattern.spacing", lin().spacing, (x, v) => x.type === "pattern" && x.pattern.type === "linear" && (x.pattern.spacing = v), "mm")}
                      <Checkbox
                        small
                        label={tl("También en otra dirección")}
                        checked={!!lin().second}
                        onChange={(c) =>
                          update((x) => {
                            if (x.type !== "pattern" || x.pattern.type !== "linear") return;
                            const other: P3 = Math.abs(x.pattern.direction[0]) > 0.5 ? [0, 1, 0] : [1, 0, 0];
                            x.pattern.second = c ? { direction: other, count: 2, spacing: x.pattern.spacing } : null;
                          })
                        }
                      />
                      <Show when={lin().second}>
                        {(s2) => (
                          <>
                            <Row label={tl("Dirección 2")}>
                              <Select
                                options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                                value={Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - s2().direction[i]) < 1e-9))?.[0] ?? "y"}
                                onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "linear" && x.pattern.second && (x.pattern.second.direction = AXIS_DIRS[v]))}
                              />
                            </Row>
                            {field(
                              tl("Cantidad 2"),
                              "kind.pattern.second.count",
                              s2().count,
                              (x, v) => x.type === "pattern" && x.pattern.type === "linear" && x.pattern.second && (x.pattern.second.count = Math.max(1, Math.round(v))),
                            )}
                            {field(
                              tl("Separación 2"),
                              "kind.pattern.second.spacing",
                              s2().spacing,
                              (x, v) => x.type === "pattern" && x.pattern.type === "linear" && x.pattern.second && (x.pattern.second.spacing = v),
                              "mm",
                            )}
                          </>
                        )}
                      </Show>
                    </>
                  )}
                </Show>
                </Show>
                </Show>
                <Checkbox
                  small
                  label={tl("Cada copia cambia de tamaño")}
                  checked={!!k().vary}
                  onChange={(c) => update((x) => x.type === "pattern" && (x.vary = c ? { scale: 20, axis: null } : null))}
                />
                <Show when={k().vary}>
                  {(v) => (
                    <>
                      {field(tl("Cambio por copia"), "kind.pattern.vary.scale", v().scale, (x, n) => x.type === "pattern" && x.vary && (x.vary.scale = n), "%")}
                      <Row label={tl("Cambia")}>
                        <Select
                          options={[
                            { value: "all", label: tl("Todo el tamaño") },
                            ...["x", "y", "z"].map((a) => ({ value: a, label: tl("Solo a lo largo de {a}", { a: a.toUpperCase() }) })),
                          ]}
                          value={v().axis ? (Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - v().axis![i]) < 1e-9))?.[0] ?? "all") : "all"}
                          onChange={(a) => update((x) => x.type === "pattern" && x.vary && (x.vary.axis = a === "all" ? null : AXIS_DIRS[a]))}
                        />
                      </Row>
                    </>
                  )}
                </Show>
                <SkipCopies value={k().skip ?? []} onChange={(v) => update((x) => x.type === "pattern" && (x.skip = v.length ? v : undefined))} />
              </>
            )}
          </Match>
          <Match when={f().kind.type === "mirror" && (f().kind as Extract<FeatureKind, { type: "mirror" }>)}>
            {(k) => (
              <>
                <ToolChecklist tools={props.tools.filter((t) => t.id !== f().id)} value={k().features} onChange={(ids) => update((x) => x.type === "mirror" && (x.features = ids))} />
                <Row label={tl("Plano")}>{planeSpecSelect(k().plane, (p) => update((x) => x.type === "mirror" && (x.plane = p)))}</Row>
                <Checkbox
                  small
                  label={tl("El reflejo como pieza aparte")}
                  checked={k().op === "new"}
                  onChange={(c) => update((x) => x.type === "mirror" && (x.op = c ? "new" : undefined))}
                />
              </>
            )}
          </Match>
          <Match when={f().kind.type === "curve" && (f().kind as Extract<FeatureKind, { type: "curve" }>)}>
            {(k) => {
              const def = () => k().def;
              const setDef = (d: CurveDef) => update((x) => x.type === "curve" && (x.def = d));
              return (
                <>
                  <Switch>
                    <Match when={def().type === "composite" && (def() as Extract<CurveDef, { type: "composite" }>)}>
                      {(d) => (
                        <>
                          <SelectionBox
                            store={props.store}
                            ui={props.ui}
                            owner={`${f().id}:aristas`}
                            kind="edges"
                            label={tl("Aristas")}
                            lost={lost("edges")}
                            refs={d().edges}
                            onChange={(refs) => setDef({ ...d(), edges: refs as EdgeRef[] })}
                          />
                          <For each={refOptions(props.store, "curve", f().id)}>
                            {(o) => (
                              <Checkbox
                                small
                                label={o.label}
                                checked={d().curves.includes(+o.value.slice(4))}
                                onChange={(c) => setDef({ ...d(), curves: c ? [...d().curves, +o.value.slice(4)] : d().curves.filter((x) => x !== +o.value.slice(4)) })}
                              />
                            )}
                          </For>
                          <p class="text-[11px] text-text-dim">{tl("Las aristas y curvas se encadenan por sus extremos en una sola.")}</p>
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "projected" && (def() as Extract<CurveDef, { type: "projected" }>)}>
                      {(d) => (
                        <>
                          <Row label={tl("Curvas de")}>
                            <Select options={sketchOptions()} value={String(d().sketch)} onChange={(v) => setDef({ ...d(), sketch: +v, entities: [] })} />
                          </Row>
                          <SelectionBox
                            store={props.store}
                            ui={props.ui}
                            owner={`${f().id}:caras`}
                            kind="faces"
                            label={tl("Sobre las caras")}
                            lost={lost("faces")}
                            refs={d().faces}
                            onChange={(refs) => setDef({ ...d(), faces: refs as FaceRef[] })}
                          />
                          <p class="text-[11px] text-text-dim">{tl("Se proyectan a lo largo de la normal del sketch.")}</p>
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "intersection" && (def() as Extract<CurveDef, { type: "intersection" }>)}>
                      {(d) => (
                        <>
                          <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:a`} kind="faces" label={tl("Caras")} lost={lost("a")} refs={d().a} onChange={(refs) => setDef({ ...d(), a: refs as FaceRef[] })} />
                          <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:b`} kind="faces" label={tl("Con las caras")} lost={lost("b")} refs={d().b} onChange={(refs) => setDef({ ...d(), b: refs as FaceRef[] })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "scan_projected" && (def() as Extract<CurveDef, { type: "scan_projected" }>)}>
                      {(d) => (
                        <>
                          <Num label={tl("Holgura")} value={d().offset} step={0.1} suffix="mm" onCommit={(v) => setDef({ ...d(), offset: v })} />
                          <Checkbox small label={tl("Desde el otro lado del sketch")} checked={d().flip} onChange={(c) => setDef({ ...d(), flip: c })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "scan_path" && (def() as Extract<CurveDef, { type: "scan_path" }>)}>
                      {(d) => (
                        <>
                          <p class="text-[11px] text-text-dim leading-relaxed">
                            {tl("Camino más corto sobre la piel del escaneo por {v1} puntos; sigue al escaneo si se alinea o se repara.", { v1: d().seeds.length })}
                          </p>
                          <Checkbox small label={tl("Cerrada")} checked={d().closed} onChange={(c) => setDef({ ...d(), closed: c })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "points" && (def() as Extract<CurveDef, { type: "points" }>)}>
                      {(d) => (
                        <>
                          <For each={d().points}>
                            {(p, i) => (
                              <div class="flex items-start gap-1">
                                <div class="flex-1">
                                  <PointField
                                    store={props.store}
                                    ui={props.ui}
                                    owner={`${f().id}:p${i()}`}
                                    label={tl("Punto {v1}", { v1: i() + 1 })}
                                    value={p}
                                    except={f().id}
                                    onChange={(q) => setDef({ ...d(), points: d().points.map((x, k) => (k === i() ? q : x)) })}
                                  />
                                </div>
                                <IconButton
                                  aria-label={tl("Quitar el punto {v1}", { v1: i() + 1 })}
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => setDef({ ...d(), points: d().points.filter((_, k) => k !== i()) })}
                                >
                                  <Icons.Trash size={11} />
                                </IconButton>
                              </div>
                            )}
                          </For>
                          <Button
                            size="sm"
                            fullWidth
                            onClick={() => {
                              const last = d().points[d().points.length - 1];
                              const at: P3 = last?.type === "at" ? [last.point[0] + 10, last.point[1], last.point[2]] : [0, 0, 0];
                              setDef({ ...d(), points: [...d().points, { type: "at", point: at }] });
                            }}
                          >
                            {tl("Agregar punto")}
                          </Button>
                          <Checkbox small label={tl("Cerrada")} checked={d().closed} onChange={(c) => setDef({ ...d(), closed: c })} />
                        </>
                      )}
                    </Match>
                  </Switch>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "surface_extend" && (f().kind as Extract<FeatureKind, { type: "surface_extend" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:bordes`}
                  kind="edges"
                  label={tl("Bordes a extender")}
                  lost={lost("edges")}
                  refs={k().edges}
                  onChange={(refs) => update((x) => x.type === "surface_extend" && (x.edges = refs as EdgeRef[]))}
                />
                {field(tl("Distancia"), "kind.distance", k().distance, (x, v) => x.type === "surface_extend" && (x.distance = v), "mm")}
                <p class="text-[11px] text-text-dim">{tl("Bordes libres de superficies: cada cara se alarga por ese lado siguiendo su forma.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "wrap" && (f().kind as Extract<FeatureKind, { type: "wrap" }>)}>
            {(k) => (
              <>
                <Row label={tl("Sketch envuelto")}>
                  <Select
                    options={(props.store.doc()?.features ?? []).filter((x) => x.kind.type === "surface_sketch").map((x) => ({ value: String(x.id), label: x.name }))}
                    value={String(k().sketch)}
                    onChange={(v) => update((x) => x.type === "wrap" && (x.sketch = +v))}
                  />
                </Row>
                {field(tl("Profundidad"), "kind.depth", k().depth, (x, v) => x.type === "wrap" && (x.depth = v), "mm")}
                <Checkbox small label={tl("Relieve (sobresale)")} checked={k().emboss} onChange={(c) => update((x) => x.type === "wrap" && (x.emboss = c))} />
                <p class="text-[11px] text-text-dim">{tl("Las regiones cerradas del sketch, siguiendo la forma de la cara: hundidas (grabado) o levantadas (relieve).")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "split_faces" && (f().kind as Extract<FeatureKind, { type: "split_faces" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras a partir")}
                  lost={lost("faces")}
                  refs={k().faces}
                  onChange={(refs) => update((x) => x.type === "split_faces" && (x.faces = refs as FaceRef[]))}
                />
                <Row label={tl("Con")}>
                  <Select
                    options={[
                      { value: "plane", label: tl("Un plano") },
                      { value: "silhouette", label: tl("La silueta (vista desde un eje)") },
                      ...refOptions(props.store, "curve", f().id).map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label })),
                    ]}
                    value={k().tool.type === "plane" ? "plane" : k().tool.type === "silhouette" ? "silhouette" : `curve:${(k().tool as { feature: number }).feature}`}
                    onChange={(v) =>
                      update(
                        (x) =>
                          x.type === "split_faces" &&
                          (x.tool =
                            v === "plane" ? { type: "plane", plane: { type: "xy" } } : v === "silhouette" ? { type: "silhouette", direction: { type: "z" } } : { type: "curve", feature: +v.slice(6) }),
                      )
                    }
                  />
                </Row>
                <Show when={k().tool.type === "silhouette" && (k().tool as { direction: AxisSpec })}>
                  {(t) => (
                    <AxisField
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:mirada`}
                      label={tl("Mirando a lo largo de")}
                      value={t().direction}
                      except={f().id}
                      onChange={(a) => update((x) => x.type === "split_faces" && (x.tool = { type: "silhouette", direction: a }))}
                    />
                  )}
                </Show>
                <Show when={k().tool.type === "plane" && (k().tool as { plane: PlaneSpec })}>
                  {(t) => <Row label={tl("Plano")}>{planeSpecSelect(t().plane, (p) => update((x) => x.type === "split_faces" && (x.tool = { type: "plane", plane: p })))}</Row>}
                </Show>
                <p class="text-[11px] text-text-dim">{tl("Las caras quedan partidas (para desmoldes o colores distintos); la pieza sigue entera.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "rotate_face" && (f().kind as Extract<FeatureKind, { type: "rotate_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras a girar")}
                  lost={lost("faces")}
                  refs={k().faces}
                  onChange={(refs) => update((x) => x.type === "rotate_face" && (x.faces = refs as FaceRef[]))}
                />
                <AxisField store={props.store} ui={props.ui} owner={`${f().id}:eje`} label={tl("Bisagra")} value={k().axis} except={f().id} onChange={(a) => update((x) => x.type === "rotate_face" && (x.axis = a))} />
                {field(tl("Ángulo"), "kind.angle", k().angle, (x, v) => x.type === "rotate_face" && (x.angle = v), "°")}
                <p class="text-[11px] text-text-dim">{tl("La bisagra suele ser una arista de la cara. Lo que sale de la pieza suma material y lo que entra resta.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "vertex_chamfer" && (f().kind as Extract<FeatureKind, { type: "vertex_chamfer" }>)}>
            {(k) => (
              <>
                <Show
                  when={k().vertex.type === "picked" && (k().vertex as Extract<PointSpec, { type: "picked" }>)}
                  fallback={<PointField store={props.store} ui={props.ui} owner={`${f().id}:vertice`} label={tl("Vértice")} value={k().vertex} except={f().id} onChange={(p) => update((x) => x.type === "vertex_chamfer" && (x.vertex = p))} />}
                >
                  {(v) => (
                    <RefPicksBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:vertice`}
                      feature={f().id}
                      what="point"
                      refs={v().refs}
                      onChange={(refs) => update((x) => x.type === "vertex_chamfer" && (x.vertex = { type: "picked", refs }))}
                    />
                  )}
                </Show>
                {field(tl("Distancia"), "kind.distance", k().distance, (x, v) => x.type === "vertex_chamfer" && (x.distance = v), "mm")}
                <p class="text-[11px] text-text-dim">{tl("Clic en el vértice (una esquina de tres aristas rectas): se corta a esa distancia sobre cada arista.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "full_round" && (f().kind as Extract<FeatureKind, { type: "full_round" }>)}>
            {(k) => (
              <>
                <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:a`} kind="face" label={tl("Un costado")} lost={lost("side_a" as MissingRef["field"])} refs={k().side_a ? [k().side_a] : []} onChange={(r) => r[0] && update((x) => x.type === "full_round" && (x.side_a = r[0] as FaceRef))} />
                <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:m`} kind="face" label={tl("La cara del medio")} lost={lost("middle" as MissingRef["field"])} refs={k().middle ? [k().middle] : []} onChange={(r) => r[0] && update((x) => x.type === "full_round" && (x.middle = r[0] as FaceRef))} />
                <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:b`} kind="face" label={tl("El otro costado")} lost={lost("side_b" as MissingRef["field"])} refs={k().side_b ? [k().side_b] : []} onChange={(r) => r[0] && update((x) => x.type === "full_round" && (x.side_b = r[0] as FaceRef))} />
                <p class="text-[11px] text-text-dim">{tl("El radio es la mitad de la distancia entre los costados (por ahora, caras planas).")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "face_blend" && (f().kind as Extract<FeatureKind, { type: "face_blend" }>)}>
            {(k) => (
              <>
                <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:a`} kind="faces" label={tl("Caras de un lado")} lost={lost("a")} refs={k().a} onChange={(r) => update((x) => x.type === "face_blend" && (x.a = r as FaceRef[]))} />
                <SelectionBox store={props.store} ui={props.ui} owner={`${f().id}:b`} kind="faces" label={tl("Caras del otro lado")} lost={lost("b")} refs={k().b} onChange={(r) => update((x) => x.type === "face_blend" && (x.b = r as FaceRef[]))} />
                {field(tl("Radio"), "kind.radius", k().radius, (x, v) => x.type === "face_blend" && (x.radius = v), "mm")}
                <Row label={tl("Lado")}>
                  <Select options={BLEND_SIDE_OPTIONS} value={k().side ?? "auto"} onChange={(v) => update((x) => x.type === "face_blend" && (x.side = v === "auto" ? undefined : (v as BlendSide)))} />
                </Row>
                <p class="text-[11px] text-text-dim">
                  {tl("Una bola del radio rueda apoyada a la vez en las caras de los dos lados (pueden no ser vecinas, ni planas): si están en un canto lo redondea, si forman un rincón lo rellena. Cada lado puede ser una cadena de caras tangentes (un óvalo, una pared con redondeos).")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "delete_face" && (f().kind as Extract<FeatureKind, { type: "delete_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras a eliminar")}
                  lost={lost("faces")}
                  refs={k().faces}
                  onChange={(refs) => update((x) => x.type === "delete_face" && (x.faces = refs as FaceRef[]))}
                />
                <p class="text-[11px] text-text-dim">
                  {tl("Las caras vecinas se extienden hasta cerrar el hueco: sirve para tapar un agujero, sacar un redondeo o un resalte. Si las vecinas no se cruzan, da error.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "surface_planar" && (f().kind as Extract<FeatureKind, { type: "surface_planar" }>)}>
            {(k) => (
              <>
                <Row label={tl("Sketch")}>
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "surface_planar" && ((x.sketch = +v), (x.regions = { type: "all" })))} />
                </Row>
                <RegionBox
                  ui={props.ui}
                  store={props.store}
                  owner={`${f().id}:regiones`}
                  lost={lost("regions")}
                  sketch={k().sketch}
                  value={k().regions}
                  onChange={(r) => update((x) => x.type === "surface_planar" && (x.regions = r))}
                />
                <p class="text-[11px] text-text-dim">{tl("Las regiones quedan como una superficie sin espesor, aparte de las piezas.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "surface_offset" && (f().kind as Extract<FeatureKind, { type: "surface_offset" }>)}>
            {(k) => (
              <>
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={tl("Desfasar")}
                  empty={tl("Elegir superficies o piezas")}
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "surface_offset" && (x.parts = v))}
                />
                {field(tl("Distancia"), "kind.distance", k().distance, (x, v) => x.type === "surface_offset" && (x.distance = v), "mm")}
                <p class="text-[11px] text-text-dim">
                  {tl("Una copia a esa distancia sobre la normal (negativa, hacia el otro lado). De una pieza sólida se copia su piel. Queda como superficie nueva.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "transform" && (f().kind as Extract<FeatureKind, { type: "transform" }>)}>
            {(k) => {
              const [alignPending, setAlignPending] = createSignal(false);
              return (
              <>
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={tl("Piezas")}
                  empty={tl("Todas")}
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "transform" && (x.parts = v))}
                />
                <Row label={tl("Movimiento")}>
                  <Select
                    options={[
                      { value: "translate", label: tl("Desplazar") },
                      { value: "rotate", label: tl("Girar") },
                      { value: "points", label: tl("De un punto a otro") },
                      { value: "align", label: tl("Alinear una cara a un plano") },
                    ]}
                    value={alignPending() ? "align" : k().motion.type}
                    onChange={(v) => {
                      // Alinear necesita la cara: se elige primero en el visor
                      setAlignPending(v === "align" && k().motion.type !== "align");
                      if (v === "align") return;
                      update((x) => {
                        if (x.type !== "transform" || x.motion.type === v) return;
                        x.motion =
                          v === "rotate"
                            ? { type: "rotate", axis: { type: "z" }, angle: 90 }
                            : v === "points"
                              ? { type: "points", from: { type: "at", point: [0, 0, 0] }, to: { type: "at", point: [20, 0, 0] } }
                              : { type: "translate", vector: [20, 0, 0] };
                      });
                    }}
                  />
                </Row>
                <Show when={alignPending() || k().motion.type === "align"}>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:cara`}
                    kind="face"
                    label={tl("Cara a apoyar")}
                    lost={lost("face")}
                    refs={k().motion.type === "align" ? [(k().motion as Extract<Motion, { type: "align" }>).face] : []}
                    onChange={(refs) => {
                      const face = refs[refs.length - 1] as FaceRef | undefined;
                      if (!face) return;
                      setAlignPending(false);
                      update((x) => {
                        if (x.type !== "transform") return;
                        x.motion = x.motion.type === "align" ? { ...x.motion, face } : { type: "align", face, to: { type: "xy" }, flip: false };
                      });
                    }}
                  />
                </Show>
                <Show when={k().motion.type === "align" && (k().motion as Extract<Motion, { type: "align" }>)}>
                  {(m) => (
                    <>
                      <Row label={tl("Sobre el plano")}>{planeSpecSelect(m().to, (p) => update((x) => x.type === "transform" && x.motion.type === "align" && (x.motion.to = p)))}</Row>
                      <Checkbox
                        small
                        label={tl("Del otro lado del plano")}
                        checked={m().flip}
                        onChange={(c) => update((x) => x.type === "transform" && x.motion.type === "align" && (x.motion.flip = c))}
                      />
                    </>
                  )}
                </Show>
                <Show when={k().motion.type === "translate" && (k().motion as Extract<Motion, { type: "translate" }>)}>
                  {(m) => (
                    <div class="grid grid-cols-3 gap-1" aria-label={tl("Desplazamiento")}>
                      <For each={["X", "Y", "Z"]}>
                        {(axis, i) =>
                          field(axis, `kind.motion.vector.${i()}`, m().vector[i()], (x, v) => x.type === "transform" && x.motion.type === "translate" && (x.motion.vector[i()] = v))
                        }
                      </For>
                    </div>
                  )}
                </Show>
                <Show when={k().motion.type === "rotate" && (k().motion as Extract<Motion, { type: "rotate" }>)}>
                  {(m) => (
                    <>
                      <AxisField
                        store={props.store}
                        ui={props.ui}
                        owner={`${f().id}:eje`}
                        label={tl("Eje")}
                        value={m().axis}
                        except={f().id}
                        onChange={(a) => update((x) => x.type === "transform" && x.motion.type === "rotate" && (x.motion.axis = a))}
                      />
                      {field(tl("Ángulo"), "kind.motion.angle", m().angle, (x, v) => x.type === "transform" && x.motion.type === "rotate" && (x.motion.angle = v), "°")}
                    </>
                  )}
                </Show>
                <Show when={k().motion.type === "points" && (k().motion as Extract<Motion, { type: "points" }>)}>
                  {(m) => (
                    <>
                      <PointField
                        store={props.store}
                        ui={props.ui}
                        owner={`${f().id}:desde`}
                        label={tl("Desde")}
                        value={m().from}
                        except={f().id}
                        onChange={(p) => update((x) => x.type === "transform" && x.motion.type === "points" && (x.motion.from = p))}
                      />
                      <PointField
                        store={props.store}
                        ui={props.ui}
                        owner={`${f().id}:hasta`}
                        label={tl("Hasta")}
                        value={m().to}
                        except={f().id}
                        onChange={(p) => update((x) => x.type === "transform" && x.motion.type === "points" && (x.motion.to = p))}
                      />
                    </>
                  )}
                </Show>
                <Checkbox small label={tl("Copiar (las originales quedan)")} checked={k().copy} onChange={(c) => update((x) => x.type === "transform" && (x.copy = c))} />
                <p class="text-[11px] text-text-dim">
                  {tl("Sin piezas elegidas mueve todas (al alinear, la de la cara). Las copias quedan como piezas nuevas.")}
                </p>
              </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "split" && (f().kind as Extract<FeatureKind, { type: "split" }>)}>
            {(k) => {
              const base = (): "xy" | "xz" | "yz" => {
                const p = k().plane;
                if (p.type === "custom") {
                  const n = p.plane.normal;
                  return Math.abs(n[2]) > 0.9 ? "xy" : Math.abs(n[1]) > 0.9 ? "xz" : "yz";
                }
                return p.type === "face" || p.type === "reference" || p.type === "frame" ? "xy" : p.type;
              };
              const offset = () => {
                const p = k().plane;
                if (p.type !== "custom") return 0;
                const n = BASE_PLANES[base()].normal;
                return p.plane.origin[0] * n[0] + p.plane.origin[1] * n[1] + p.plane.origin[2] * n[2];
              };
              const set = (b: "xy" | "xz" | "yz", o: number) => update((x) => x.type === "split" && (x.plane = { type: "custom", plane: offsetPlane(b, o) }));
              return (
                <>
                  <Row label={tl("Plano")}>
                    <Select options={(["xy", "xz", "yz"] as const).map((v) => ({ value: v, label: PLANE_LABELS[v] }))} value={base()} onChange={(v) => set(v as "xy", offset())} />
                  </Row>
                  <Num label={tl("A distancia")} suffix="mm" value={offset()} onCommit={(v) => set(base(), v)} />
                  <Checkbox small label={tl("Conservar el otro lado")} checked={k().flip} onChange={(c) => update((x) => x.type === "split" && (x.flip = c))} />
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "import" && (f().kind as Extract<FeatureKind, { type: "import" }>)}>
            {(k) => (
              <>
                <p class="text-[11px] text-text-dim">{k().format.toUpperCase()} · {(k().data.length / 1024).toFixed(0)} KB</p>
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "import" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "derived" && (f().kind as Extract<FeatureKind, { type: "derived" }>)}>
            {(k) => (
              <>
                <DerivedBox store={props.store} kind={k()} missing={lost("parts")} update={update} />
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "derived" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "scan_body" && (f().kind as Extract<FeatureKind, { type: "scan_body" }>)}>
            {(k) => (
              <>
                <p class="text-[11px] text-text-dim leading-relaxed">
                  {tl("El escaneo cerrado por vóxeles. Sigue al escaneo: si se alinea, se repara o se elige otro, se rehace.")}
                </p>
                <Num label={tl("Holgura")} suffix="mm" step={0.05} value={k().offset} onCommit={(v) => update((x) => x.type === "scan_body" && (x.offset = v))} />
                <p class="text-[11px] text-text-dim leading-relaxed">{tl("Positiva agranda el escaneo (el encaje no aprieta); negativa lo achica.")}</p>
                <Row label={tl("Forma")}>
                  <Select
                    options={[
                      { value: "full", label: tl("Lleno") },
                      { value: "out", label: tl("Carcasa hacia afuera") },
                      { value: "in", label: tl("Carcasa hacia adentro") },
                    ]}
                    value={!k().thickness ? "full" : k().thickness! > 0 ? "out" : "in"}
                    onChange={(v) =>
                      update((x) => {
                        if (x.type !== "scan_body") return;
                        const t = Math.abs(x.thickness || 2);
                        x.thickness = v === "full" ? 0 : v === "out" ? t : -t;
                      })
                    }
                  />
                </Row>
                <Show when={k().thickness}>
                  <Num
                    label={tl("Espesor")}
                    suffix="mm"
                    step={0.5}
                    min={0.1}
                    value={Math.abs(k().thickness ?? 0)}
                    onCommit={(v) => update((x) => x.type === "scan_body" && (x.thickness = Math.sign(x.thickness || 1) * Math.max(0.1, Math.abs(v))))}
                  />
                  <p class="text-[11px] text-text-dim leading-relaxed">
                    {tl("Desde la piel corrida la holgura. En escaneos grandes hay un mínimo (la grilla de vóxeles tiene un tope).")}
                  </p>
                </Show>
                <Row label={tl("Solo bajo")}>
                  <Select
                    options={[{ value: "", label: tl("Todo el escaneo") }, ...sketchOptions()]}
                    value={k().region ? String(k().region!.sketch) : ""}
                    onChange={(v) => update((x) => x.type === "scan_body" && (x.region = v ? { sketch: +v, regions: { type: "all" } } : null))}
                  />
                </Row>
                <Show when={k().region}>
                  {(r) => (
                    <>
                      <RegionBox
                        ui={props.ui}
                        store={props.store}
                        owner={`${f().id}:regiones`}
                        lost={lost("regions")}
                        sketch={r().sketch}
                        value={r().regions}
                        onChange={(sel) => update((x) => x.type === "scan_body" && x.region && (x.region.regions = sel))}
                      />
                      <p class="text-[11px] text-text-dim leading-relaxed">
                        {tl("El contorno se proyecta por la normal del sketch: queda el primer tramo del escaneo hacia cada lado.")}
                      </p>
                    </>
                  )}
                </Show>
                <Num
                  label={tl("Triángulos")}
                  step={1000}
                  min={500}
                  value={k().faces}
                  onCommit={(v) => update((x) => x.type === "scan_body" && (x.faces = Math.round(Math.min(100000, Math.max(500, v)))))}
                />
                <p class="text-[11px] text-text-dim leading-relaxed">{tl("Más es más fiel y hace más lentas las operaciones (unos 3 s cada 20 000).")}</p>
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "scan_body" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "boolean" && (f().kind as Extract<FeatureKind, { type: "boolean" }>)}>
            {(k) => (
              <>
                <Row label={tl("Operación")}>
                  <Select
                    options={[
                      { value: "union", label: tl("Unir") },
                      { value: "subtract", label: tl("Restar") },
                      { value: "intersect", label: tl("Intersecar") },
                    ]}
                    value={k().op}
                    onChange={(v) => update((x) => x.type === "boolean" && (x.op = v as "union" | "subtract" | "intersect"))}
                  />
                </Row>
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={k().op === "subtract" ? tl("Piezas que quedan") : tl("Piezas")}
                  empty={tl("Elegir las piezas")}
                  value={k().targets}
                  missing={lost("targets")}
                  onChange={(v) => update((x) => x.type === "boolean" && (x.targets = v))}
                />
                <Show when={k().op !== "union" || k().tools.length > 0}>
                  <PartChecklist
                    store={props.store}
                    ui={props.ui}
                    featureId={f().id}
                    label={k().op === "subtract" ? tl("Piezas que restan") : tl("Con")}
                    empty={k().op === "subtract" ? tl("Elegir con qué restar") : tl("Ninguna más")}
                    value={k().tools}
                    missing={lost("tools")}
                    onChange={(v) => update((x) => x.type === "boolean" && (x.tools = v))}
                  />
                </Show>
                <Show when={k().op === "subtract"}>
                  <Checkbox small label={tl("Conservar las que restan")} checked={k().keep_tools} onChange={(c) => update((x) => x.type === "boolean" && (x.keep_tools = c))} />
                </Show>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "split_by" && (f().kind as Extract<FeatureKind, { type: "split_by" }>)}>
            {(k) => (
              <>
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={tl("Piezas a partir")}
                  empty={tl("Todos los sólidos")}
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "split_by" && (x.parts = v))}
                />
                <Row label={tl("Con")}>
                  <Select
                    options={[
                      { value: "plane", label: tl("Un plano") },
                      ...(props.store.result()?.parts ?? []).map((p) => ({ value: `part:${partKey(p.id)}`, label: `${p.name}${p.surface ? " (superficie)" : ""}` })),
                    ]}
                    value={k().tool.type === "plane" ? "plane" : `part:${partKey((k().tool as { part: PartId }).part)}`}
                    onChange={(v) =>
                      update((x) => {
                        if (x.type !== "split_by") return;
                        if (v === "plane") x.tool = { type: "plane", plane: { type: "custom", plane: offsetPlane("xy", 0) } };
                        else {
                          const part = (props.store.result()?.parts ?? []).find((p) => `part:${partKey(p.id)}` === v);
                          if (part) x.tool = { type: "part", part: part.id };
                        }
                      })
                    }
                  />
                </Row>
                <Show when={k().tool.type === "plane" && (k().tool as { plane: PlaneSpec })}>
                  {(t) => <Row label={tl("Plano")}>{planeSpecSelect(t().plane, (p) => update((x) => x.type === "split_by" && x.tool.type === "plane" && (x.tool.plane = p)), lost("plane").length > 0)}</Row>}
                </Show>
                <p class="text-[11px] text-text-dim">
                  {tl("Cada pedazo queda como una pieza. Con otra pieza (una superficie curva, por ejemplo) se parte por su forma; la herramienta queda como estaba. Las superficies se recortan con Superficies → Recortar.")}
                </p>
              </>
            )}
          </Match>
          <Match when={(f().kind.type === "split_parts" || f().kind.type === "delete_parts") && f().kind}>
            {(k) => {
              const kind = k() as Extract<FeatureKind, { type: "split_parts" | "delete_parts" }>;
              return (
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={kind.type === "split_parts" ? tl("Piezas a separar") : tl("Piezas a borrar")}
                  empty={kind.type === "split_parts" ? tl("Todas") : tl("Elegir las piezas")}
                  value={(k() as typeof kind).parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => (x.type === "split_parts" || x.type === "delete_parts") && (x.parts = v))}
                />
              );
            }}
          </Match>
          <Match when={f().kind.type === "hole" && (f().kind as Extract<FeatureKind, { type: "hole" }>)}>
            {(k) => {
              // Tamaño estándar que corresponde a los diámetros actuales (si alguno)
              const ALL_HOLES = [...METRIC_HOLES, ...INCH_HOLES];
              const fit = (): { size: string; mode: "clearance" | "tap" | "modeled" } | null => {
                const mod = k().modeled;
                if (mod)
                  return {
                    size: ALL_HOLES.find((m) => Math.abs(m.nominal - mod.nominal) < 1e-6 && Math.abs(m.pitch - mod.pitch) < 1e-6)?.size ??
                      METRIC_HOLES.find((m) => Math.abs(m.nominal - mod.nominal) < 1e-6)?.size ??
                      "",
                    mode: "modeled",
                  };
                // La rosca cosmética dice la medida (el pasante y la broca de UNC y UNF a veces coinciden)
                const named = ALL_HOLES.find((m) => m.size === k().thread);
                if (named) return { size: named.size, mode: "tap" };
                for (const m of ALL_HOLES) {
                  if (Math.abs(m.clearance - k().diameter) < 1e-6) return { size: m.size, mode: "clearance" };
                  if (Math.abs(m.tap - k().diameter) < 1e-6) return { size: m.size, mode: "tap" };
                }
                return null;
              };
              // Sistema de medidas: el de la medida actual, o el elegido
              const [inch, setInch] = createSignal(INCH_HOLES.some((m) => m.size === fit()?.size));
              const table = () => (inch() ? INCH_HOLES : METRIC_HOLES);
              const applySize = (size: string, mode: "clearance" | "tap" | "modeled") => {
                const m = ALL_HOLES.find((x) => x.size === size);
                if (!m) return;
                update((x) => {
                  if (x.type !== "hole") return;
                  x.diameter = mode === "clearance" ? m.clearance : m.tap;
                  x.thread = mode === "clearance" ? null : m.size;
                  // Al cambiar de medida la rosca modelada toma el paso grueso y conserva la holgura
                  x.modeled = mode === "modeled" ? { nominal: m.nominal, pitch: m.pitch, clearance: x.modeled?.clearance ?? 0, left: x.modeled?.left ?? false } : null;
                  if (x.style.type === "counterbore") x.style = { type: "counterbore", diameter: m.cbore, depth: m.cboreDepth };
                  if (x.style.type === "countersink") x.style = { type: "countersink", diameter: m.csink, angle: (m as { csinkAngle?: number }).csinkAngle ?? 90 };
                });
              };
              const view = () => props.store.sketchView(k().sketch);
              const sketchPoints = () => {
                const s = props.sketches.find((x) => x.id === k().sketch);
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "point" && !e.construction) : [];
              };
              return (
                <>
                  <Row label={tl("Centros en")}>
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "hole" && ((x.sketch = +v), (x.points = [])))} />
                  </Row>
                  <p class="text-[11px] text-text-dim">
                    {sketchPoints().length ? tl("{length} puntos del sketch", { length: sketchPoints().length }) : tl("{v1} centros de círculos", { v1: view()?.sketch.entities.filter((e) => e.geometry.type === "circle").length ?? 0 })}
                  </p>
                  <Row label={tl("Tipo")}>
                    <Select
                      options={[
                        { value: "simple", label: tl("Simple") },
                        { value: "counterbore", label: tl("Con caja") },
                        { value: "countersink", label: tl("Avellanado") },
                        { value: "tapered", label: tl("Cónico") },
                      ]}
                      value={k().style.type}
                      onChange={(v) => {
                        if (v === k().style.type) return;
                        const m = ALL_HOLES.find((x) => x.size === fit()?.size) ?? METRIC_HOLES.find((x) => x.size === "M6")!;
                        update((x) => {
                          if (x.type !== "hole") return;
                          x.style =
                            v === "counterbore"
                              ? { type: "counterbore", diameter: m.cbore, depth: m.cboreDepth }
                              : v === "countersink"
                                ? { type: "countersink", diameter: m.csink, angle: (m as { csinkAngle?: number }).csinkAngle ?? 90 }
                                : v === "tapered"
                                  ? { type: "tapered", angle: 1.7833 }
                                  : { type: "simple" };
                          // Un cónico no lleva rosca modelada: queda cosmética
                          if (v === "tapered" && x.modeled) {
                            x.thread = threadName(x.modeled);
                            x.modeled = null;
                          }
                        });
                      }}
                    />
                  </Row>
                  <Show when={k().thread || k().modeled}>
                    <ThreadLinkBox
                      store={props.store}
                      ui={props.ui}
                      feature={f().id}
                      value={k().link}
                      want="any"
                      what={tl("el agujero")}
                      onChange={(link, src) =>
                        update((x) => {
                          if (x.type !== "hole") return;
                          x.link = link;
                          if (!src) return;
                          if (x.modeled) x.modeled = { ...src.spec, clearance: x.modeled.clearance };
                          else {
                            x.thread = threadName(src.spec);
                            x.diameter = src.spec.nominal - src.spec.pitch;
                          }
                        })
                      }
                    />
                  </Show>
                  <Show when={!k().link}>
                  <Row label={tl("Sistema")}>
                    <Select
                      options={[
                        { value: "metric", label: tl("Métrico (ISO)") },
                        { value: "inch", label: tl("Pulgadas (UNC/UNF)") },
                      ]}
                      value={inch() ? "inch" : "metric"}
                      onChange={(v) => {
                        if ((v === "inch") === inch()) return;
                        setInch(v === "inch");
                        // La medida más parecida del otro sistema, con el mismo uso
                        const now = ALL_HOLES.find((m) => m.size === fit()?.size);
                        const near = (now ? [...table()].sort((a, b) => Math.abs(a.nominal - now.nominal) - Math.abs(b.nominal - now.nominal))[0] : undefined) ?? table()[0];
                        applySize(near.size, fit()?.mode ?? "clearance");
                      }}
                    />
                  </Row>
                  <Row label={tl("Tamaño")}>
                    <Select
                      options={[{ value: "", label: tl("A medida") }, ...table().map((m) => ({ value: m.size, label: m.size }))]}
                      value={table().some((m) => m.size === fit()?.size) ? (fit()?.size ?? "") : ""}
                      onChange={(v) => v && applySize(v, fit()?.mode ?? "clearance")}
                    />
                  </Row>
                  </Show>
                  <Show when={fit()}>
                    {(fi) => (
                      <Row label={tl("Para")}>
                        <Select
                          options={[
                            { value: "clearance", label: tl("Que pase el tornillo") },
                            { value: "tap", label: tl("Roscar (rosca cosmética)") },
                            { value: "modeled", label: tl("Roscar (rosca modelada)") },
                          ]}
                          value={fi().mode}
                          onChange={(v) => applySize(fi().size || "M6", v as "clearance" | "tap" | "modeled")}
                        />
                      </Row>
                    )}
                  </Show>
                  <Show
                    when={k().modeled}
                    fallback={field(tl("Diámetro"), "kind.diameter", k().diameter, (x, v) => x.type === "hole" && (x.diameter = v), "mm")}
                  >
                    {(t) => (
                      <>
                        <Show when={!k().link}>
                          {field(tl("Paso"), "kind.modeled.pitch", t().pitch, (x, v) => x.type === "hole" && x.modeled && (x.modeled.pitch = v), "mm")}
                        </Show>
                        {field(tl("Holgura"), "kind.modeled.clearance", t().clearance, (x, v) => x.type === "hole" && x.modeled && (x.modeled.clearance = v), "mm")}
                        <Show when={!k().link}>
                          <Checkbox small label={tl("A izquierdas")} checked={t().left} onChange={(c) => update((x) => x.type === "hole" && !!x.modeled && (x.modeled.left = c))} />
                        </Show>
                        <p class="text-[11px] text-text-dim">
                          {threadName(t())}{tl(": el agujero sale con el filete (diámetro menor")} {fmt(t().nominal - 1.082532 * t().pitch + t().clearance)} {tl("mm). Para imprimir en FDM conviene una holgura de 0,2–0,4 mm. Cada vuelta del filete tarda en calcularse.")}
                        </p>
                      </>
                    )}
                  </Show>
                  <Row label={tl("Profundidad")}>
                    <Select
                      options={[
                        { value: "through_all", label: tl("Pasante") },
                        { value: "blind", label: tl("Ciega") },
                      ]}
                      value={k().depth.type}
                      onChange={(v) => v !== k().depth.type && update((x) => x.type === "hole" && (x.depth = v === "blind" ? { type: "blind", depth: 10 } : { type: "through_all" }))}
                    />
                  </Row>
                  {/* Sin elegir, hacia el lado donde hay material; la flecha del visor también lo da vuelta */}
                  <div class="flex items-center justify-between gap-2">
                    <Checkbox
                      small
                      label={tl("Hacia el otro lado")}
                      checked={k().reverse ?? state()?.handles?.[0]?.reversed ?? false}
                      onChange={(c) => update((x) => x.type === "hole" && (x.reverse = c))}
                    />
                    <Show when={k().reverse != null} fallback={<span class="text-[11px] text-text-dim">{tl("Automático")}</span>}>
                      <button
                        class="text-[11px] text-text-muted hover:text-text"
                        title={tl("Que vuelva a ir hacia el lado donde hay material")}
                        onClick={() => update((x) => x.type === "hole" && (x.reverse = null))}
                      >
                        {tl("Automático")}
                      </button>
                    </Show>
                  </div>
                  <Checkbox
                    small
                    label={tl("Compensar para imprimir")}
                    checked={!!k().compensate}
                    onChange={(c) => update((x) => x.type === "hole" && (x.compensate = c))}
                    hint={tl("El diámetro es el que quieres medir impreso: se agranda lo que la impresora achica los agujeros (según la impresora elegida en Fabricar).")}
                  />
                  <Show when={k().depth.type === "blind"}>
                    {field(tl("Hasta"), "kind.depth.depth", (k().depth as { depth: number }).depth, (x, v) => x.type === "hole" && x.depth.type === "blind" && (x.depth.depth = v), "mm")}
                    {field(tl("Punta"), "kind.tip_angle", k().tip_angle, (x, v) => x.type === "hole" && (x.tip_angle = v), "°")}
                  </Show>
                  <Show when={k().style.type === "counterbore" && (k().style as { diameter: number; depth: number })}>
                    {(st) => (
                      <>
                        {field(tl("Caja: diámetro"), "kind.style.diameter", st().diameter, (x, v) => x.type === "hole" && x.style.type === "counterbore" && (x.style.diameter = v), "mm")}
                        {field("Caja: profundidad", "kind.style.depth", st().depth, (x, v) => x.type === "hole" && x.style.type === "counterbore" && (x.style.depth = v), "mm")}
                      </>
                    )}
                  </Show>
                  <Show when={k().style.type === "countersink" && (k().style as { diameter: number; angle: number })}>
                    {(st) => (
                      <>
                        {field(tl("Avellanado: diámetro"), "kind.style.diameter", st().diameter, (x, v) => x.type === "hole" && x.style.type === "countersink" && (x.style.diameter = v), "mm")}
                        {field(tl("Avellanado: ángulo"), "kind.style.angle", st().angle, (x, v) => x.type === "hole" && x.style.type === "countersink" && (x.style.angle = v), "°")}
                      </>
                    )}
                  </Show>
                  <Show when={k().style.type === "tapered" && (k().style as { angle: number })}>
                    {(st) => (
                      <>
                        {field(tl("Cono: ángulo"), "kind.style.angle", st().angle, (x, v) => x.type === "hole" && x.style.type === "tapered" && (x.style.angle = v), "°")}
                        <p class="text-[11px] text-text-dim">
                          {tl("Ángulo de la pared con el eje (NPT: 1°47' = 1,7833°). El diámetro es el de la boca; se angosta hasta el fondo.")}
                        </p>
                      </>
                    )}
                  </Show>
                  <Show when={k().thread && !k().modeled}>
                    <p class="text-[11px] text-text-dim">{tl("Rosca")} {k().thread} {tl("(cosmética: no se modela)")}</p>
                  </Show>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "sketch3d" && f()}>{(ff) => <Sketch3dEditor store={props.store} ui={props.ui} feature={ff()} />}</Match>
          <Match when={f().kind.type === "surface_sketch" && (f().kind as Extract<FeatureKind, { type: "surface_sketch" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:cara`}
                  kind="face"
                  label={tl("Cara")}
                  lost={lost("face")}
                  refs={[k().face]}
                  onChange={(refs) => refs[0] && update((x) => x.type === "surface_sketch" && (x.face = refs[0] as FaceRef))}
                />
                <Button size="sm" fullWidth onClick={() => void props.store.acceptDraft().then(() => props.ui.editSketch(f().id))}>
                  {tl("Editar el sketch")}
                </Button>
                <p class="text-[11px] text-text-dim">
                  {tl("Se dibuja en el desarrollo de la cara (el plano tangente en su centro: X a lo largo de la cara, Y a lo ancho) y las curvas se llevan sobre ella midiendo sobre la superficie. Sirven de camino para un Barrido.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "helix" && (f().kind as Extract<FeatureKind, { type: "helix" }>)}>
            {(k) => (
              <>
                <AxisField store={props.store} ui={props.ui} owner={`${f().id}:eje`} label={tl("Eje")} value={k().axis} except={f().id} onChange={(a) => update((x) => x.type === "helix" && (x.axis = a))} />
                {field(k().radius2 != null ? tl("Radio inicial") : tl("Radio"), "kind.radius", k().radius, (x, v) => x.type === "helix" && (x.radius = v), "mm")}
                <Checkbox
                  small
                  label={tl("Espiral plana")}
                  checked={!!k().flat}
                  onChange={(c) =>
                    update((x) => {
                      if (x.type !== "helix") return;
                      x.flat = c || undefined;
                      if (c && x.radius2 == null) x.radius2 = x.radius * 2;
                    })
                  }
                />
                <Checkbox
                  small
                  label={tl("Cónica (dos radios)")}
                  checked={k().radius2 != null}
                  onChange={(c) =>
                    update((x) => {
                      if (x.type !== "helix") return;
                      x.radius2 = c ? x.radius / 2 : null;
                      if (!c) x.flat = undefined;
                    })
                  }
                />
                <Show when={k().radius2 != null}>
                  {field(tl("Radio final"), "kind.radius2", k().radius2 ?? k().radius, (x, v) => x.type === "helix" && (x.radius2 = v), "mm")}
                </Show>
                <Show when={!k().flat}>
                  {field(tl("Paso"), "kind.pitch", k().pitch, (x, v) => x.type === "helix" && (x.pitch = v), "mm")}
                </Show>
                {field(tl("Vueltas"), "kind.turns", k().turns, (x, v) => x.type === "helix" && (x.turns = v))}
                <Checkbox small label={tl("A izquierdas")} checked={k().left} onChange={(c) => update((x) => x.type === "helix" && (x.left = c))} />
                <p class="text-[11px] text-text-dim">{tl("Para un resorte: un círculo en un plano que pase por el eje, en el arranque de la hélice, y un Barrido con la hélice como camino.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "move_face" && (f().kind as Extract<FeatureKind, { type: "move_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras planas")}
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "move_face" && (x.faces = refs as FaceRef[]))}
                />
                {field(tl("Distancia"), "kind.distance", k().distance, (x, v) => x.type === "move_face" && (x.distance = v), "mm")}
                <p class="text-[11px] text-text-dim">{tl("Positiva: hacia afuera (suma material); negativa: hacia adentro.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sheet_metal" && (f().kind as Extract<FeatureKind, { type: "sheet_metal" }>)}>
            {(k) => (
              <>
                <Row label={tl("Sketch")}>
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "sheet_metal" && (x.sketch = +v))} />
                </Row>
                <RegionBox
                  ui={props.ui}
                  store={props.store}
                  owner={`${f().id}:regiones`}
                  lost={lost("regions")}
                  sketch={k().sketch}
                  value={k().regions}
                  onChange={(r) => update((x) => x.type === "sheet_metal" && (x.regions = r))}
                />
                {field(tl("Espesor"), "kind.thickness", k().thickness, (x, v) => x.type === "sheet_metal" && (x.thickness = v), "mm")}
                {field(tl("Radio de doblez"), "kind.radius", k().radius, (x, v) => x.type === "sheet_metal" && (x.radius = v), "mm")}
                {field(tl("Factor K"), "kind.k_factor", k().k_factor, (x, v) => x.type === "sheet_metal" && (x.k_factor = v))}
                <Checkbox small label={tl("Hacia el otro lado")} checked={k().flip} onChange={(c) => update((x) => x.type === "sheet_metal" && (x.flip = c))} />
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "sheet_metal" && (x.op = o)))}</Row>
                <p class="text-[11px] text-text-dim">
                  {tl("El radio es el interior de los dobleces de sus pestañas. El factor K dice dónde queda la fibra neutra (0,44 para acero y aluminio doblados en plegadora; 0,5 si el radio es grande): con él se calcula el largo del desarrollo.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "flange" && (f().kind as Extract<FeatureKind, { type: "flange" }>)}>
            {(k) => {
              // El radio de la chapa de la que sale (la última antes)
              const sheet = () => {
                const list = props.store.doc()?.features ?? [];
                const before = list.slice(0, list.findIndex((x) => x.id === f().id));
                const sm = [...before].reverse().find((x) => x.kind.type === "sheet_metal");
                return sm?.kind.type === "sheet_metal" ? sm.kind : undefined;
              };
              const sheetRadius = () => sheet()?.radius;
              const sheetThickness = () => sheet()?.thickness;
              return (
                <>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:arista`}
                    kind="edges"
                    label={tl("Arista del borde")}
                    refs={k().edge ? [k().edge!] : []}
                    lost={lost("edge")}
                    onChange={(refs) => update((x) => x.type === "flange" && (x.edge = (refs[refs.length - 1] as EdgeRef | undefined) ?? null))}
                  />
                  {field(tl("Largo"), "kind.length", k().length, (x, v) => x.type === "flange" && (x.length = v), "mm")}
                  {field(tl("Ángulo"), "kind.angle", k().angle, (x, v) => x.type === "flange" && (x.angle = v), "°")}
                  <Show when={k().angle >= 180}>
                    <p class="text-[11px] text-text-dim">{tl("180° deja un dobladillo: la pared vuelve sobre la chapa.")}</p>
                  </Show>
                  {field(tl("Radio"), "kind.radius", k().radius ?? sheetRadius() ?? 0, (x, v) => x.type === "flange" && (x.radius = v), "mm")}
                  <Checkbox small label={tl("Doblar hacia el otro lado")} checked={k().flip} onChange={(c) => update((x) => x.type === "flange" && (x.flip = c))} />
                  <p class="text-[11px] text-text-dim">
                    {tl("Dobla hacia el lado de la cara de la arista elegida. El largo es la pared después del doblez; el radio, el interior (el de la chapa si no se cambia).")}
                  </p>
                  <Row label={tl("Alivio")}>
                    <Select
                      options={RELIEF_OPTIONS}
                      value={k().relief ?? "rectangle"}
                      onChange={(v) => update((x) => x.type === "flange" && (x.relief = v as BendRelief))}
                    />
                  </Row>
                  <Show when={(k().relief ?? "rectangle") !== "none"}>
                    {field(tl("Ancho del alivio"), "kind.relief_width", k().relief_width ?? sheetThickness() ?? 0, (x, v) => x.type === "flange" && (x.relief_width = v), "mm")}
                  </Show>
                  <Row label={tl("Esquina")}>
                    <Select
                      options={CORNER_OPTIONS}
                      value={k().corner_relief ?? "none"}
                      onChange={(v) => update((x) => x.type === "flange" && (x.corner_relief = v as CornerRelief))}
                    />
                  </Row>
                  <Checkbox
                    small
                    label={tl("Esquina cerrada con otra pestaña")}
                    checked={!!k().closed_corner}
                    onChange={(c) => update((x) => x.type === "flange" && (x.closed_corner = c))}
                  />
                  <Show when={k().closed_corner}>
                    {field(tl("Holgura de la esquina"), "kind.corner_gap", k().corner_gap ?? 0.2, (x, v) => x.type === "flange" && (x.corner_gap = v), "mm")}
                  </Show>
                  <Show when={(k().corner_relief ?? "none") !== "none"}>
                    {field(
                      k().corner_relief === "square" ? tl("Medio lado") : tl("Radio del alivio"),
                      "kind.corner_size",
                      k().corner_size ?? sheetThickness() ?? 0,
                      (x, v) => x.type === "flange" && (x.corner_size = v),
                      "mm",
                    )}
                  </Show>
                  <p class="text-[11px] text-text-dim">
                    {tl("El alivio es una ranura en el extremo donde la chapa sigue al lado del doblez (una esquina hacia adentro): sin ella el doblez quedaría pegado y la chapa se rasgaría. Donde se junta con otra pestaña queda una muesca cuadrada del largo del doblez; también puede ser redonda o cuadrada más grande. El ancho, si no se cambia, es el espesor.")}
                  </p>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "thread" && (f().kind as Extract<FeatureKind, { type: "thread" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:cara`}
                  kind="face"
                  label={tl("Cara cilíndrica")}
                  refs={[k().face]}
                  lost={lost("face")}
                  onChange={(refs) => refs.length && update((x) => x.type === "thread" && (x.face = refs[refs.length - 1] as FaceRef))}
                />
                <ThreadLinkBox
                  store={props.store}
                  ui={props.ui}
                  feature={f().id}
                  value={k().link}
                  want={(() => {
                    // Un agujero se coordina con un macho y un eje con una hembra
                    const own = props.store.result()?.threads?.find((t) => t.feature === f().id);
                    return own ? (own.internal ? "external" : "internal") : "any";
                  })()}
                  what={tl("la rosca")}
                  onChange={(link, src) =>
                    update((x) => {
                      if (x.type !== "thread") return;
                      x.link = link;
                      if (src) {
                        x.pitch = src.spec.pitch;
                        x.left = src.spec.left;
                      }
                    })
                  }
                />
                <Show when={!k().link}>
                <Row label={tl("Medida")}>
                  <Select
                    options={[{ value: "", label: tl("Paso a medida") }, ...METRIC_HOLES.map((m) => ({ value: String(m.pitch), label: `${m.size} × ${String(m.pitch).replace(".", ",")}` }))]}
                    value={METRIC_HOLES.some((m) => m.pitch === k().pitch) ? String(k().pitch) : ""}
                    onChange={(v) => v && update((x) => x.type === "thread" && (x.pitch = +v))}
                  />
                </Row>
                {field(tl("Paso"), "kind.pitch", k().pitch, (x, v) => x.type === "thread" && (x.pitch = v), "mm")}
                </Show>
                {field(tl("Largo"), "kind.length", k().length, (x, v) => x.type === "thread" && (x.length = v), "mm")}
                {field(tl("Holgura"), "kind.clearance", k().clearance, (x, v) => x.type === "thread" && (x.clearance = v), "mm")}
                <Checkbox small label={tl("Desde el otro extremo")} checked={k().flip} onChange={(c) => update((x) => x.type === "thread" && (x.flip = c))} />
                <Show when={!k().link}>
                  <Checkbox small label={tl("A izquierdas")} checked={k().left} onChange={(c) => update((x) => x.type === "thread" && (x.left = c))} />
                </Show>
                <p class="text-[11px] text-text-dim">
                  {tl("En un eje, el cilindro es el diámetro nominal (Ø6 para M6); en un agujero, el diámetro menor (5 mm para M6). Largo 0: toda la cara. La holgura achica la rosca de un eje y agranda la de un agujero (0,2–0,4 mm para imprimir en FDM).")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "rib" && (f().kind as Extract<FeatureKind, { type: "rib" }>)}>
            {(k) => (
              <>
                <Row label={tl("Sketch")}>
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "rib" && (x.sketch = +v))} />
                </Row>
                {field(tl("Espesor"), "kind.thickness", k().thickness, (x, v) => x.type === "rib" && (x.thickness = v), "mm")}
                <Checkbox small label={tl("Hacia el otro lado")} checked={k().flip} onChange={(c) => update((x) => x.type === "rib" && (x.flip = c))} />
                <Checkbox
                  small
                  label={tl("Hasta el escaneo")}
                  checked={!!k().scan}
                  onChange={(c) => update((x) => x.type === "rib" && (x.scan = c ? { offset: 0 } : null))}
                />
                <Show when={k().scan}>
                  {field(tl("Separación"), "kind.scan.offset", k().scan?.offset ?? 0, (x, v) => x.type === "rib" && !!x.scan && (x.scan.offset = v), "mm")}
                </Show>
                <p class="text-[11px] text-text-dim">
                  {k().scan
                    ? tl("Cada línea del sketch se lleva hasta la piel del escaneo, en su plano, con el espesor repartido a los dos lados.")
                    : tl("Cada línea del sketch se lleva hasta el sólido, en su plano, con el espesor repartido a los dos lados. El sketch tiene que estar en un plano que corte el sólido.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "replace_face" && (f().kind as Extract<FeatureKind, { type: "replace_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras planas")}
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "replace_face" && (x.faces = refs as FaceRef[]))}
                />
                <PlaneField
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:hasta`}
                  label={tl("Hasta")}
                  value={k().target}
                  except={f().id}
                  lost={lost("target").length > 0}
                  onChange={(p) => update((x) => x.type === "replace_face" && (x.target = p))}
                />
                <p class="text-[11px] text-text-dim">{tl("Las caras llegan hasta el plano: suma material si queda afuera, resta si queda adentro.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "face_to_scan" && (f().kind as Extract<FeatureKind, { type: "face_to_scan" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras planas")}
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "face_to_scan" && (x.faces = refs as FaceRef[]))}
                />
                {field(tl("Separación"), "kind.offset", k().offset, (x, v) => x.type === "face_to_scan" && (x.offset = v), "mm")}
                <p class="text-[11px] text-text-dim">
                  {tl("Las caras llegan hasta la piel del escaneo, aunque sea curva, y quedan a esa distancia: suma material si el escaneo está afuera, resta si está adentro.")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "scale" && (f().kind as Extract<FeatureKind, { type: "scale" }>)}>
            {(k) => {
              const uniform = () => k().factor[0] === k().factor[1] && k().factor[1] === k().factor[2];
              const [perAxis, setPerAxis] = createSignal(!uniform());
              return (
                <>
                  <Checkbox small label={tl("Distinta en cada eje")} checked={perAxis()} onChange={(c) => {
                    setPerAxis(c);
                    if (!c) update((x) => x.type === "scale" && (x.factor = [x.factor[0], x.factor[0], x.factor[0]]));
                  }} />
                  <Show
                    when={perAxis()}
                    fallback={field(tl("Factor"), "kind.factor.0", k().factor[0], (x, v) => x.type === "scale" && (x.factor = [v, v, v]))}
                  >
                    <div class="grid grid-cols-3 gap-1">
                      <For each={["X", "Y", "Z"]}>
                        {(axis, i) => field(axis, `kind.factor.${i()}`, k().factor[i()], (x, v) => x.type === "scale" && (x.factor[i()] = v))}
                      </For>
                    </div>
                  </Show>
                  <PointField store={props.store} ui={props.ui} owner={`${f().id}:centro`} label={tl("Centro")} value={k().center} except={f().id} onChange={(p) => update((x) => x.type === "scale" && (x.center = p))} />
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "surface_extrude" || f().kind.type === "surface_revolve") && f().kind}>
            {(kk) => {
              type SurfaceKind = Extract<FeatureKind, { type: "surface_extrude" | "surface_revolve" }>;
              const k = () => kk() as SurfaceKind;
              const sketch = () => props.sketches.find((s) => s.id === k().sketch);
              const curves = () => {
                const s = sketch();
                return s?.kind.type === "sketch"
                  ? s.kind.sketch.entities.filter((e) => !e.construction && ["line", "arc", "spline", "circle", "ellipse"].includes(e.geometry.type))
                  : [];
              };
              const NAMES: Record<string, string> = { line: tl("Línea"), arc: tl("Arco"), spline: tl("Spline"), circle: tl("Círculo"), ellipse: tl("Elipse") };
              const upd = (m: (x: SurfaceKind) => void) => update((x) => (x.type === "surface_extrude" || x.type === "surface_revolve") && m(x));
              const ext = () => (k().type === "surface_extrude" ? (k() as Extract<FeatureKind, { type: "surface_extrude" }>) : undefined);
              const rev = () => (k().type === "surface_revolve" ? (k() as Extract<FeatureKind, { type: "surface_revolve" }>) : undefined);
              const allLines = () => {
                const s = sketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "line") : [];
              };
              return (
                <>
                  <Row label={tl("Sketch")}>
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => upd((x) => ((x.sketch = +v), (x.entities = [])))} />
                  </Row>
                  <div class="space-y-1" aria-label={tl("Curvas")}>
                    <span class="text-xs text-text-muted">{tl("Curvas (sin elegir: todas)")}</span>
                    <For each={curves()} fallback={<p class="text-[11px] text-text-dim">{tl("Ese sketch no tiene curvas")}</p>}>
                      {(e) => (
                        <Checkbox
                          small
                          label={`${NAMES[e.geometry.type]} ${e.id}`}
                          checked={k().entities.includes(e.id)}
                          onChange={(c) => upd((x) => (x.entities = c ? [...x.entities, e.id] : x.entities.filter((i) => i !== e.id)))}
                        />
                      )}
                    </For>
                  </div>
                  <Show when={ext()}>
                    {(e) => (
                      <>
                        <Row label={tl("Hasta")}>
                          <Select
                            options={[
                              { value: "blind", label: tl("Distancia") },
                              { value: "symmetric", label: tl("Simétrica") },
                              { value: "two_sides", label: tl("Dos direcciones") },
                            ]}
                            value={e().extent.type}
                            onChange={(v) =>
                              update((x) => {
                                if (x.type !== "surface_extrude" || v === x.extent.type) return;
                                const d = "distance" in x.extent ? x.extent.distance : 10;
                                x.extent = v === "two_sides" ? { type: "two_sides", distance: d, second: d / 2 } : { type: v as "blind" | "symmetric", distance: d };
                              })
                            }
                          />
                        </Row>
                        {field(
                          tl("Distancia"),
                          "kind.extent.distance",
                          (e().extent as { distance: number }).distance,
                          (x, v) => x.type === "surface_extrude" && "distance" in x.extent && (x.extent.distance = v),
                          "mm",
                        )}
                        <Show when={e().extent.type === "two_sides"}>
                          {field(
                            tl("Hacia atrás"),
                            "kind.extent.second",
                            (e().extent as { second: number }).second,
                            (x, v) => x.type === "surface_extrude" && x.extent.type === "two_sides" && (x.extent.second = v),
                            "mm",
                          )}
                        </Show>
                        <Checkbox small label={tl("Hacia el otro lado")} checked={e().reverse} onChange={(c) => update((x) => x.type === "surface_extrude" && (x.reverse = c))} />
                      </>
                    )}
                  </Show>
                  <Show when={rev()}>
                    {(r) => (
                      <>
                        <Row label={tl("Eje")}>
                          <Select
                            options={[
                              ...(r().axis.type === "edge" ? [{ value: "edge", label: tl("Arista del sólido") }] : []),
                              { value: "x", label: "X" },
                              { value: "y", label: "Y" },
                              { value: "z", label: "Z" },
                              ...allLines().map((l) => ({ value: `line:${l.id}`, label: l.construction ? tl("Línea {id} (construcción)", { id: l.id }) : tl("Línea {id}", { id: l.id }) })),
                              ...refOptions(props.store, "axis"),
                            ]}
                            value={(() => {
                              const a = r().axis;
                              return a.type === "sketch_line" ? `line:${a.line}` : (refValue(a) ?? a.type);
                            })()}
                            onChange={(v) =>
                              update((x) => {
                                if (x.type !== "surface_revolve") return;
                                x.axis = v.startsWith("line:")
                                  ? { type: "sketch_line", sketch: x.sketch, line: +v.slice(5) }
                                  : isRefValue(v)
                                    ? axisOfValue(v)
                                    : { type: v as "x" | "y" | "z" };
                              })
                            }
                          />
                        </Row>
                        {field(tl("Ángulo"), "kind.angle", r().angle, (x, v) => x.type === "surface_revolve" && (x.angle = v), "°")}
                      </>
                    )}
                  </Show>
                  <p class="text-[11px] text-text-dim">
                    {tl("Una superficie no tiene espesor: no se une a los sólidos ni la cortan sin elegirla. Se cierra con Relleno y Coser, o se le da espesor con Engrosar.")}
                  </p>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "fill" && (f().kind as Extract<FeatureKind, { type: "fill" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:borde`}
                  kind="edges"
                  label={tl("Aristas del borde")}
                  refs={k().edges}
                  lost={lost("edges")}
                  onChange={(refs) => update((x) => x.type === "fill" && (x.edges = refs as EdgeRef[]))}
                />
                <Checkbox
                  small
                  label={tl("Tangente a las caras vecinas")}
                  checked={k().tangent}
                  onChange={(c) => update((x) => x.type === "fill" && (x.tangent = c))}
                />
                <p class="text-[11px] text-text-dim">
                  {tl("El borde tiene que cerrarse. Plano y sin tangencia queda una cara plana; tangente, sigue la curvatura del borde de cada superficie (no sirve contra una pared perpendicular al relleno).")}
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sew" && (f().kind as Extract<FeatureKind, { type: "sew" }>)}>
            {(k) => (
              <>
                <PartChecklist
                  store={props.store}
                  ui={props.ui}
                  featureId={f().id}
                  label={tl("Superficies")}
                  empty={tl("Elegir las superficies")}
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "sew" && (x.parts = v))}
                />
                <Checkbox small label={tl("Hacer sólido si cierran")} checked={k().solid} onChange={(c) => update((x) => x.type === "sew" && (x.solid = c))} />
                {field(tl("Separación máxima"), "kind.tolerance", k().tolerance, (x, v) => x.type === "sew" && (x.tolerance = v), "mm")}
                <p class="text-[11px] text-text-dim">{tl("Une los bordes que están a menos de la separación máxima. Las piezas cosidas pasan a ser una.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "thicken" && (f().kind as Extract<FeatureKind, { type: "thicken" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label={tl("Caras")}
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "thicken" && (x.faces = refs as FaceRef[]))}
                />
                {field(tl("Espesor"), "kind.thickness", k().thickness, (x, v) => x.type === "thicken" && (x.thickness = v), "mm")}
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "thicken" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sweep" && (f().kind as Extract<FeatureKind, { type: "sweep" }>)}>
            {(k) => {
              const pathSketch = () => (k().path.type === "sketch" ? props.sketches.find((s) => s.id === (k().path as { sketch: number }).sketch) : undefined);
              const curves = () => {
                const s = pathSketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => !e.construction && ["line", "arc", "spline"].includes(e.geometry.type)) : [];
              };
              const NAMES: Record<string, string> = { line: tl("Línea"), arc: tl("Arco"), spline: tl("Spline") };
              return (
                <>
                  <Row label={tl("Tipo de perfil")}>
                    <Select
                      options={[
                        { value: "sketch", label: tl("Regiones de un sketch") },
                        { value: "circle", label: tl("Círculo (tubo)") },
                      ]}
                      value={k().circle != null ? "circle" : "sketch"}
                      onChange={(v) => update((x) => x.type === "sweep" && (x.circle = v === "circle" ? 2 : null))}
                    />
                  </Row>
                  <Show
                    when={k().circle == null}
                    fallback={field(tl("Radio del tubo"), "kind.circle", k().circle ?? 2, (x, v) => x.type === "sweep" && (x.circle = v), "mm")}
                  >
                    <Row label={tl("Perfil")}>
                      <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "sweep" && (x.sketch = +v))} />
                    </Row>
                    <RegionBox
                      ui={props.ui}
                      store={props.store}
                      owner={`${f().id}:regiones`}
                      lost={lost("regions")}
                      sketch={k().sketch}
                      value={k().regions}
                      onChange={(r) => update((x) => x.type === "sweep" && (x.regions = r))}
                    />
                  </Show>
                  <Row label={tl("Camino")}>
                    <Select
                      options={[...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                      value={(() => {
                        const p = k().path;
                        return p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                      })()}
                      onChange={(v) =>
                        update(
                          (x) =>
                            x.type === "sweep" &&
                            (x.path = v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                        )
                      }
                    />
                  </Row>
                  <Show when={k().path.type === "sketch"}>
                  <div class="space-y-1" aria-label={tl("Tramos del camino")}>
                    <span class="text-xs text-text-muted">{tl("Tramos (sin elegir: todos)")}</span>
                    <For each={curves()} fallback={<p class="text-[11px] text-text-dim">{tl("Ese sketch no tiene líneas, arcos ni splines")}</p>}>
                      {(e) => (
                        <Checkbox
                          small
                          label={`${NAMES[e.geometry.type]} ${e.id}`}
                          checked={k().path.type === "sketch" && (k().path as { entities: number[] }).entities.includes(e.id)}
                          onChange={(c) =>
                            update((x) => {
                              if (x.type !== "sweep" || x.path.type !== "sketch") return;
                              x.path.entities = c ? [...x.path.entities, e.id] : x.path.entities.filter((i) => i !== e.id);
                            })
                          }
                        />
                      )}
                    </For>
                  </div>
                  </Show>
                  <Row label={tl("Orientación")}>
                    <Select options={ORIENT_OPTIONS} value={k().orient ?? "follow"} onChange={(v) => update((x) => x.type === "sweep" && (x.orient = v === "follow" ? undefined : (v as SweepOrient)))} />
                  </Row>
                  <Row label={tl("Curva guía")}>
                    <Select
                      options={[{ value: "", label: tl("Ninguna") }, ...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                      value={(() => {
                        const g = k().guide;
                        return !g ? "" : g.type === "sketch" ? String(g.sketch) : `curve:${g.feature}`;
                      })()}
                      onChange={(v) =>
                        update(
                          (x) =>
                            x.type === "sweep" &&
                            (x.guide = !v ? null : v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                        )
                      }
                    />
                  </Row>
                  <Show when={!k().guide}>
                    {field(tl("Torsión"), "kind.twist", k().twist ?? 0, (x, v) => x.type === "sweep" && (x.twist = v === 0 ? null : v), "°")}
                  </Show>
                  <Checkbox
                    small
                    label={tl("Delgado (solo una pared)")}
                    checked={k().thin != null}
                    onChange={(c) => update((x) => x.type === "sweep" && (x.thin = c ? 1 : null))}
                  />
                  <Show when={k().thin != null}>
                    {field(tl("Espesor"), "kind.thin", k().thin ?? 1, (x, v) => x.type === "sweep" && (x.thin = v), "mm")}
                  </Show>
                  <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "sweep" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "surface_sweep" && (f().kind as Extract<FeatureKind, { type: "surface_sweep" }>)}>
            {(k) => (
              <>
                <Row label={tl("Curvas de")}>
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "surface_sweep" && ((x.sketch = +v), (x.entities = [])))} />
                </Row>
                <Row label={tl("Camino")}>
                  <Select
                    options={[...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                    value={(() => {
                      const p = k().path;
                      return p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                    })()}
                    onChange={(v) =>
                      update(
                        (x) =>
                          x.type === "surface_sweep" &&
                          (x.path = v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                      )
                    }
                  />
                </Row>
                <Row label={tl("Orientación")}>
                  <Select
                    options={ORIENT_OPTIONS}
                    value={k().orient ?? "follow"}
                    onChange={(v) => update((x) => x.type === "surface_sweep" && (x.orient = v === "follow" ? undefined : (v as SweepOrient)))}
                  />
                </Row>
                <p class="text-[11px] text-text-dim">{tl("Cada curva del sketch (abierta o cerrada) da una superficie sin espesor.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "surface_loft" && (f().kind as Extract<FeatureKind, { type: "surface_loft" }>)}>
            {(k) => (
              <>
                <For each={k().sections}>
                  {(sec, i) => (
                    <Row label={tl("Sección {v1}", { v1: i() + 1 })}>
                      <Select
                        options={sketchOptions()}
                        value={String(sec.sketch)}
                        onChange={(v) => update((x) => x.type === "surface_loft" && (x.sections[i()] = { sketch: +v, entities: [] }))}
                      />
                    </Row>
                  )}
                </For>
                <Button
                  size="sm"
                  fullWidth
                  onClick={() =>
                    update((x) => {
                      if (x.type !== "surface_loft") return;
                      const used = new Set(x.sections.map((s) => s.sketch));
                      const next = props.sketches.find((s) => !used.has(s.id));
                      if (next) x.sections.push({ sketch: next.id, entities: [] });
                    })
                  }
                >
                  {tl("Agregar sección")}
                </Button>
                <Checkbox small label={tl("Caras planas entre secciones")} checked={k().ruled} onChange={(c) => update((x) => x.type === "surface_loft" && (x.ruled = c))} />
                <p class="text-[11px] text-text-dim">{tl("Cada sección es una cadena de curvas (abierta o cerrada) de un sketch.")}</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "loft" && (f().kind as Extract<FeatureKind, { type: "loft" }>)}>
            {(k) => (
              <>
                <For each={k().sections}>
                  {(sec, i) => (
                    <div class="flex items-end gap-1">
                      <div class="flex-1">
                        <Row label={tl("Sección {v1}", { v1: i() + 1 })}>
                          <Select
                            options={sketchOptions()}
                            value={String(sec.sketch)}
                            onChange={(v) => update((x) => x.type === "loft" && (x.sections[i()] = { sketch: +v, regions: { type: "all" } }))}
                          />
                        </Row>
                      </div>
                      <IconButton
                        aria-label={tl("Quitar la sección {v1}", { v1: i() + 1 })}
                        size="sm"
                        variant="ghost"
                        disabled={k().sections.length <= 2}
                        onClick={() => update((x) => x.type === "loft" && x.sections.splice(i(), 1))}
                      >
                        <Icons.X size={10} />
                      </IconButton>
                    </div>
                  )}
                </For>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    update((x) => {
                      if (x.type !== "loft") return;
                      const used = new Set(x.sections.map((s) => s.sketch));
                      const next = props.sketches.find((s) => !used.has(s.id));
                      if (next) x.sections.push({ sketch: next.id, regions: { type: "all" } });
                    })
                  }
                >
                  {tl("Agregar sección")}
                </Button>
                <Checkbox small label={tl("Caras planas entre secciones")} checked={k().ruled} onChange={(c) => update((x) => x.type === "loft" && (x.ruled = c))} />
                <Checkbox
                  small
                  label={tl("Empieza en un punto")}
                  checked={!!k().start_point}
                  onChange={(c) => update((x) => x.type === "loft" && (x.start_point = c ? { type: "at", point: [0, 0, -10] } : null))}
                />
                <Show when={k().start_point}>
                  {(p) => <PointField store={props.store} ui={props.ui} owner={`${f().id}:desde`} label={tl("Punta inicial")} value={p()} except={f().id} onChange={(q) => update((x) => x.type === "loft" && (x.start_point = q))} />}
                </Show>
                <Checkbox
                  small
                  label={tl("Termina en un punto")}
                  checked={!!k().end_point}
                  onChange={(c) => update((x) => x.type === "loft" && (x.end_point = c ? { type: "at", point: [0, 0, 20] } : null))}
                />
                <Show when={k().end_point}>
                  {(p) => <PointField store={props.store} ui={props.ui} owner={`${f().id}:hasta`} label={tl("Punta final")} value={p()} except={f().id} onChange={(q) => update((x) => x.type === "loft" && (x.end_point = q))} />}
                </Show>
                <Checkbox small label={tl("Cerrada (vuelve a la primera)")} checked={!!k().closed} onChange={(c) => update((x) => x.type === "loft" && (x.closed = c || undefined))} />
                <Checkbox
                  small
                  label={tl("Sale perpendicular a la primera sección")}
                  checked={k().start_tangent != null}
                  onChange={(c) => update((x) => x.type === "loft" && ((x.start_tangent = c ? 1 : null), c && (x.ruled = false)))}
                />
                <Show when={k().start_tangent != null}>
                  {field(tl("Peso al comienzo"), "kind.start_tangent", k().start_tangent ?? 1, (x, v) => x.type === "loft" && (x.start_tangent = v))}
                </Show>
                <Checkbox
                  small
                  label={tl("Llega perpendicular a la última sección")}
                  checked={k().end_tangent != null}
                  onChange={(c) => update((x) => x.type === "loft" && ((x.end_tangent = c ? 1 : null), c && (x.ruled = false)))}
                />
                <Show when={k().end_tangent != null}>
                  {field(tl("Peso al final"), "kind.end_tangent", k().end_tangent ?? 1, (x, v) => x.type === "loft" && (x.end_tangent = v))}
                </Show>
                <Checkbox small label={tl("Delgada (solo una pared)")} checked={k().thin != null} onChange={(c) => update((x) => x.type === "loft" && (x.thin = c ? 1 : null))} />
                <Show when={k().thin != null}>
                  {field(tl("Espesor"), "kind.thin", k().thin ?? 1, (x, v) => x.type === "loft" && (x.thin = v), "mm")}
                </Show>
                <Row label={tl("Línea central")}>
                  <Select
                    options={[{ value: "", label: tl("Ninguna") }, ...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                    value={(() => {
                      const p = k().path;
                      return !p ? "" : p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                    })()}
                    onChange={(v) =>
                      update(
                        (x) =>
                          x.type === "loft" &&
                          (x.path = !v ? null : v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                      )
                    }
                  />
                </Row>
                <Show when={k().path}>
                  <Row label={tl("Curva guía")}>
                    <Select
                      options={[{ value: "", label: tl("Ninguna") }, ...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                      value={(() => {
                        const p = k().guide;
                        return !p ? "" : p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                      })()}
                      onChange={(v) =>
                        update(
                          (x) =>
                            x.type === "loft" &&
                            (x.guide = !v ? null : v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                        )
                      }
                    />
                  </Row>
                </Show>
                <Row label={tl("Con el sólido")}>{opSelect(k().op, (o) => update((x) => x.type === "loft" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={isScanReference(f().kind)}>
            <p class="text-[11px] text-text-dim leading-relaxed">
              {tl("Tomado del escaneo: sigue al escaneo si se alinea, se mueve o se repara. Para cambiar de dónde sale, borrarlo y volver a tomarlo en la pestaña «Desde el escaneo».")}
            </p>
          </Match>
          <Match when={f().kind.type === "plane" && (f().kind as Extract<FeatureKind, { type: "plane" }>)}>
            {(k) => {
              const def = () => k().def;
              const setDef = (d: PlaneDef) => update((x) => x.type === "plane" && (x.def = d));
              const base = (): PlaneSpec => (def().type === "offset" || def().type === "angle" ? (def() as { base: PlaneSpec }).base : { type: "xy" });
              return (
                <>
                  <Row label={tl("Tipo")}>
                    <Select
                      options={[
                        { value: "offset", label: tl("Desplazado") },
                        { value: "angle", label: tl("En ángulo") },
                        { value: "midplane", label: tl("Plano medio") },
                        { value: "three_points", label: tl("Por tres puntos") },
                        { value: "picked", label: tl("Por clics (se deduce)") },
                      ]}
                      value={def().type}
                      onChange={(v) => {
                        if (v === def().type) return;
                        if (v === "picked") setDef({ type: "picked", refs: [] });
                        else if (v === "offset") setDef({ type: "offset", base: base(), distance: 10 });
                        else if (v === "angle") setDef({ type: "angle", base: base(), axis: { type: "x" }, angle: 45 });
                        else if (v === "midplane") setDef({ type: "midplane", a: base(), b: { type: "xy" } });
                        else
                          setDef({
                            type: "three_points",
                            points: [
                              { type: "at", point: [0, 0, 0] },
                              { type: "at", point: [10, 0, 0] },
                              { type: "at", point: [0, 10, 0] },
                            ],
                          });
                      }}
                    />
                  </Row>
                  <Switch>
                    <Match when={def().type === "picked" && (def() as Extract<PlaneDef, { type: "picked" }>)}>
                      {(d) => (
                        <>
                          <RefPicksBox
                            store={props.store}
                            ui={props.ui}
                            owner={`${f().id}:refs`}
                            feature={f().id}
                            what="plane"
                            refs={d().refs}
                            lost={lost("refs" as MissingRef["field"])}
                            onChange={(refs) => setDef({ ...d(), refs })}
                          />
                          {field(tl("Distancia"), "kind.def.offset", d().offset ?? 0, (x, v) => x.type === "plane" && x.def.type === "picked" && (x.def.offset = v || undefined), "mm")}
                          {field(tl("Ángulo"), "kind.def.angle", d().angle ?? 0, (x, v) => x.type === "plane" && x.def.type === "picked" && (x.def.angle = v || undefined), "°")}
                          <Checkbox small label={tl("Dar vuelta la normal")} checked={!!d().flip} onChange={(c) => setDef({ ...d(), flip: c || undefined })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "offset" && (def() as Extract<PlaneDef, { type: "offset" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:base`} label={tl("Desde")} value={d().base} except={f().id} lost={lost("base").length > 0} onChange={(p) => setDef({ ...d(), base: p })} />
                          {field(tl("Distancia"), "kind.def.distance", d().distance, (x, v) => x.type === "plane" && x.def.type === "offset" && (x.def.distance = v), "mm")}
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "angle" && (def() as Extract<PlaneDef, { type: "angle" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:base`} label={tl("Desde")} value={d().base} except={f().id} lost={lost("base").length > 0} onChange={(p) => setDef({ ...d(), base: p })} />
                          <AxisField store={props.store} ui={props.ui} owner={`${f().id}:eje`} label={tl("Alrededor de")} value={d().axis} except={f().id} onChange={(a) => setDef({ ...d(), axis: a })} />
                          {field(tl("Ángulo"), "kind.def.angle", d().angle, (x, v) => x.type === "plane" && x.def.type === "angle" && (x.def.angle = v), "°")}
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "midplane" && (def() as Extract<PlaneDef, { type: "midplane" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:a`} label={tl("Entre")} value={d().a} except={f().id} lost={lost("a").length > 0} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:b`} label="Y" value={d().b} except={f().id} lost={lost("b").length > 0} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "three_points" && (def() as Extract<PlaneDef, { type: "three_points" }>)}>
                      {(d) => (
                        <For each={[0, 1, 2] as const}>
                          {(k) => (
                            <PointField
                              store={props.store}
                              ui={props.ui}
                              owner={`${f().id}:p${k}`}
                              label={tl("Punto {v1}", { v1: k + 1 })}
                              value={d().points[k]}
                              except={f().id}
                              onChange={(p) => setDef({ ...d(), points: d().points.map((q, i) => (i === k ? p : q)) as [PointSpec, PointSpec, PointSpec] })}
                            />
                          )}
                        </For>
                      )}
                    </Match>
                  </Switch>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "axis" && (f().kind as Extract<FeatureKind, { type: "axis" }>)}>
            {(k) => {
              const def = () => k().def;
              const setDef = (d: AxisDef) => update((x) => x.type === "axis" && (x.def = d));
              return (
                <>
                  <Row label={tl("Tipo")}>
                    <Select
                      options={[
                        { value: "two_points", label: tl("Por dos puntos") },
                        { value: "edge", label: tl("Arista") },
                        { value: "face", label: tl("Eje de un cilindro") },
                        { value: "planes", label: tl("Cruce de dos planos") },
                        { value: "picked", label: tl("Por clics (se deduce)") },
                      ]}
                      value={def().type}
                      onChange={(v) => {
                        if (v === def().type) return;
                        if (v === "picked") setDef({ type: "picked", refs: [] });
                        else if (v === "two_points") setDef({ type: "two_points", a: { type: "at", point: [0, 0, 0] }, b: { type: "at", point: [0, 0, 10] } });
                        else if (v === "planes") setDef({ type: "planes", a: { type: "xz" }, b: { type: "yz" } });
                        else if (v === "edge") setDef({ type: "edge", edge: undefined as unknown as EdgeRef });
                        else setDef({ type: "face", face: undefined as unknown as FaceRef });
                      }}
                    />
                  </Row>
                  <Switch>
                    <Match when={def().type === "picked" && (def() as Extract<AxisDef, { type: "picked" }>)}>
                      {(d) => (
                        <RefPicksBox
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:refs`}
                          feature={f().id}
                          what="axis"
                          refs={d().refs}
                          lost={lost("refs" as MissingRef["field"])}
                          onChange={(refs) => setDef({ type: "picked", refs })}
                        />
                      )}
                    </Match>
                    <Match when={def().type === "two_points" && (def() as Extract<AxisDef, { type: "two_points" }>)}>
                      {(d) => (
                        <>
                          <PointField store={props.store} ui={props.ui} owner={`${f().id}:a`} label={tl("Desde")} value={d().a} except={f().id} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PointField store={props.store} ui={props.ui} owner={`${f().id}:b`} label={tl("Hasta")} value={d().b} except={f().id} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "edge" && (def() as Extract<AxisDef, { type: "edge" }>)}>
                      {(d) => (
                        <SelectionBox
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:arista`}
                          kind="edges"
                          label={tl("Arista")}
                          refs={d().edge ? [d().edge] : []}
                          lost={lost("edge")}
                          onChange={(refs) => refs.length && setDef({ type: "edge", edge: refs[refs.length - 1] as EdgeRef })}
                        />
                      )}
                    </Match>
                    <Match when={def().type === "face" && (def() as Extract<AxisDef, { type: "face" }>)}>
                      {(d) => (
                        <SelectionBox
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:cara`}
                          kind="face"
                          label={tl("Cara cilíndrica o cónica")}
                          refs={d().face ? [d().face] : []}
                          lost={lost("face")}
                          onChange={(refs) => refs[0] && setDef({ type: "face", face: refs[0] as FaceRef })}
                        />
                      )}
                    </Match>
                    <Match when={def().type === "planes" && (def() as Extract<AxisDef, { type: "planes" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:a`} label={tl("Plano")} value={d().a} except={f().id} lost={lost("a").length > 0} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:b`} label={tl("Con")} value={d().b} except={f().id} lost={lost("b").length > 0} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                  </Switch>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "point" && (f().kind as Extract<FeatureKind, { type: "point" }>)}>
            {(k) => (
              <>
                <Row label={tl("Tipo")}>
                  <Select
                    options={[
                      { value: "picked", label: tl("Por clics (se deduce)") },
                      { value: "other", label: tl("Coordenadas, arista o referencia") },
                    ]}
                    value={k().def.type === "picked" ? "picked" : "other"}
                    onChange={(v) =>
                      update((x) => {
                        if (x.type !== "point" || (v === "picked") === (x.def.type === "picked")) return;
                        x.def = v === "picked" ? { type: "picked", refs: [] } : { type: "at", point: [0, 0, 0] };
                      })
                    }
                  />
                </Row>
                <Show
                  when={k().def.type === "picked" && (k().def as Extract<PointSpec, { type: "picked" }>)}
                  fallback={
                    <PointField
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:punto`}
                      label={tl("Punto")}
                      value={k().def}
                      except={f().id}
                      onChange={(p) => update((x) => x.type === "point" && (x.def = p))}
                    />
                  }
                >
                  {(d) => (
                    <RefPicksBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:refs`}
                      feature={f().id}
                      what="point"
                      refs={d().refs}
                      lost={lost("refs" as MissingRef["field"])}
                      onChange={(refs) => update((x) => x.type === "point" && (x.def = { type: "picked", refs }))}
                    />
                  )}
                </Show>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "frame" && (f().kind as Extract<FeatureKind, { type: "frame" }>)}>
            {(k) => (
              <>
                <PointField
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:origen`}
                  label={tl("Origen")}
                  value={k().origin}
                  except={f().id}
                  onChange={(p) => update((x) => x.type === "frame" && (x.origin = p))}
                />
                <AxisField
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:z`}
                  label={tl("Eje Z")}
                  value={k().z}
                  except={f().id}
                  onChange={(a) => update((x) => x.type === "frame" && (x.z = a))}
                />
                <Checkbox
                  small
                  label={tl("Hacia dónde va X")}
                  checked={!!k().x}
                  onChange={(c) => update((x) => x.type === "frame" && (x.x = c ? { type: "x" } : undefined))}
                />
                <Show when={k().x}>
                  {(ax) => (
                    <AxisField
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:x`}
                      label={tl("Eje X")}
                      value={ax()}
                      except={f().id}
                      onChange={(a) => update((x) => x.type === "frame" && (x.x = a))}
                    />
                  )}
                </Show>
                <div class="flex gap-3">
                  <Checkbox small label={tl("Invertir Z")} checked={!!k().flip_z} onChange={(c) => update((x) => x.type === "frame" && (x.flip_z = c || undefined))} />
                  <Checkbox small label={tl("Invertir X")} checked={!!k().flip_x} onChange={(c) => update((x) => x.type === "frame" && (x.flip_x = c || undefined))} />
                </div>
                <p class="text-[11px] text-text-dim">
                  {tl("Sin X propio, X sale del X del mundo (o del Y si Z va a lo largo de X). Se usa como sus tres planos, sus tres ejes y su origen.")}
                </p>
              </>
            )}
          </Match>
        </Switch>
        {/* Unir, restar o intersecar: con qué piezas (sin elegir, las que toca; el agujero, la más cercana) */}
        <Show when={f().kind.type === "hole" || ("op" in f().kind && f().kind.type !== "boolean" && (f().kind as { op: BodyOp }).op !== "new")}>
          <PartChecklist
            store={props.store}
            ui={props.ui}
            featureId={f().id}
            label={f().kind.type === "hole" ? tl("En las piezas") : tl("Con las piezas")}
            empty={f().kind.type === "hole" ? tl("La más cercana") : tl("Las que toca")}
            value={f().scope ?? []}
            auto={state()?.auto_scope}
            missing={lost("scope")}
            onChange={(v) => void props.store.updateFeature(f().id, (x) => (x.scope = v.length ? v : undefined))}
          />
        </Show>
        <p class="text-[11px] text-text-dim">{planeLabelFor(f())}</p>
      </div>
    </Section>
  );
};

function planeLabelFor(f: Feature): string {
  return f.kind.type === "sketch" ? tl("Sobre: {v1}", { v1: planeLabel(f.kind.plane) }) : "";
}

// ─── Geometría de referencia en los diálogos ──────────────────────────────

/** Referencias calculadas de un tipo, con el nombre de su operación (sin `except`) */
function refOptions(store: CadStore, kind: "plane" | "axis" | "point" | "curve", except?: number): { value: string; label: string }[] {
  const names = new Map((store.doc()?.features ?? []).map((f) => [f.id, f.name]));
  return (store.result()?.references ?? []).flatMap((r) => {
    if (r.id === except) return [];
    const name = names.get(r.id) ?? tl("Referencia {id}", { id: r.id });
    // Un sistema de coordenadas da sus tres planos, sus tres ejes y su origen
    if (r.kind === "frame") {
      if (kind === "plane") return FRAME_PLANES.map(([n, label]) => ({ value: `frame:${r.id}:${n}`, label: `${name} · ${label}` }));
      if (kind === "axis") return (["x", "y", "z"] as const).map((a) => ({ value: `frame:${r.id}:${a}`, label: `${name} · ${a.toUpperCase()}` }));
      if (kind === "point") return [{ value: `ref:${r.id}`, label: `${name} · origen` }];
      return [];
    }
    return r.kind === kind || (kind === "curve" && r.kind === "curves") ? [{ value: `ref:${r.id}`, label: name }] : [];
  });
}

/** Planos de un sistema de coordenadas: el eje perpendicular y su nombre */
const FRAME_PLANES: [FrameAxis, string][] = [
  ["z", "planta XY"],
  ["y", "frente XZ"],
  ["x", "lateral YZ"],
];

/** Valor de un `Select` para un plano o eje de referencia o de un sistema de coordenadas */
function refValue(s: PlaneSpec | AxisSpec): string | undefined {
  if (s.type === "reference") return `ref:${s.feature}`;
  if (s.type === "frame") return `frame:${s.feature}:${"normal" in s ? s.normal : s.axis}`;
  return undefined;
}

const isRefValue = (v: string) => v.startsWith("ref:") || v.startsWith("frame:");

function planeOfValue(v: string): PlaneSpec {
  const [kind, id, which] = v.split(":");
  return kind === "frame" ? { type: "frame", feature: +id, normal: which as FrameAxis } : { type: "reference", feature: +id };
}

function axisOfValue(v: string): AxisSpec {
  const [kind, id, which] = v.split(":");
  return kind === "frame" ? { type: "frame", feature: +id, axis: which as FrameAxis } : { type: "reference", feature: +id };
}

/** Plano: base, de referencia o una cara plana elegida en el visor */
const PlaneField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: PlaneSpec; except?: number; lost?: boolean; onChange: (p: PlaneSpec) => void }> = (props) => {
  const [wantFace, setWantFace] = createSignal(false);
  const current = () => (wantFace() ? "face" : (refValue(props.value) ?? props.value.type));
  return (
    <>
      <Row label={props.label}>
        <Select
          options={[
            ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
            ...refOptions(props.store, "plane", props.except),
            { value: "face", label: props.lost ? tl("Cara del sólido · no encontrada") : tl("Cara del sólido") },
            ...(props.value.type === "custom" ? [{ value: "custom", label: tl("Plano propio") }] : []),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            setWantFace(v === "face");
            if (v === "xy" || v === "xz" || v === "yz") props.onChange({ type: v });
            else if (isRefValue(v)) props.onChange(planeOfValue(v));
          }}
        />
      </Row>
      <Show when={wantFace() || props.value.type === "face"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="face"
          label={`${props.label}: cara`}
          refs={props.value.type === "face" ? [props.value.face] : []}
          lost={props.lost ? [0] : []}
          onChange={(refs) => {
            if (refs[0]) props.onChange({ type: "face", face: refs[0] as FaceRef });
            setWantFace(false);
          }}
        />
      </Show>
    </>
  );
};

/** Eje: X/Y/Z, de referencia o una arista elegida */
const AxisField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: AxisSpec; except?: number; onChange: (a: AxisSpec) => void }> = (props) => {
  const [wantEdge, setWantEdge] = createSignal(false);
  const current = () => (wantEdge() ? "edge" : (refValue(props.value) ?? props.value.type));
  return (
    <>
      <Row label={props.label}>
        <Select
          options={[
            ...(["x", "y", "z"] as const).map((k) => ({ value: k, label: k.toUpperCase() })),
            ...refOptions(props.store, "axis", props.except),
            { value: "edge", label: tl("Arista del sólido") },
            ...(props.value.type === "sketch_line" ? [{ value: "sketch_line", label: tl("Línea del sketch") }] : []),
            ...(props.value.type === "custom" ? [{ value: "custom", label: tl("Eje propio") }] : []),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            setWantEdge(v === "edge");
            if (v === "x" || v === "y" || v === "z") props.onChange({ type: v });
            else if (isRefValue(v)) props.onChange(axisOfValue(v));
          }}
        />
      </Row>
      <Show when={wantEdge() || props.value.type === "edge"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="edges"
          label={`${props.label}: arista`}
          refs={props.value.type === "edge" ? [props.value.edge] : []}
          onChange={(refs) => {
            const last = refs[refs.length - 1];
            if (last) props.onChange({ type: "edge", edge: last as EdgeRef });
            setWantEdge(false);
          }}
        />
      </Show>
    </>
  );
};

/** Punto: coordenadas, centro o punto de una arista, o de referencia */
const PointField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: PointSpec; except?: number; onChange: (p: PointSpec) => void }> = (props) => {
  const current = () => (props.value.type === "reference" ? `ref:${props.value.feature}` : props.value.type);
  const edgeOf = () => (props.value.type === "center" || props.value.type === "on_edge" ? [props.value.edge] : []);
  return (
    <div class="space-y-1" data-point-field={props.label}>
      <Row label={props.label}>
        <Select
          options={[
            { value: "at", label: tl("Coordenadas") },
            { value: "center", label: tl("Centro de una arista") },
            { value: "on_edge", label: tl("Sobre una arista recta") },
            ...refOptions(props.store, "point", props.except),
            // Elegidos en el visor (no se arman desde la lista)
            ...(props.value.type === "edge_end" ? [{ value: "edge_end", label: tl("Vértice del sólido") }] : []),
            ...(props.value.type === "picked" ? [{ value: "picked", label: tl("Por clics") }] : []),
            ...(props.value.type === "scan" ? [{ value: "scan", label: tl("Del escaneo") }] : []),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            const edge = edgeOf()[0];
            if (v === "at") props.onChange({ type: "at", point: [0, 0, 0] });
            else if (v.startsWith("ref:")) props.onChange({ type: "reference", feature: +v.slice(4) });
            else if (edge) props.onChange(v === "center" ? { type: "center", edge } : { type: "on_edge", edge, at: 0.5 });
            else props.onChange(v === "center" ? { type: "center", edge: undefined as unknown as EdgeRef } : { type: "on_edge", edge: undefined as unknown as EdgeRef, at: 0.5 });
          }}
        />
      </Row>
      <Show when={props.value.type === "at" && props.value}>
        {(p) => (
          <div class="grid grid-cols-3 gap-1">
            <For each={["X", "Y", "Z"]}>
              {(axis, i) => (
                <Num
                  label={axis}
                  value={p().point[i()]}
                  onCommit={(v) => props.onChange({ type: "at", point: p().point.map((c, k) => (k === i() ? v : c)) as P3 })}
                />
              )}
            </For>
          </div>
        )}
      </Show>
      <Show when={props.value.type === "center" || props.value.type === "on_edge"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="edges"
          label={`${props.label}: arista`}
          refs={edgeOf().filter(Boolean)}
          onChange={(refs) => {
            const edge = refs[refs.length - 1] as EdgeRef | undefined;
            if (!edge) return;
            props.onChange(props.value.type === "on_edge" ? { ...props.value, edge } : { type: "center", edge });
          }}
        />
      </Show>
      <Show when={props.value.type === "on_edge" && props.value}>
        {(p) => <Num label={tl("Fracción del largo")} step={0.05} value={p().at} onCommit={(v) => props.onChange({ ...p(), at: Math.min(1, Math.max(0, v)) })} />}
      </Show>
    </div>
  );
};

// ─── Piezas en los diálogos ───────────────────────────────────────────────

/** Lista de piezas que hay antes de la operación para tildar; las perdidas en rojo */
/**
 * Con qué piezas une o resta una operación: se eligen con clic en el visor
 * (otra vez la quita), y la lista muestra solo las elegidas (con muchas piezas,
 * una lista de casillas no sirve). Sin elegir, las que toca.
 */
/** Copias de un patrón que no se hacen, escritas como lista ("2, 5") */
const SkipCopies: Component<{ value: number[]; onChange: (v: number[]) => void }> = (props) => (
  <>
    <label class="flex items-center gap-2 text-xs">
      <span class="text-text-muted shrink-0 w-24">{tl("Omitir copias")}</span>
      <input
        type="text"
        aria-label={tl("Omitir copias")}
        placeholder={tl("ninguna")}
        value={props.value.join(", ")}
        class="w-full min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
        onChange={(e) => {
          const nums = e.currentTarget.value
            .split(/[\s,;]+/)
            .map(Number)
            .filter((n) => Number.isInteger(n) && n >= 1);
          props.onChange([...new Set(nums)].sort((a, b) => a - b));
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          e.stopPropagation();
        }}
      />
    </label>
    <p class="text-[11px] text-text-dim">{tl("Números de las copias que no se hacen (1 es la primera; en dos direcciones se cuentan por filas).")}</p>
  </>
);

/** Lo que trae una pieza derivada: de qué pestaña, qué piezas (ninguna marcada = todas) y en qué configuración */
const DerivedBox: Component<{
  store: CadStore;
  kind: Extract<FeatureKind, { type: "derived" }>;
  /** Posiciones de `kind.parts` que el original ya no tiene */
  missing: number[];
  update: (mutate: (k: FeatureKind) => void) => void;
}> = (props) => {
  const sources = () => props.store.tabs().tabs.filter((x) => x.kind === "design" && x.id !== props.store.tabs().active);
  const [labels, setLabels] = createSignal<{ id: PartId; name: string }[]>([]);
  const [configs, setConfigs] = createSignal<string[]>([]);
  let seq = 0;
  createEffect(() => {
    const id = props.kind.studio;
    // Se releen al cambiar de pestaña de origen o las pestañas
    props.store.tabs();
    const n = ++seq;
    invoke<{ id: PartId; name: string }[]>("cad_tab_parts", { id }).then(
      (p) => n === seq && setLabels(p),
      () => n === seq && setLabels([]),
    );
    invoke<CadDocument | null>("cad_tab_document", { id }).then(
      (d) => n === seq && setConfigs((d?.configurations ?? []).map((c) => c.name)),
      () => n === seq && setConfigs([]),
    );
  });
  const chosen = () => props.kind.parts ?? [];
  const has = (id: PartId) => chosen().some((x) => samePart(x, id));
  const choose = (next: PartId[]) => props.update((x) => x.type === "derived" && (x.parts = next.length ? next : undefined));
  const gone = () => chosen().filter((_, i) => props.missing.includes(i));
  const tabOptions = () => [
    ...sources().map((x) => ({ value: String(x.id), label: tBackend(x.name) })),
    ...(sources().some((x) => x.id === props.kind.studio) ? [] : [{ value: String(props.kind.studio), label: tl("Pestaña que ya no está") }]),
  ];
  return (
    <>
      <Row label={tl("De la pestaña")}>
        <Select
          options={tabOptions()}
          value={String(props.kind.studio)}
          onChange={(v) =>
            props.update((x) => {
              if (x.type !== "derived" || x.studio === +v) return;
              x.studio = +v;
              x.parts = undefined;
              x.configuration = undefined;
            })
          }
        />
      </Row>
      <Show when={configs().length > 0 || props.kind.configuration}>
        <Row label={tl("Configuración")}>
          <Select
            options={[{ value: "", label: tl("La de base") }, ...configs().map((c) => ({ value: c, label: c }))]}
            value={props.kind.configuration ?? ""}
            onChange={(v) => props.update((x) => x.type === "derived" && (x.configuration = v || undefined))}
          />
        </Row>
      </Show>
      <div class="space-y-1" data-derived-parts aria-label={tl("Piezas")}>
        <span class="text-xs text-text-muted">{tl("Piezas (sin marcar ninguna, todas)")}</span>
        <Show when={labels().length > 0} fallback={<p class="text-[11px] text-text-dim">{tl("Esa pestaña todavía no tiene piezas")}</p>}>
          <For each={labels()}>
            {(p) => (
              <Checkbox
                small
                label={p.name}
                checked={has(p.id)}
                onChange={() => choose(has(p.id) ? chosen().filter((x) => !samePart(x, p.id)) : [...chosen(), p.id])}
              />
            )}
          </For>
        </Show>
        <Show when={gone().length > 0}>
          <div class="flex items-center justify-between text-[11px] text-error">
            <span>{tl("El original ya no tiene {n} de las piezas elegidas", { n: gone().length })}</span>
            <button class="underline hover:text-text" onClick={() => choose(chosen().filter((_, i) => !props.missing.includes(i)))}>
              {tl("Quitarlas")}
            </button>
          </div>
        </Show>
      </div>
    </>
  );
};

const PartChecklist: Component<{
  store: CadStore;
  ui: CadUi;
  featureId: number;
  label: string;
  /** Qué significa no elegir ninguna */
  empty: string;
  value: PartId[];
  /** Las que eligió sola sin `value`: se ven marcadas y desde ahí se cambia */
  auto?: PartId[];
  /** Posiciones de `value` que el último cálculo no encontró */
  missing?: number[];
  onChange: (v: PartId[]) => void;
}> = (props) => {
  const ui = props.ui;
  const owner = () => `${props.featureId}:piezas:${props.label}`;
  const active = () => (ui.pick() as { owner?: string }).owner === owner();
  const [parts, setParts] = createSignal<PartView[]>([]);
  let seq = 0;
  createEffect(() => {
    props.store.doc();
    const n = ++seq;
    void props.store.partsBefore(props.featureId).then((p) => n === seq && setParts(p));
  });
  // Las elegidas solas: al elegir se ve el sólido de antes y el cálculo no
  // las trae, así que valen las últimas conocidas
  const [lastAuto, setLastAuto] = createSignal<PartId[]>([]);
  createEffect(() => props.auto && setLastAuto(props.auto));
  const shown = () => (props.value.length > 0 ? props.value : (props.auto ?? lastAuto()));
  const has = (id: PartId) => shown().some((x) => samePart(x, id));
  const nameOf = (id: PartId) => parts().find((p) => samePart(p.id, id))?.name ?? props.store.result()?.parts.find((p) => samePart(p.id, id))?.name;
  const gone = () => props.value.filter((v, i) => (props.missing ?? []).includes(i) || (parts().length > 0 && !parts().some((p) => samePart(p.id, v))));
  const toggle = (id: PartId) => props.onChange(has(id) ? shown().filter((x) => !samePart(x, id)) : [...shown(), id]);
  // Se elige sobre el sólido de antes de la operación (lo que resta ya no se ve en la vista previa)
  const activate = () => {
    void props.store.setSelecting(true);
    ui.setPick({ kind: "parts", owner: owner(), prompt: tl("Clic en las piezas (otra vez para quitarla)"), toggle: (id) => toggle(id) });
  };
  // Elegidas resaltadas en el visor mientras se elige
  createEffect(() => {
    if (!active()) return;
    const faces = (props.store.result()?.parts ?? []).filter((p) => has(p.id)).flatMap((p) => Array.from({ length: p.faces[1] - p.faces[0] }, (_, k) => p.faces[0] + k));
    ui.setHighlight({ faces, edges: [] });
  });
  createEffect(
    on(active, (now, before) => {
      if (now || !before) return;
      ui.setHighlight({ faces: [], edges: [] });
      if (ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });
  return (
    <div class="space-y-1" aria-label={props.label}>
      <span class="text-xs text-text-muted">{props.label}</span>
      <Show when={parts().length > 0} fallback={<p class="text-[11px] text-text-dim">{tl("Todavía no hay piezas")}</p>}>
        <div
          data-parts-box
          class={clsx(
            "min-h-[2rem] rounded border px-1.5 py-1 cursor-pointer text-xs",
            active() ? "border-accent bg-accent/10" : "border-border bg-surface/40 hover:border-border-hover",
          )}
          onClick={() => (active() ? ui.cancelPick() : activate())}
        >
          <Show when={shown().length > 0} fallback={<span class="text-text-dim">{active() ? tl("Clic en las piezas") : tl("{empty}: clic para elegir", { empty: props.empty })}</span>}>
            <For each={shown()}>
              {(id) => (
                <div class="group flex items-center justify-between gap-1 rounded px-1 hover:bg-surface">
                  <span class={clsx(props.value.length ? "text-text" : "text-text-muted")}>{nameOf(id) ?? tl("Pieza")}</span>
                  <button
                    class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-text"
                    aria-label={tl("Quitar {v1}", { v1: nameOf(id) ?? tl("la pieza") })}
                    onClick={(e) => {
                      e.stopPropagation();
                      toggle(id);
                    }}
                  >
                    <Icons.X size={10} />
                  </button>
                </div>
              )}
            </For>
          </Show>
        </div>
      </Show>
      <For each={gone()}>
        {(g) => (
          <div class="flex items-center justify-between text-[11px] text-error">
            <span>{tl("Pieza no encontrada")}</span>
            <button aria-label={tl("Quitar la pieza no encontrada")} onClick={() => props.onChange(props.value.filter((x) => !samePart(x, g)))}>
              <Icons.X size={10} />
            </button>
          </div>
        )}
      </For>
      <Show when={props.value.length === 0 && shown().length > 0}>
        <p class="text-[11px] text-text-dim">{props.empty} {tl("(elegidas solas)")}</p>
      </Show>
    </div>
  );
};

/** Qué regiones del sketch usar: todas o las elegidas con clic en el visor */
// ─── Cajas de selección ───────────────────────────────────────────────────

const NOUNS = { get edges() { return tl("Arista"); }, get faces() { return tl("Cara"); }, get face() { return tl("Cara"); } } as const;
const REF_PROMPTS = {
  get plane() { return tl("Clic en lo que define el plano: una cara o un plano; dos planos; un plano y una arista; una arista y un punto; tres vértices…"); },
  get axis() { return tl("Clic en lo que define el eje: una arista, una cara cilíndrica, dos vértices, dos planos…"); },
  get point() { return tl("Clic en lo que define el punto: un vértice, una arista (su medio o su centro), una arista y una cara…"); },
};
const REF_HINTS = {
  get plane() { return tl("Una cara o un plano (desfasado) · dos planos (medio o bisector) · un plano y una línea (en ángulo) · una línea y un punto · una curva y un punto (normal) · una cara curva y un punto (tangente) · tres puntos"); },
  get axis() { return tl("Una arista o línea · una circunferencia · una cara cilíndrica o cónica · dos puntos · dos planos (su cruce) · un plano y un punto (normal)"); },
  get point() { return tl("Un vértice · una arista (su medio, o el centro si es circular) · una esfera · dos puntos (el medio) · una arista y una cara (el cruce) · un punto y un plano (la proyección) · dos líneas (su cruce)"); },
};

/**
 * Lo que arma una referencia por clics: cada clic en el visor suma algo
 * (cara, arista, vértice o plano) y el tipo se deduce de lo elegido; abajo
 * dice qué se armó. También se suman referencias del árbol.
 */
const RefPicksBox: Component<{
  store: CadStore;
  ui: CadUi;
  owner: string;
  feature: number;
  what: "plane" | "axis" | "point";
  refs: RefPick[];
  lost?: number[];
  onChange: (refs: RefPick[]) => void;
}> = (props) => {
  const ui = props.ui;
  const active = () => (ui.pick() as { owner?: string }).owner === props.owner;
  const names = () => new Map((props.store.doc()?.features ?? []).map((f) => [f.id, f.name]));
  const label = (r: RefPick): string => {
    if (r.type === "face") return tl("Cara");
    if (r.type === "edge") return tl("Arista");
    if (r.type === "plane") {
      const p = r.plane;
      return p.type === "reference" ? (names().get(p.feature) ?? tl("Plano")) : p.type === "frame" ? `${names().get(p.feature) ?? tl("Sistema")} · ${FRAME_PLANES.find(([n]) => n === p.normal)![1]}` : p.type === "face" ? tl("Cara plana") : p.type === "custom" ? tl("Plano propio") : PLANE_LABELS[p.type];
    }
    if (r.type === "axis") {
      const a = r.axis;
      return a.type === "reference" ? (names().get(a.feature) ?? tl("Eje")) : a.type === "frame" ? `${names().get(a.feature) ?? tl("Sistema")} · ${a.axis.toUpperCase()}` : a.type === "sketch_line" ? tl("Línea {line}", { line: a.line }) : a.type === "x" || a.type === "y" || a.type === "z" ? tl("Eje {a}", { a: a.type.toUpperCase() }) : tl("Eje");
    }
    const p = r.point;
    if (p.type === "edge_end") return tl("Vértice");
    if (p.type === "reference") return names().get(p.feature) ?? tl("Punto");
    if (p.type === "at") return `Punto (${p.point.map((v) => +v.toFixed(2)).join("; ")})`;
    return tl("Punto");
  };
  const activate = () => {
    void props.store.setSelecting(true);
    ui.setPick({ kind: "refs", owner: props.owner, prompt: REF_PROMPTS[props.what], add: (p) => props.onChange([...props.refs, p]) });
  };
  // Vacía al abrir el diálogo: se empieza eligiendo (memo: `on` sobre
  // `!!draft()` correría con cada cambio del borrador, no solo al abrirlo)
  const isOpen = createMemo(() => !!props.store.draft());
  createEffect(
    on(
      isOpen,
      (open) => {
        if (open && untrack(() => props.refs.length === 0)) queueMicrotask(() => props.store.draft() && activate());
      },
    ),
  );
  createEffect(
    on(active, (now, before) => {
      if (!now && before && ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });
  const how = () => props.store.result()?.references.find((r) => r.id === props.feature)?.how;
  const problem = () => {
    const st = props.store.stateOf(props.feature);
    return st?.state === "error" ? st.message : undefined;
  };
  // Del árbol: planos, ejes y puntos de referencia (y los ejes base)
  const treeOptions = () => [
    { value: "", label: tl("Sumar del árbol…") },
    ...(["x", "y", "z"] as const).map((a) => ({ value: `axis:${a}`, label: tl("Eje {a}", { a: a.toUpperCase() }) })),
    ...refOptions(props.store, "plane", props.feature).map((o) => ({ value: `plane:${o.value.slice(4)}`, label: o.label })),
    ...refOptions(props.store, "axis", props.feature).map((o) => ({ value: `axisref:${o.value.slice(4)}`, label: o.label })),
    ...refOptions(props.store, "point", props.feature).map((o) => ({ value: `point:${o.value.slice(4)}`, label: o.label })),
  ];
  const addFromTree = (v: string) => {
    const [kind, id] = v.split(":");
    const r: RefPick | undefined =
      kind === "axis"
        ? { type: "axis", axis: { type: id as "x" | "y" | "z" } }
        : kind === "plane"
          ? { type: "plane", plane: { type: "reference", feature: +id } }
          : kind === "axisref"
            ? { type: "axis", axis: { type: "reference", feature: +id } }
            : kind === "point"
              ? { type: "point", point: { type: "reference", feature: +id } }
              : undefined;
    if (r) props.onChange([...props.refs, r]);
  };
  return (
    <div class="space-y-1" aria-label={tl("Lo elegido")} data-ref-picks>
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs text-text-muted">{tl("Elegido")}</span>
        <Button size="sm" variant={active() ? "primary" : "default"} onClick={() => (active() ? ui.cancelPick() : activate())}>
          {active() ? tl("Listo") : tl("Elegir en el visor")}
        </Button>
      </div>
      <For each={props.refs} fallback={<p class="text-[11px] text-text-dim">{tl("Nada todavía: clic en el visor")}</p>}>
        {(r, i) => (
          <div class="flex items-center gap-1 text-xs" data-ref-pick={i()}>
            <span class={clsx("flex-1 truncate", (props.lost ?? []).includes(i()) ? "text-error" : "text-text")}>
              {label(r)}
              {(props.lost ?? []).includes(i()) ? tl(" · no encontrada") : ""}
            </span>
            <IconButton aria-label={tl("Quitar {v1}", { v1: label(r) })} size="sm" variant="ghost" onClick={() => props.onChange(props.refs.filter((_, k) => k !== i()))}>
              <Icons.Trash size={11} />
            </IconButton>
          </div>
        )}
      </For>
      <Select options={treeOptions()} value="" onChange={(v) => v && addFromTree(v)} />
      <Show when={how()} fallback={<p class="text-[11px] text-text-dim leading-relaxed">{problem() ?? REF_HINTS[props.what]}</p>}>
        <p class="text-[11px] text-accent" data-ref-how>
          {tl("Se armó:")} {how()}
        </p>
      </Show>
    </div>
  );
};

const PROMPTS = {
  get edges() { return tl("Clic en las aristas (otra vez para quitarla)"); },
  get faces() { return tl("Clic en las caras (otra vez para quitarla)"); },
  get face() { return tl("Clic en la cara"); },
};

/**
 * Caja de un campo de referencias, como en Onshape: activa, lo que se elige en
 * el visor va ahí (y se ve el sólido de antes de la operación); la lista
 * resalta cada ítem al pasar el mouse y lo quita con ✗.
 */
/**
 * Engranar con otro engrane: toma su módulo y ángulo de presión, se pone a la
 * distancia entre centros y gira para que los dientes calcen.
 */
const GearLinkBox: Component<{
  store: CadStore;
  ui: CadUi;
  feature: number;
  shape: Extract<PrimitiveShape, { type: "gear" }>;
  field: (label: string, rel: string, raw: number, write: (k: FeatureKind, v: number) => void, suffix?: string) => JSX.Element;
  update: (mutate: (k: FeatureKind) => void) => void;
}> = (props) => {
  const sources = createMemo(() => {
    const doc = props.store.doc();
    const order = new Map((doc?.features ?? []).map((f, i) => [f.id, i]));
    const at = order.get(props.feature) ?? Infinity;
    // Las poleas y los piñones de cadena no engranan con nada: los une una correa o una cadena
    return (props.store.result()?.gears ?? []).filter((x) => x.feature !== props.feature && (order.get(x.feature) ?? Infinity) < at && x.part.spec.kind !== "pulley" && x.part.spec.kind !== "sprocket");
  });
  const name = (id: number) => props.store.doc()?.features.find((f) => f.id === id)?.name ?? `#${id}`;
  const gear = (x: FeatureKind) => (x.type === "primitive" && x.shape.type === "gear" ? x.shape : undefined);
  const link = () => props.shape.link ?? undefined;
  const own = () => props.store.result()?.gears?.find((x) => x.feature === props.feature);
  const source = () => sources().find((x) => x.feature === link()?.feature);
  const [designError, setDesignError] = createSignal("");
  const dotP = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  /** Clic en el visor sobre el plano del primer engrane: `use` recibe el punto relativo a su centro y su marco (x, y en el plano) */
  const pickPoint = (use: (d: P3, frame: { x: P3; y: P3 }) => void) => {
    const src = source();
    if (!src) return;
    const nz = Math.hypot(...src.z) || 1;
    const z: P3 = [src.z[0] / nz, src.z[1] / nz, src.z[2] / nz];
    const k = dotP(src.x, z);
    const xr: P3 = [src.x[0] - z[0] * k, src.x[1] - z[1] * k, src.x[2] - z[2] * k];
    const nx = Math.hypot(...xr) || 1;
    const x: P3 = [xr[0] / nx, xr[1] / nx, xr[2] / nx];
    const y: P3 = [z[1] * x[2] - z[2] * x[1], z[2] * x[0] - z[0] * x[2], z[0] * x[1] - z[1] * x[0]];
    props.ui.setPick({
      kind: "plane_point",
      prompt: tl("Clic donde quieres este engrane (se toma el punto sobre el plano de la cara de abajo del primero)"),
      plane: { origin: src.origin, normal: z, x_dir: x },
      done: (at) => use([at[0] - src.origin[0], at[1] - src.origin[1], at[2] - src.origin[2]], { x, y }),
    });
  };
  /** Calcula los dientes de este engrane por relación o por distancia entre centros y los escribe */
  const design = (ratio: number | null, distance: number | null) => {
    const src = source();
    if (!src) return;
    void props.store
      .gearDesign(JSON.parse(JSON.stringify(src.part)) as GearPart, props.shape.spec.kind, ratio, distance)
      .then((teeth) => {
        setDesignError("");
        props.update((x) => {
          const s = gear(x);
          if (s) s.spec.teeth = teeth;
        });
      })
      .catch((e) => setDesignError(String(e)));
  };
  return (
    <Show when={link() || sources().length > 0}>
      <Row label={tl("Engranar con")}>
        <Select
          options={[{ value: "", label: tl("Ninguno") }, ...sources().map((x) => ({ value: String(x.feature), label: name(x.feature) }))]}
          value={link() ? String(link()!.feature) : ""}
          onChange={(v) =>
            props.update((x) => {
              const s = gear(x);
              if (!s) return;
              if (!v) {
                // Soltar: queda donde está
                const o = own();
                if (o && x.type === "primitive") [x.origin, x.z, x.x] = [o.origin, o.z, o.x];
                s.link = null;
              } else {
                s.link = { feature: Number(v), angle: s.link?.angle ?? 0, offset: s.link?.offset ?? 0, distance: s.link?.distance ?? null };
              }
            })
          }
        />
      </Row>
      <Show when={link()}>
        {(l) => (
          <>
            <Show
              when={source()?.part.spec.kind === "rack"}
              fallback={
                <>
                  {props.field(tl("Dirección del centro"), "kind.shape.link.angle", l().angle, (x, v) => { const s = gear(x); if (s?.link) s.link.angle = v; }, "°")}
                  <Button onClick={() => pickPoint((d, frame) => {
                    const angle = (Math.atan2(dotP(d, frame.y), dotP(d, frame.x)) * 180) / Math.PI;
                    props.update((x) => { const s = gear(x); if (s?.link) s.link.angle = +angle.toFixed(2); });
                  })}>
                    {tl("Elegir la dirección en el visor")}
                  </Button>
                </>
              }
            >
              {props.field(tl("Posición sobre la cremallera"), "kind.shape.link.along", l().along ?? 0, (x, v) => { const s = gear(x); if (s?.link) s.link.along = v; }, "mm")}
              <Button onClick={() => pickPoint((d, frame) => {
                const sp = source()!.part.spec;
                const beta = (sp.helix_angle * Math.PI) / 180;
                const mn = source()!.part.transverse_module ? sp.module * Math.cos(beta) : sp.module;
                const pitch = (Math.PI * mn) / Math.cos(beta);
                const along = dotP(d, frame.x) - ((sp.teeth - 1) * pitch) / 2;
                props.update((x) => { const s = gear(x); if (s?.link) s.link.along = +along.toFixed(2); });
              })}>
                {tl("Elegir la posición en el visor")}
              </Button>
            </Show>
            {props.field(tl("Corrimiento axial"), "kind.shape.link.offset", l().offset, (x, v) => { const s = gear(x); if (s?.link) s.link.offset = v; }, "mm")}
            <Row label={tl("Distancia entre centros")}>
              <NumberInput
                value={l().distance ?? own()?.center_distance ?? 0}
                onChange={(v) => props.update((x) => { const s = gear(x); if (s?.link) s.link.distance = v > 0 ? v : null; })}
              />
            </Row>
            <Row label={tl("Relación (dientes / dientes del otro)")}>
              <NumberInput
                value={source() ? props.shape.spec.teeth / source()!.part.spec.teeth : 0}
                onChange={(v) => design(v, null)}
              />
            </Row>
            <Show when={source()?.part.spec.kind === "bevel" && props.shape.spec.kind === "bevel"}>
              {(_) => {
                const src = () => source()!;
                const sigma = () => (props.shape.spec.shaft_angle * Math.PI) / 180;
                // tan δ₁ = senΣ / (z₂/z₁ + cosΣ)
                const ideal = () => (Math.atan2(Math.sin(sigma()), props.shape.spec.teeth / src().part.spec.teeth + Math.cos(sigma())) * 180) / Math.PI;
                const fix = () =>
                  void props.store.commit((d) => {
                    const f = d.features.find((x) => x.id === src().feature);
                    if (f?.kind.type === "primitive" && f.kind.shape.type === "gear") f.kind.shape.spec.cone_angle = +ideal().toFixed(3);
                  });
                return (
                  <>
                    <Row label={tl("Cono del primero que da esta relación")}>
                      <span class="font-mono text-xs text-text">{ideal().toFixed(2)}°</span>
                    </Row>
                    <Show when={Math.abs(ideal() - src().part.spec.cone_angle) > 0.05}>
                      <Button onClick={fix}>{tl("Poner ese cono en el primero")}</Button>
                    </Show>
                  </>
                );
              }}
            </Show>
            <Show when={l().distance != null}>
              <Button onClick={() => design(null, l().distance!)}>{tl("Calcular los dientes para esta distancia")}</Button>
            </Show>
            <Show when={designError()}>
              <p class="text-[11px] text-error">{designError()}</p>
            </Show>
            <Show when={l().distance != null}>
              <Button onClick={() => props.update((x) => { const s = gear(x); if (s?.link) s.link.distance = null; })}>
                {tl("Usar la distancia de las correcciones")}
              </Button>
            </Show>
            <p class="text-[11px] text-text-dim">
              {tl("El módulo y el ángulo de presión se toman del otro engrane; con una distancia pedida, la corrección de perfil se ajusta sola.")}
            </p>
            <Show when={source() && own()}>
              <GearStrengthBox store={props.store} first={source()!.part} second={own()!.part} />
            </Show>
          </>
        )}
      </Show>
    </Show>
  );
};

/**
 * Planetario de una vez con este engrane de sol: satélites enlazados a él, portasatélites con sus pasadores y
 * corona concentrica. El backend revisa los dientes y la condición de montaje.
 */
/**
 * Arma en el ensamble abierto el planetario creado con «Armar un planetario»: corona fija, sol y
 * portasatélites girando, satélites en sus pasadores y los acoplamientos. Devuelve el motivo si no se pudo.
 */
async function assemblePlanetary(store: CadStore): Promise<string | undefined> {
  const made = planetaryAssembly(await store.assemblyFeatures(), store.result()?.gears ?? [], store.result()?.parts ?? [], store.doc()?.assembly ?? undefined);
  if (typeof made === "string") return made;
  await store.editAssembly((a) => {
    a.instances = made.instances;
    a.mates = made.mates;
    a.next_id = made.next_id;
  });
  const found = await store.assemblySuggest();
  await store.editAssembly((a) => {
    a.relations = [...(a.relations ?? []), ...found];
  });
}

const PlanetaryBox: Component<{ store: CadStore; ui: CadUi; feature: number; shape: Extract<PrimitiveShape, { type: "gear" }> }> = (props) => {
  const [open, setOpen] = createSignal(false);
  // Ya creado: se puede armar en una pestaña de ensamble nueva
  const [made, setMade] = createSignal(false);
  const [planets, setPlanets] = createSignal(3);
  const [teeth, setTeeth] = createSignal(12);
  const [plan, setPlan] = createSignal<PlanetaryPlan>();
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  let seq = 0;
  createEffect(() => {
    if (!open()) return;
    const mine = ++seq;
    void props.store
      .planetaryPlan(props.shape.spec.module, props.shape.spec.teeth, teeth(), null, planets(), props.shape.spec.pressure_angle)
      .then((p) => mine === seq && (setPlan(p), setError("")))
      .catch((e) => mine === seq && (setPlan(undefined), setError(String(e))));
  });
  const create = async () => {
    const p = plan();
    const sun = props.store.doc()?.features.find((f) => f.id === props.feature);
    if (!p || !sun) return;
    setBusy(true);
    try {
      // El sol se acepta primero: lo demás se agrega al diseño ya guardado
      await props.store.acceptDraft();
      const sunNow = props.store.doc()?.features.find((f) => f.id === props.feature) ?? sun;
      let first = 0;
      await props.store.commit((d) => {
        first = d.next_id ?? Math.max(0, ...d.features.map((f) => f.id)) + 1;
        const feats = planetFeatures(sunNow, p, first);
        d.features.push(...feats);
        d.next_id = first + feats.length;
      });
      const planet0 = props.store.result()?.gears?.find((g) => g.feature === first);
      if (!planet0) throw new Error(tl("el primer satélite no se calculó: revisa el aviso del árbol"));
      await props.store.commit((d) => {
        const id = d.next_id ?? Math.max(0, ...d.features.map((f) => f.id)) + 1;
        d.features.push(ringFeature(sunNow, p, planet0, id));
        d.next_id = id + 1;
      });
      setOpen(false);
      setMade(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  /** Pestaña de ensamble nueva con el planetario armado (las piezas quedan en este diseño) */
  const assemble = async () => {
    setBusy(true);
    try {
      await changeDesignTabs(props.store, props.ui, "cad_tab_add", { kind: "assembly", name: null });
      if (props.store.activeTab()?.kind !== "assembly") return;
      const note = await assemblePlanetary(props.store);
      if (note) props.ui.setMessage(tBackend(note));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="space-y-1 border-t border-border pt-1.5" data-planetary>
      <Checkbox small label={tl("Armar un planetario con este sol")} checked={open()} onChange={setOpen} />
      <Show when={made() && !open()}>
        <Button size="sm" fullWidth disabled={busy()} onClick={() => void assemble()} data-planetary-assemble>
          {tl("Armar en una pestaña de ensamble nueva")}
        </Button>
      </Show>
      <Show when={open()}>
        <Row label={tl("Satélites")}>
          <NumberInput value={planets()} onChange={(v) => v >= 2 && setPlanets(Math.round(v))} />
        </Row>
        <Row label={tl("Dientes de cada satélite")}>
          <NumberInput value={teeth()} onChange={(v) => v >= 6 && setTeeth(Math.round(v))} />
        </Row>
        <Show when={error()}>
          <p class="text-[11px] text-error">{tBackend(error())}</p>
        </Show>
        <Show when={plan()}>
          {(p) => (
            <div class="space-y-0.5 text-[11px] text-text-dim">
              <p>
                {tl("Corona de {z} dientes · centros a {a} mm", { z: p().ring_teeth, a: +p().center_distance.toFixed(3) })}
              </p>
              <p>
                {tl("Corona quieta: el portasatélites gira {i} veces menos que el sol", { i: +p().ratio_ring_fixed.toFixed(3) })} · {tl("portasatélites quieto: corona {r}", { r: +p().ratio_carrier_fixed.toFixed(3) })}
              </p>
              <For each={p().warnings}>{(w) => <p class="text-warning">{tBackend(w)}</p>}</For>
              <Button disabled={busy()} onClick={() => void create()}>
                {tl("Crear satélites, portasatélites y corona")}
              </Button>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};

/**
 * Correa o cadena entre esta rueda y otra: dientes (o eslabones) para la distancia entre centros deseada y
 * los centros exactos con ese largo entero.
 */
const DriveBox: Component<{ store: CadStore; spec: GearSpec }> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [other, setOther] = createSignal(40);
  const [center, setCenter] = createSignal(100);
  const [result, setResult] = createSignal<DriveLength>();
  const [error, setError] = createSignal("");
  let seq = 0;
  createEffect(() => {
    if (!open()) return;
    const mine = ++seq;
    void props.store
      .driveLength(props.spec.form, props.spec.teeth, other(), center())
      .then((r) => mine === seq && (setResult(r), setError("")))
      .catch((e) => mine === seq && (setResult(undefined), setError(String(e))));
  });
  const chain = () => props.spec.kind === "sprocket";
  return (
    <div class="space-y-1 border-t border-border pt-1.5" data-drive>
      <Checkbox small label={chain() ? tl("Largo de la cadena con otro piñón") : tl("Largo de la correa con otra polea")} checked={open()} onChange={setOpen} />
      <Show when={open()}>
        <Row label={chain() ? tl("Dientes del otro piñón") : tl("Dientes de la otra polea")}>
          <NumberInput value={other()} onChange={(v) => v >= 8 && setOther(Math.round(v))} />
        </Row>
        <Row label={tl("Distancia entre centros deseada (mm)")}>
          <NumberInput value={center()} onChange={(v) => v > 0 && setCenter(v)} />
        </Row>
        <Show when={error()}>
          <p class="text-[11px] text-error">{tBackend(error())}</p>
        </Show>
        <Show when={result()}>
          {(r) => (
            <div class="space-y-0.5 text-[11px]">
              <p>
                {chain() ? tl("{n} eslabones", { n: r().count }) : tl("{n} dientes de correa", { n: r().count })} · {tl("largo")} {+r().length.toFixed(2)} mm
              </p>
              <p>{tl("Con ese largo, los centros quedan a {mm} mm", { mm: +r().center_distance.toFixed(3) })}</p>
              <For each={r().notes}>{(n) => <p class="text-warning">{tBackend(n)}</p>}</For>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};

/**
 * Resistencia del par (orientativa): flexión del pie de cada diente y presión de contacto, con el par que
 * recibe el primer engrane, su velocidad, los materiales y un factor de impresión.
 */
const GearStrengthBox: Component<{ store: CadStore; first: GearPart; second: GearPart }> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [materials, setMaterials] = createSignal<GearMaterial[]>([]);
  const [input, setInput] = createSignal<GearStrengthInput>({ torque: 1000, rpm: 300, service: 1, material_a: "pla", material_b: "pla", factor_a: 1, factor_b: 1 });
  const [result, setResult] = createSignal<GearStrength>();
  const [error, setError] = createSignal("");
  let seq = 0;
  createEffect(() => {
    if (open() && materials().length === 0) void props.store.gearMaterials().then(setMaterials);
  });
  // Se recalcula solo al cambiar cualquier dato mientras está abierto
  createEffect(() => {
    if (!open()) return;
    const mine = ++seq;
    const i = input();
    const first = JSON.parse(JSON.stringify(props.first)) as GearPart;
    const second = JSON.parse(JSON.stringify(props.second)) as GearPart;
    void props.store
      .gearStrength(first, second, i)
      .then((r) => mine === seq && (setResult(r), setError("")))
      .catch((e) => mine === seq && (setResult(undefined), setError(String(e))));
  });
  const set = <K extends keyof GearStrengthInput>(k: K, v: GearStrengthInput[K]) => setInput({ ...input(), [k]: v });
  const options = () => materials().map((m) => ({ value: m.id, label: tl(m.name) }));
  const fmt1 = (x: number) => (Math.abs(x) >= 100 ? x.toFixed(0) : x.toFixed(2));
  const verdict = (s: number) => (s >= 1.5 ? "text-success" : s >= 1 ? "text-warning" : "text-error");
  return (
    <div class="space-y-1 border-t border-border pt-1.5" data-gear-strength>
      <Checkbox small label={tl("Resistencia del par (orientativa)")} checked={open()} onChange={setOpen} />
      <Show when={open()}>
        <Row label={tl("Par en el primero (N·m)")}>
          <NumberInput value={+(input().torque / 1000).toFixed(4)} onChange={(v) => v > 0 && set("torque", v * 1000)} />
        </Row>
        <Row label={tl("Velocidad del primero (rpm)")}>
          <NumberInput value={input().rpm} onChange={(v) => v >= 0 && set("rpm", v)} />
        </Row>
        <Row label={tl("Factor de aplicación (1 uniforme, 1,5 con golpes)")}>
          <NumberInput value={input().service} onChange={(v) => v >= 1 && set("service", v)} />
        </Row>
        <Row label={tl("Material del primero")}>
          <Select options={options()} value={input().material_a} onChange={(v) => set("material_a", v)} />
        </Row>
        <Row label={tl("Material del segundo")}>
          <Select options={options()} value={input().material_b} onChange={(v) => set("material_b", v)} />
        </Row>
        <Row label={tl("Factor de impresión (1: sin rebaja)")}>
          <NumberInput value={input().factor_a} onChange={(v) => v > 0 && setInput({ ...input(), factor_a: v, factor_b: v })} />
        </Row>
        <Show when={error()}>
          <p class="text-[11px] text-error">{tBackend(error())}</p>
        </Show>
        <Show when={result()}>
          {(r) => (
            <div class="space-y-0.5 text-[11px]">
              <p class="text-text-dim">
                {tl("Fuerza tangencial")} {fmt1(r().tangential_force)} N · {tl("velocidad")} {r().speed.toFixed(2)} m/s · K<sub>v</sub> {r().dynamic.toFixed(2)} · ε<sub>α</sub> {r().contact_ratio.toFixed(2)}
              </p>
              <For each={[[tl("Flexión del primero"), r().bending_a], [tl("Flexión del segundo"), r().bending_b]] as [string, GearBending | null][]}>
                {([label, b]) => (
                  <Show when={b}>
                    {(b) => (
                      <p>
                        {label}: σ<sub>F</sub> {fmt1(b().sigma_f)} / {fmt1(b().allowable)} MPa · {tl("seguridad")} <span class={verdict(b().safety)}>{b().safety.toFixed(2)}</span>
                        <span class="text-text-dim"> (Y<sub>F</sub> {b().y_f.toFixed(2)}, Y<sub>S</sub> {b().y_s.toFixed(2)})</span>
                      </p>
                    )}
                  </Show>
                )}
              </For>
              <p>
                {tl("Contacto")}: σ<sub>H</sub> {fmt1(r().sigma_h)} / {fmt1(r().allowable_h)} MPa · {tl("seguridad")} <span class={verdict(r().safety_h)}>{r().safety_h.toFixed(2)}</span>
              </p>
              <For each={r().notes}>{(n) => <p class="text-text-dim">{tBackend(n)}</p>}</For>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};

/** Medidas de un engrane (cuadro de propiedades de su operación) */
const GearFields: Component<{
  store: CadStore;
  ui: CadUi;
  feature: number;
  shape: Extract<PrimitiveShape, { type: "gear" }>;
  field: (label: string, rel: string, raw: number, write: (k: FeatureKind, v: number) => void, suffix?: string) => JSX.Element;
  update: (mutate: (k: FeatureKind) => void) => void;
}> = (props) => {
  const [info, setInfo] = createSignal<GearInfo | null>(null);
  let seq = 0;
  createEffect(() => {
    const { type: _t, ...part } = props.shape;
    const mine = ++seq;
    void props.store.gearInfo(JSON.parse(JSON.stringify(part)) as GearPart).then((i) => mine === seq && setInfo(i));
  });
  const g = (x: FeatureKind) => (x.type === "primitive" && x.shape.type === "gear" ? x.shape : undefined);
  const num = (label: string, key: keyof GearPart, suffix = "mm") =>
    props.field(label, `kind.shape.${key}`, props.shape[key] as number, (x, v) => {
      const s = g(x);
      if (s) (s[key] as number) = v;
    }, suffix);
  const spec = (label: string, key: keyof GearSpec, suffix?: string) =>
    props.field(label, `kind.shape.spec.${key}`, props.shape.spec[key] as number, (x, v) => {
      const s = g(x);
      if (s) (s.spec[key] as number) = v;
    }, suffix);
  const kind = () => props.shape.spec.kind;
  // Polea dentada o piñón de cadena: sin módulo ni evolvente, el paso lo da la correa o la cadena
  const wheel = () => kind() === "pulley" || kind() === "sprocket";
  const [forms, setForms] = createSignal<ToothFormInfo[]>([]);
  void props.store.toothForms().then(setForms);
  const familyForms = () => forms().filter((f) => f.family === (kind() === "pulley" ? "belt" : kind() === "sprocket" ? "chain" : "gear"));
  return (
    <>
      <Show when={!wheel()}>
        <GearLinkBox store={props.store} ui={props.ui} feature={props.feature} shape={props.shape} field={props.field} update={props.update} />
      </Show>
      <Show when={kind() === "external" && !props.shape.link}>
        <PlanetaryBox store={props.store} ui={props.ui} feature={props.feature} shape={props.shape} />
      </Show>
      <Row label={tl("Tipo")}>
        <Select
          options={[
            { value: "external", label: tl("Engrane exterior") },
            { value: "internal", label: tl("Corona (dientes por dentro)") },
            { value: "rack", label: tl("Cremallera") },
            { value: "bevel", label: tl("Cónico recto") },
            { value: "worm", label: tl("Tornillo sin fin") },
            { value: "pulley", label: tl("Polea dentada (correa)") },
            { value: "sprocket", label: tl("Piñón de cadena") },
          ]}
          value={kind()}
          onChange={(v) =>
            props.update((x) => {
              const s = g(x);
              if (!s) return;
              s.spec.kind = v as GearKind;
              if (v === "internal" && s.outer <= 0) s.outer = Math.ceil(s.spec.module * (s.spec.teeth + 6) + 4);
              if (v === "rack" && s.outer <= 0) s.outer = 8;
              const wheelLike = v === "pulley" || v === "sprocket";
              if (v !== "external" && v !== "bevel" && !wheelLike) [s.hole_count, s.spokes] = [0, 0];
              if (v !== "external" && v !== "bevel" && v !== "worm" && !wheelLike) [s.hub_length, s.hub_diameter, s.bore] = [0, 0, 0];
              // Forma del diente según el tipo: la primera correa o cadena, o evolvente
              const fam = v === "pulley" ? "belt" : v === "sprocket" ? "chain" : "gear";
              const first = forms().find((f) => f.family === fam);
              if (wheelLike && first) {
                [s.spec.form, s.spec.teeth, s.width, s.spec.helix_angle, s.spec.shift, s.herringbone, s.throat, s.link] = [first.form, v === "pulley" ? 20 : 17, first.width, 0, 0, false, 0, null];
              } else if (s.spec.form !== "involute" && !(v === "external" && s.spec.form === "cycloidal")) s.spec.form = "involute";
              if (v !== "pulley") s.flange = 0;
              if (v === "worm") [s.spec.teeth, s.width, s.hub_length, s.hub_diameter, s.spec.helix_angle] = [Math.min(s.spec.teeth, 4), Math.max(s.width, 6 * s.spec.module * Math.PI), 0, 0, 0];
              if (v === "bevel") [s.spec.helix_angle, s.herringbone] = [0, false];
              s.keyway = false;
            })
          }
        />
      </Row>
      <Show when={wheel()}>
        <Row label={kind() === "pulley" ? tl("Correa") : tl("Cadena")}>
          <Select
            options={familyForms().map((f) => ({ value: f.form, label: f.name }))}
            value={props.shape.spec.form}
            onChange={(v) => props.update((x) => { const s = g(x); if (s) s.spec.form = v as ToothForm; })}
          />
        </Row>
      </Show>
      <Show when={!wheel()}>
        {spec(tl("Módulo"), "module", "mm")}
        <Row label={tl("Paso diametral (dientes/pulgada)")}>
          <NumberInput
            value={+(25.4 / props.shape.spec.module).toFixed(4)}
            onChange={(v) => v > 0 && props.update((x) => { const s = g(x); if (s) s.spec.module = +(25.4 / v).toFixed(6); })}
          />
        </Row>
      </Show>
      <Show when={kind() === "external"}>
        <Row label={tl("Perfil del diente")}>
          <Select
            options={[
              { value: "involute", label: tl("Evolvente") },
              { value: "cycloidal", label: tl("Cicloidal") },
            ]}
            value={props.shape.spec.form === "cycloidal" ? "cycloidal" : "involute"}
            onChange={(v) => props.update((x) => { const s = g(x); if (s) [s.spec.form, s.spec.helix_angle, s.spec.shift] = [v as ToothForm, v === "cycloidal" ? 0 : s.spec.helix_angle, v === "cycloidal" ? 0 : s.spec.shift]; })}
          />
        </Row>
        <Show when={props.shape.spec.form === "cycloidal"}>
          {spec(tl("Radio del círculo generador (× módulo)"), "cycloid_circle", "")}
          <p class="text-[11px] text-text-dim">{tl("Los dos engranes del par deben tener el mismo círculo generador. Con 1,5 sirve desde 6 dientes; menos de la mitad del primitivo evita el socavado.")}</p>
        </Show>
      </Show>
      {spec(kind() === "worm" ? tl("Entradas") : tl("Dientes"), "teeth", "")}
      <Show when={kind() === "worm"}>
        {spec(tl("Diámetro primitivo (0: 10·módulo)"), "worm_diameter", "mm")}
        <Checkbox
          small
          label={tl("Mano izquierda")}
          checked={props.shape.spec.left_hand}
          onChange={(c) => props.update((x) => { const s = g(x); if (s) s.spec.left_hand = c; })}
        />
      </Show>
      {num(tl("Ancho"), "width")}
      <Show when={!wheel() && props.shape.spec.form !== "cycloidal"}>{spec(tl("Ángulo de presión"), "pressure_angle", "°")}</Show>
      <Show when={kind() === "pulley"}>{num(tl("Diámetro de las bridas (0: sin bridas)"), "flange")}</Show>
      <Show when={kind() === "bevel"}>
        {spec(tl("Ángulo del cono primitivo"), "cone_angle", "°")}
        <Checkbox
          small
          label={tl("Extremos cónicos")}
          checked={!!props.shape.cone_ends}
          onChange={(c) => props.update((x) => { const s = g(x); if (s) s.cone_ends = c; })}
          hint={tl("Las caras de atrás y de adelante son conos perpendiculares al cono primitivo, como en un cónico de verdad (en vez de planos). Tarda unos segundos más y llega hasta un cono de 50°.")}
        />
        <Show when={props.shape.link}>{spec(tl("Ángulo entre ejes"), "shaft_angle", "°")}</Show>
        {spec(tl("Ángulo de espiral (+ derecha, − izquierda, 0: recto)"), "spiral_angle", "°")}
      </Show>
      <Show when={kind() !== "rack" && kind() !== "bevel" && kind() !== "worm" && !wheel() && props.shape.spec.form !== "cycloidal"}>
        {spec(tl("Ángulo de hélice (+ derecha, − izquierda)"), "helix_angle", "°")}
        <Show when={props.shape.spec.helix_angle !== 0}>
          <Checkbox
            small
            label={tl("Doble helicoidal (espina de pescado)")}
            checked={!!props.shape.herringbone}
            onChange={(c) => props.update((x) => { const s = g(x); if (s) s.herringbone = c; })}
          />
          <Show when={props.shape.herringbone}>{num(tl("Canal central"), "groove")}</Show>
          <Checkbox
            small
            label={tl("El módulo es el transversal")}
            checked={!!props.shape.transverse_module}
            onChange={(c) => props.update((x) => { const s = g(x); if (s) s.transverse_module = c; })}
            hint={tl("Con hélice, el módulo de arriba suele ser el normal (el de la herramienta). Marca esto si tu dato es el del plano de giro: el normal sale de mt·cos β.")}
          />
        </Show>
      </Show>
      <Show when={kind() === "external" && props.shape.spec.helix_angle !== 0 && !props.shape.link}>
        {num(tl("Garganta: distancia al sin fin (0: sin ella)"), "throat")}
      </Show>
      <Show when={!wheel() && props.shape.spec.form !== "cycloidal"}>{spec(tl("Corrección de perfil"), "shift", "")}</Show>
      {spec(wheel() ? tl("Holgura de la ranura o el asiento") : tl("Juego entre dientes"), "backlash", "mm")}
      <Checkbox
        small
        label={tl("Holgura según la impresora")}
        checked={!!props.shape.auto_clearance}
        onChange={(c) => props.update((x) => { const s = g(x); if (s) s.auto_clearance = c; })}
        hint={tl("La holgura de impresión sale de la impresora y el material elegidos en Fabricar (ajuste justo). Cambia sola si cambias de impresora.")}
      />
      <Show when={!props.shape.auto_clearance}>{num(tl("Holgura de impresión"), "print_clearance")}</Show>
      <Show when={kind() === "internal"}>{num(tl("Diámetro exterior"), "outer")}</Show>
      <Show when={kind() === "rack"}>{num(tl("Alto del cuerpo"), "outer")}</Show>
      <Show when={wheel()}>
        <DriveBox store={props.store} spec={props.shape.spec} />
      </Show>
      <Show when={kind() === "external" || kind() === "bevel" || kind() === "worm" || wheel()}>
        {num(tl("Agujero del eje"), "bore")}
        {num(kind() === "worm" ? tl("Diámetro del vástago") : tl("Diámetro del cubo"), "hub_diameter")}
        {num(kind() === "worm" ? tl("Largo del vástago") : tl("Largo del cubo"), "hub_length")}
        <Checkbox
          small
          label={tl("Chavetero (DIN 6885)")}
          checked={props.shape.keyway}
          onChange={(c) => props.update((x) => { const s = g(x); if (s) s.keyway = c; })}
        />
        <Show when={kind() === "external" || kind() === "bevel" || wheel()}>
          {num(tl("Aligeramientos (cantidad)"), "hole_count", "")}
          <Show when={props.shape.hole_count > 0}>
            {num(tl("Ø de cada agujero"), "hole_diameter")}
            {num(tl("Ø del círculo de agujeros"), "hole_circle")}
          </Show>
          <Show when={props.shape.hole_count === 0}>
            {num(tl("Rayos (cantidad)"), "spokes", "")}
            <Show when={(props.shape.spokes ?? 0) > 0}>
              {num(tl("Ancho de cada rayo"), "spoke_width")}
              {num(tl("Grosor del aro"), "rim")}
            </Show>
          </Show>
        </Show>
        <Show when={props.shape.hub_length > 0 && props.shape.bore > 0}>{num(tl("Prisionero (Ø del agujero radial)"), "set_screw")}</Show>
      </Show>
      <Show when={info()}>
        {(i) => (
          <div class="space-y-0.5 text-[11px] text-text-dim">
            <Show when={kind() !== "rack"}>
              <p>
                {tl("Ø primitivo")} {+i().pitch_diameter.toFixed(3)} · {tl("Ø exterior")} {+i().tip_diameter.toFixed(3)} · {tl("Ø de pie")} {+i().root_diameter.toFixed(3)} mm
              </p>
            </Show>
            <For each={i().warnings}>{(w) => <p class="text-warning">{tl(w)}</p>}</For>
            <Show when={i().error}>{(e) => <p class="text-error">{e()}</p>}</Show>
          </div>
        )}
      </Show>
    </>
  );
};

/** Operaciones a las que ya se les contestó "no" a coordinar la rosca (no se vuelve a preguntar) */
const [threadDeclined, setThreadDeclined] = createSignal<number[]>([]);

/**
 * Rosca asociada: con qué otra rosca se coordina la operación (medida, eje y
 * fase del filete). Sin asociar y con roscas que calzan, pregunta; la que está
 * en el mismo eje va primero. También se elige con un clic en el visor.
 */
const ThreadLinkBox: Component<{
  store: CadStore;
  ui: CadUi;
  feature: number;
  value: ThreadLink | null | undefined;
  want: ThreadWant;
  /** Qué se coordina ("el tornillo", "la rosca"…), para la pregunta */
  what: string;
  /** Preguntar con cualquier rosca que calce (si no, solo con una en el mismo eje) */
  askAlways?: boolean;
  onChange: (link: ThreadLink | null, source: ThreadAxis | null) => void;
}> = (props) => {
  const owner = () => `${props.feature}:rosca`;
  const threads = () => props.store.result()?.threads ?? [];
  const doc = () => props.store.doc();
  const own = () => threads().filter((t) => t.feature === props.feature);
  const candidates = createMemo(() => {
    const list = threadCandidates(doc(), threads(), props.feature, props.want);
    // Las que están en el eje propio, primero
    const onAxis = (t: ThreadAxis) => own().some((o) => coaxialThreads(t, o));
    return [...list.filter(onAxis), ...list.filter((t) => !onAxis(t))];
  });
  const suggested = () => candidates().find((t) => own().some((o) => coaxialThreads(t, o)));
  const key = (l: { feature: number; index: number }) => `${l.feature}:${l.index}`;
  const source = () => (props.value ? threads().find((t) => key(t) === key(props.value!)) : undefined);
  const choose = (t: ThreadAxis | undefined) => props.onChange(t ? { feature: t.feature, index: t.index } : null, t ?? null);
  const label = (t: ThreadAxis) => `${threadLabel(doc(), threads(), t)}${own().some((o) => coaxialThreads(t, o)) ? tl(" (en el eje)") : ""}`;
  const asking = () => !props.value && candidates().length > 0 && !threadDeclined().includes(props.feature) && (props.askAlways || !!suggested());
  const picking = () => (props.ui.pick() as { owner?: string }).owner === owner();
  const pick = () => {
    if (picking()) return props.ui.cancelPick();
    void props.store.setSelecting(true);
    props.ui.setPick({
      kind: "face",
      owner: owner(),
      prompt: tl("Clic en la rosca con la que se coordina: el agujero, el tornillo, la tuerca o el eje"),
      done: (face) => {
        void props.store.setSelecting(false);
        const t = threadOfFace(face, candidates());
        if (t) choose(t);
        else props.ui.setMessage(props.want === "internal" ? tl("Ahí no hay una rosca interior anterior a esta operación") : props.want === "external" ? tl("Ahí no hay una rosca exterior anterior a esta operación") : tl("Ahí no hay una rosca anterior a esta operación"));
      },
    });
  };
  onCleanup(() => {
    if (picking()) props.ui.cancelPick();
  });
  const pickButton = (text: string) => (
    <Button size="sm" variant={picking() ? "primary" : "default"} onClick={pick}>
      {text}
    </Button>
  );
  return (
    <Show
      when={!asking()}
      fallback={
        <div class="space-y-2 rounded border border-accent/60 bg-accent/10 p-2 text-xs" data-thread-question>
          <p class="text-text">
            {suggested() ? tl("¿Coordinar {what} con la rosca de {v1}?", { what: props.what, v1: threadLabel(doc(), threads(), suggested()!) }) : tl("¿Coordinar {what} con una rosca existente?", { what: props.what })}
          </p>
          <p class="text-[11px] text-text-dim">{tl("Toma su medida y su eje y alinea el filete para que calcen. No las une: cada una sigue siendo lo que es.")}</p>
          <Show when={!suggested()}>
            <Select options={candidates().map((t) => ({ value: key(t), label: label(t) }))} value="" placeholder={tl("Elegir la rosca")} onChange={(v) => choose(candidates().find((t) => key(t) === v))} />
          </Show>
          <div class="flex flex-wrap gap-1">
            <Show when={suggested()}>
              {(t) => (
                <Button size="sm" variant="primary" onClick={() => choose(t())}>
                  {tl("Coordinar")}
                </Button>
              )}
            </Show>
            {pickButton(tl("Elegir en el visor"))}
            <Button size="sm" variant="ghost" onClick={() => setThreadDeclined((d) => [...d, props.feature])}>
              No
            </Button>
          </div>
        </div>
      }
    >
      <div class="space-y-1">
        <Row label={tl("Rosca asociada")}>
          <Select
            options={[{ value: "", label: tl("Ninguna") }, ...candidates().map((t) => ({ value: key(t), label: label(t) })), ...(props.value && !source() ? [{ value: key(props.value), label: tl("(ya no está)") }] : [])]}
            value={props.value ? key(props.value) : ""}
            onChange={(v) => choose(v ? candidates().find((t) => key(t) === v) : undefined)}
          />
        </Row>
        <Show when={props.value || candidates().length > 0}>
          <div class="flex justify-end">{pickButton(props.value ? tl("Elegir otra en el visor") : tl("Elegir en el visor"))}</div>
        </Show>
        <Show when={source()}>
          {(t) => (
            <p class="text-[11px] text-text-dim">
              {threadName(t().spec)} × {String(t().spec.pitch).replace(".", ",")}{tl(": medida, eje y fase del filete de")} {threadLabel(doc(), threads(), t()).split(" · ")[0]}{tl(". Si cambia, esta lo sigue.")}
            </p>
          )}
        </Show>
      </div>
    </Show>
  );
};

const SelectionBox: Component<{
  store: CadStore;
  ui: CadUi;
  /** Identifica la caja (operación y campo) */
  owner: string;
  kind: "edges" | "faces" | "face";
  label: string;
  refs: (EdgeRef | FaceRef)[];
  /** Referencias que el último cálculo no encontró (posiciones en `refs`) */
  lost?: number[];
  /**
   * Redondeo y chaflán: se elige sobre la vista previa de esta operación (se
   * ve cómo queda a medida que se suman o quitan aristas, y escribir la medida
   * no deja de elegir)
   */
  live?: number;
  /** Se puede dejar vacía: al abrir el diálogo no pide nada */
  optional?: boolean;
  onChange: (refs: (EdgeRef | FaceRef)[]) => void;
}> = (props) => {
  const ui = props.ui;
  const active = () => (ui.pick() as { owner?: string }).owner === props.owner;
  const [hover, setHover] = createSignal<number>();
  // Índice en el sólido mostrado de cada referencia (solo con la caja activa:
  // es el sólido de antes de la operación)
  const [resolved, setResolved] = createSignal<(number | null)[]>([]);
  let seq = 0;
  createEffect(() => {
    const refs = props.refs;
    const version = props.store.result()?.version;
    // En vivo lo elegido ya está redondeado: no hay aristas que resolver
    if (!active() || version === undefined || props.live !== undefined) return setResolved([]);
    const n = ++seq;
    const edges = props.kind === "edges";
    void props.store.resolveRefs(edges ? [] : (refs as FaceRef[]), edges ? (refs as EdgeRef[]) : []).then((r) => {
      if (n === seq) setResolved(edges ? r.edges : r.faces);
    });
  });
  // Resaltado: todo lo de la caja activa, o el ítem bajo el mouse
  createEffect(() => {
    if (!active()) return;
    const h = hover();
    const list = resolved().filter((x, i): x is number => x !== null && (h === undefined || i === h));
    ui.setHighlight(props.kind === "edges" ? { faces: [], edges: list } : { faces: list, edges: [] });
  });
  createEffect(
    on(active, (now, before) => {
      if (now || !before) return;
      ui.setHighlight({ faces: [], edges: [] });
      if (ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );

  // Con la caja activa manda lo que se ve (el sólido de antes de la operación);
  // si no, lo que dijo el último cálculo
  const isLost = (i: number) => (active() && props.live === undefined ? resolved()[i] === null : (props.lost ?? []).includes(i));
  const toggleRef = (ref: EdgeRef | FaceRef, index: number) => {
    const at = resolved().indexOf(index);
    if (at >= 0) return props.onChange(props.refs.filter((_, i) => i !== at));
    // Lo nuevo reemplaza a la primera referencia perdida
    const gone = props.refs.findIndex((_, i) => resolved()[i] === null);
    props.onChange(gone >= 0 ? props.refs.map((r, i) => (i === gone ? ref : r)) : [...props.refs, ref]);
  };
  /** En vivo: la arista cuya recta pasa más cerca de `p` (lo redondeado está junto a ella) */
  const removeNear = (p: P3) => {
    const refs = props.refs as EdgeRef[];
    const d = (r: EdgeRef) => {
      const v = [p[0] - r.point[0], p[1] - r.point[1], p[2] - r.point[2]];
      const n = Math.hypot(...r.direction) || 1;
      const along = (v[0] * r.direction[0] + v[1] * r.direction[1] + v[2] * r.direction[2]) / n;
      const all = Math.hypot(...v);
      return Math.sqrt(Math.max(0, all * all - along * along)) + 0.1 * all;
    };
    let best = -1;
    refs.forEach((r, i) => (best < 0 || d(r) < d(refs[best]) ? (best = i) : 0));
    if (best >= 0) props.onChange(refs.filter((_, i) => i !== best));
  };
  const same = (a: EdgeRef, b: EdgeRef) => Math.hypot(a.point[0] - b.point[0], a.point[1] - b.point[1], a.point[2] - b.point[2]) < 1e-6;
  /** En vivo: `fresh` ocupa primero los lugares de las referencias perdidas y lo que sobra va al final */
  const addLive = (refs: EdgeRef[], fresh: EdgeRef[]) => {
    const lost = (props.lost ?? []).filter((i) => i < refs.length).sort((a, c) => a - c);
    const next = [...refs];
    const rest = [...fresh];
    for (const i of lost) if (rest.length) next[i] = rest.shift()!;
    return [...next, ...rest];
  };
  const activate = () => {
    const base = { prompt: PROMPTS[props.kind], owner: props.owner };
    const live = props.live;
    if (props.kind === "edges" && live !== undefined) {
      return ui.setPick({
        ...base,
        prompt: tl("Clic en las aristas o en una cara (todas sus aristas); clic en lo redondeado lo quita"),
        kind: "edges",
        toggle: (r) => {
          const refs = props.refs as EdgeRef[];
          // Lo nuevo reemplaza a la primera referencia perdida
          if (!refs.some((x) => same(x, r))) props.onChange(addLive(refs, [r]));
        },
        live: {
          feature: live,
          remove: removeNear,
          // Una cara: suma sus aristas; si ya estaban todas, las quita
          face: (edges) => {
            const refs = props.refs as EdgeRef[];
            const fresh = edges.filter((e) => !refs.some((x) => same(x, e)));
            props.onChange(fresh.length ? addLive(refs, fresh) : refs.filter((x) => !edges.some((e) => same(x, e))));
          },
        },
      });
    }
    void props.store.setSelecting(true);
    if (props.kind === "edges") ui.setPick({ ...base, kind: "edges", toggle: (r, i) => toggleRef(r, i) });
    else if (props.kind === "faces") ui.setPick({ ...base, kind: "faces", toggle: (r, i) => toggleRef(r, i) });
    else ui.setPick({ ...base, kind: "face", done: (r) => props.onChange([r]) });
  };
  // Vacía o con referencias perdidas al abrirse el diálogo: se empieza eligiendo
  // (al editar, el borrador se abre después de montar la caja). En una operación
  // nueva con aristas o caras ya elegidas, igual queda activa para sumar más,
  // salvo que otra caja del diálogo esté pidiendo algo. Una caja opcional
  // vacía no se activa sola (se quedaría con los clics de la principal)
  const isOpen = createMemo(() => !!props.store.draft());
  createEffect(
    on(
      isOpen,
      (open, wasOpen) => {
        if (!open) return;
        if (untrack(() => props.live !== undefined || (props.refs.length === 0 && !props.optional) || (props.lost ?? []).length > 0)) activate();
        else if (!wasOpen && props.kind !== "face" && untrack(props.store.draft)?.isNew) queueMicrotask(() => ui.pick().kind === "none" && props.store.draft() && activate());
      },
    ),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });

  return (
    <div class="space-y-1">
      <span class="text-xs text-text-muted">{props.label}</span>
      <div
        aria-label={props.label}
        data-selection-box
        class={clsx(
          "min-h-[2rem] rounded border px-1.5 py-1 cursor-pointer text-xs",
          active() ? "border-accent bg-accent/10" : "border-border bg-surface/40 hover:border-border-hover",
        )}
        onClick={() => (active() ? ui.cancelPick() : activate())}
      >
        <Show
          when={props.refs.length > 0}
          fallback={<span class="text-text-dim">{active() ? PROMPTS[props.kind] : tl("Nada elegido: clic para elegir")}</span>}
        >
          <For each={props.refs}>
            {(_, i) => (
              <div
                class="group flex items-center justify-between gap-1 rounded px-1 hover:bg-surface"
                onMouseEnter={() => setHover(i())}
                onMouseLeave={() => setHover(undefined)}
              >
                <span class={clsx(isLost(i()) ? "text-error" : "text-text")}>
                  {NOUNS[props.kind]} {i() + 1}
                  {isLost(i()) ? tl(" · no encontrada") : ""}
                </span>
                <button
                  class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-text"
                  aria-label={tl("Quitar {noun} {n}", { noun: NOUNS[props.kind].toLowerCase(), n: i() + 1 })}
                  onClick={(e) => {
                    e.stopPropagation();
                    setHover(undefined);
                    props.onChange(props.refs.filter((_, j) => j !== i()));
                  }}
                >
                  <Icons.X size={10} />
                </button>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

/** Caja de regiones de un sketch: clic dentro de una región la suma o la quita */
const RegionBox: Component<{
  ui: CadUi;
  store: CadStore;
  owner: string;
  sketch: number;
  value: RegionSelection;
  /** Regiones que el último cálculo no encontró */
  lost?: number[];
  onChange: (r: RegionSelection) => void;
}> = (props) => {
  const ui = props.ui;
  const active = () => (ui.pick() as { owner?: string }).owner === props.owner;
  const points = () => (props.value.type === "points" ? props.value.points : []);
  createEffect(
    on(active, (now, before) => {
      if (!now && before && ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });
  const activate = () => {
    void props.store.setSelecting(true);
    ui.setPick({
      kind: "region",
      owner: props.owner,
      sketch: props.sketch,
      prompt: tl("Clic dentro de las regiones a usar (otra vez para quitarla)"),
      chosen: points,
      toggle: (p) => {
        const view = props.store.sketchView(props.sketch);
        if (!view) return;
        // Región más interna bajo el clic; si ya había un punto en ella, se quita
        const hit = [...view.regions].filter((r) => regionContains(r, p)).sort((a, b) => b.depth - a.depth)[0];
        if (!hit) return;
        const list = points();
        const inside = list.filter((q) => regionContains(hit, q) && !view.regions.some((o) => o.depth > hit.depth && regionContains(o, q)));
        const next = inside.length ? list.filter((q) => !inside.includes(q)) : [...list, hit.sample];
        props.onChange(next.length ? { type: "points", points: next } : { type: "all" });
      },
    });
  };
  return (
    <div class="space-y-1">
      <span class="text-xs text-text-muted">{tl("Regiones")}</span>
      <div
        aria-label={tl("Regiones")}
        data-selection-box
        class={clsx(
          "min-h-[2rem] rounded border px-1.5 py-1 cursor-pointer text-xs",
          active() ? "border-accent bg-accent/10" : "border-border bg-surface/40 hover:border-border-hover",
        )}
        onClick={() => (active() ? ui.cancelPick() : activate())}
      >
        <Show when={points().length > 0} fallback={<span class="text-text-dim">{tl("Todas")}{active() ? tl(" · clic en las regiones a usar") : ""}</span>}>
          <For each={points()}>
            {(_, i) => (
              <div class="group flex items-center justify-between gap-1 rounded px-1 hover:bg-surface">
                <span class={(props.lost ?? []).includes(i()) ? "text-error" : "text-text"}>
                  {tl("Región")} {i() + 1}
                  {(props.lost ?? []).includes(i()) ? tl(" · no encontrada") : ""}
                </span>
                <button
                  class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-text"
                  aria-label={tl("Quitar región {v1}", { v1: i() + 1 })}
                  onClick={(e) => {
                    e.stopPropagation();
                    const next = points().filter((_, j) => j !== i());
                    props.onChange(next.length ? { type: "points", points: next } : { type: "all" });
                  }}
                >
                  <Icons.X size={10} />
                </button>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

const ToolChecklist: Component<{ tools: Feature[]; value: number[]; onChange: (ids: number[]) => void }> = (props) => (
  <div class="space-y-1">
    <span class="text-xs text-text-muted">{tl("Qué repetir (nada = todo el sólido)")}</span>
    <For each={props.tools} fallback={<p class="text-[11px] text-text-dim">{tl("No hay operaciones con herramienta")}</p>}>
      {(t) => (
        <Checkbox
          small
          label={t.name}
          checked={props.value.includes(t.id)}
          onChange={(c) => props.onChange(c ? [...props.value, t.id] : props.value.filter((x) => x !== t.id))}
        />
      )}
    </For>
  </div>
);

// ─── Ensamble ─────────────────────────────────────────────────────────────

const deg = (r: number) => (r * 180) / Math.PI;

/** Instancias de las piezas, relaciones entre ellas, grados libres y choques */
const AssemblyPanel: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly ?? null;
  const parts = () => store.result()?.parts ?? [];
  const sol = () => store.result()?.assembly?.solution;
  const [part, setPart] = createSignal<string>("");
  const [kind, setKind] = createSignal<MateKind>("revolute");
  const [picking, setPicking] = createSignal<"" | "a" | "b">("");
  const [hits, setHits] = createSignal<[number, number, number][] | null>(null);
  const [gaps, setGaps] = createSignal<[number, number, number][] | null>(null);
  const [gapError, setGapError] = createSignal("");
  const partKey = (p: PartView) => `${p.id.feature}:${p.id.index}`;
  const instName = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
  const [note, setNote] = createSignal<string>();
  // En una pestaña de ensamble, las configuraciones del diseño de cada instancia
  const [configs] = createResource(
    () => (store.activeTab()?.kind === "assembly" ? JSON.stringify([...new Set((asm()?.instances ?? []).map(instanceStudio))].sort((a, b) => a - b)) : ""),
    async (key) => {
      const out: Record<number, string[]> = {};
      for (const id of JSON.parse(key) as number[]) {
        const d = await invoke<CadDocument | null>("cad_tab_document", { id }).catch(() => null);
        out[id] = (d?.configurations ?? []).map((c) => c.name);
      }
      return out;
    },
  );
  /** Otra configuración: el ensamble trae esa variante de la pieza (y la instancia, su número nuevo) */
  const setConfiguration = (id: number, name: string) =>
    void Promise.resolve(store.editAssembly((a) => (a.instances.find((i) => i.id === id)!.configuration = name || undefined))).then(() => store.loadActive());
  // Animación: la unión que se recorre, de dónde a dónde y cuántos cuadros
  const movable = () => (asm()?.mates ?? []).filter((m) => m.kind === "revolute" || m.kind === "slider" || m.kind === "cylindrical" || m.kind === "pin_slot");
  const [animMate, setAnimMate] = createSignal<number>();
  const [animFrom, setAnimFrom] = createSignal(0);
  const [animTo, setAnimTo] = createSignal(90);
  const [animFrames, setAnimFrames] = createSignal(30);
  const exportAnimation = async () => {
    const m = movable().find((x) => x.id === (animMate() ?? movable()[0]?.id));
    if (!m) return;
    const path = await save({ filters: [{ name: "GLB", extensions: ["glb"] }], defaultPath: "ensamble_animado.glb" });
    if (!path) return;
    try {
      const bytes = await store.exportAssemblyAnimation(path, m.id, m.kind === "slider" ? "distance" : "angle", animFrom(), animTo(), animFrames());
      props.ui.setMessage(tl("Exportado ({kb} KB)", { kb: Math.max(1, Math.round(bytes / 1024)) }));
    } catch (e) {
      props.ui.setMessage(String(e));
    }
  };

  const insert = () => {
    const p = parts().find((x) => partKey(x) === (part() || (parts()[0] && partKey(parts()[0]))));
    if (!p) return props.ui.setMessage(tl("El diseño todavía no tiene piezas"));
    void store.editAssembly((a) => {
      const id = a.next_id || 1;
      const copies = a.instances.filter((i) => samePart(i.part, p.id)).length;
      // Las nuevas, corridas a un costado para que no se pisen
      const size = Math.cbrt(Math.max(p.volume, 1)) * 1.5;
      a.instances.push({
        id,
        part: p.id,
        name: copies ? `${p.name} (${copies + 1})` : p.name,
        position: [a.instances.length * size, 0, 0],
        rotation: [0, 0, 0],
        fixed: a.instances.length === 0,
      });
      a.next_id = id + 1;
    });
  };

  /** Dos clics: la cara de una instancia y la de otra; con eso, la relación */
  const newMate = () => {
    let first: Connector | null = null;
    const pick = (step: "a" | "b") => {
      setPicking(step);
      props.ui.setPick({
        kind: "asm_face",
        prompt: step === "a" ? tl("Clic en la cara de la primera pieza (plana o cilíndrica)") : tl("Clic en la cara de la segunda pieza"),
        done: async (face) => {
          try {
            const c = await store.assemblyConnector(face);
            if (step === "a") {
              first = c;
              return pick("b");
            }
            setPicking("");
            if (!first || first.instance === c.instance) return props.ui.setMessage(tl("Las dos caras tienen que ser de instancias distintas"));
            const a = first;
            void store.editAssembly((x) => {
              const id = x.next_id || 1;
              // Caras planas enfrentadas: normales opuestas
              const facing = a.z[0] * c.z[0] + a.z[1] * c.z[1] + a.z[2] * c.z[2] < 0;
              x.mates.push({ id, name: `${MATE_LABELS[kind()]} ${x.mates.length + 1}`, kind: kind(), a, b: c, flip: kind() === "planar" || kind() === "fastened" ? facing : false });
              x.next_id = id + 1;
            });
          } catch (e) {
            setPicking("");
            props.ui.setMessage(String(e));
          }
        },
      });
    };
    pick("a");
  };

  const removeInstance = (id: number) =>
    void store.editAssembly((a) => {
      a.instances = a.instances.filter((i) => i.id !== id);
      a.mates = a.mates.filter((m) => m.a.instance !== id && m.b.instance !== id);
    });

  return (
    <div class="space-y-3">
      <Section title={tl("Instancias")}>
        <div class="flex gap-1.5">
          <div class="flex-1">
            <Select
              options={parts().map((p) => ({ value: partKey(p), label: p.name }))}
              value={part() || (parts()[0] ? partKey(parts()[0]) : "")}
              onChange={setPart}
            />
          </div>
          <Button size="sm" onClick={insert} disabled={!parts().length}>
            {tl("Insertar")}
          </Button>
        </div>
        <For each={asm()?.instances ?? []} fallback={<p class="text-[11px] text-text-dim">{tl("Insertar piezas del diseño para ensamblarlas")}</p>}>
          {(inst) => {
            // Su pieza ya no está (se borró en su diseño, o se borró la pestaña): no se dibuja
            const orphan = () => !!store.result() && !parts().some((p) => samePart(p.id, inst.part));
            return (
              <div class={clsx("space-y-1 rounded border p-1.5", orphan() ? "border-error/70 bg-error/5" : "border-border")} data-instance={inst.id} data-orphan={orphan() || undefined}>
                <div class="flex items-center gap-1.5 text-xs">
                  <span class={clsx("flex-1 truncate", orphan() ? "text-error" : "text-text")}>{inst.name}</span>
                  <Checkbox
                    small
                    label={tl("Fija")}
                    checked={inst.fixed}
                    onChange={(c) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.fixed = c))}
                  />
                  <IconButton aria-label={tl("Quitar {name}", { name: inst.name })} size="sm" variant="ghost" onClick={() => removeInstance(inst.id)}>
                    <Icons.Trash size={11} />
                  </IconButton>
                </div>
                <Show when={(configs()?.[instanceStudio(inst)]?.length ?? 0) > 0 || inst.configuration}>
                  <Row label={tl("Configuración")}>
                    <Select
                      options={[{ value: "", label: tl("La de base") }, ...(configs()?.[instanceStudio(inst)] ?? []).map((c) => ({ value: c, label: c }))]}
                      value={inst.configuration ?? ""}
                      onChange={(v) => setConfiguration(inst.id, v)}
                    />
                  </Row>
                </Show>
                <Show when={orphan()}>
                  <p class="text-[11px] text-error">{tl("Su pieza ya no está (se borró en su diseño o se borró su pestaña): no se dibuja ni se mueve.")}</p>
                </Show>
                <Show when={asm() && jointsOf(asm()!, inst.id).length > 1}>
                  {(_) => {
                    const joints = () => jointsOf(asm()!, inst.id);
                    const label = (j: ReturnType<typeof jointsOf>[number]) => `${j.mate.name || `#${j.mate.id}`} · ${j.mode === "turn" ? tl("gira") : tl("desliza")}`;
                    return (
                      <Row label={tl("Al arrastrar mueve por")}>
                        <Select
                          options={joints().map((j) => ({ value: jointKey(j), label: label(j) }))}
                          value={props.ui.dragJoints()[inst.id] ?? jointKey(joints()[0])}
                          onChange={(v) => props.ui.setDragJoints({ ...props.ui.dragJoints(), [inst.id]: v })}
                        />
                      </Row>
                    );
                  }}
                </Show>
                <span class="text-[10px] text-text-dim">{tl("Posición (mm)")}</span>
                <div class="grid grid-cols-3 gap-1">
                  <For each={["X", "Y", "Z"]}>
                    {(axis, k) => (
                      <Num
                        label={axis}
                        value={inst.position[k()]}
                        onCommit={(v) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.position[k()] = v))}
                      />
                    )}
                  </For>
                </div>
                <span class="text-[10px] text-text-dim">{tl("Giro (°)")}</span>
                <div class="grid grid-cols-3 gap-1">
                  <For each={["X", "Y", "Z"]}>
                    {(axis, k) => (
                      <Num
                        label={axis}
                        value={Math.round(deg(inst.rotation[k()]) * 1000) / 1000}
                        onCommit={(v) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.rotation[k()] = (v * Math.PI) / 180))}
                      />
                    )}
                  </For>
                </div>
              </div>
            );
          }}
        </For>
      </Section>
      <Section title={tl("Animación")}>
        <Show when={movable().length > 0} fallback={<p class="text-[11px] text-text-dim">{tl("Hace falta una bisagra, deslizante, cilíndrica o pasador en ranura para animar.")}</p>}>
          <Select
            options={movable().map((m) => ({ value: String(m.id), label: m.name || MATE_LABELS[m.kind] }))}
            value={String(animMate() ?? movable()[0].id)}
            onChange={(v) => setAnimMate(+v)}
          />
          <div class="flex gap-1.5">
            <Num label={tl("Desde")} value={animFrom()} onCommit={setAnimFrom} />
            <Num label={tl("Hasta")} value={animTo()} onCommit={setAnimTo} />
          </div>
          <Num label={tl("Cuadros")} value={animFrames()} step={1} min={2} onCommit={(v) => setAnimFrames(Math.max(2, Math.round(v)))} />
          <Button size="sm" fullWidth onClick={exportAnimation}>
            {tl("Exportar GLB animado")}
          </Button>
          <p class="text-[11px] text-text-dim">{tl("Bisagras: grados; deslizantes: mm. Cada pieza es un nodo que se mueve cuadro a cuadro.")}</p>
        </Show>
      </Section>
      <Section title={tl("Vista explosionada")}>
        <div class="flex items-center gap-2 text-[11px]">
          <Slider value={props.ui.assemblyExplode()} min={0} max={2} step={0.05} onChange={(v) => props.ui.setAssemblyExplode(v)} />
          <span class="w-10 text-right font-mono">{Math.round(props.ui.assemblyExplode() * 100)}%</span>
        </div>
        <p class="text-[11px] text-text-dim">{tl("Aleja las piezas del centro para ver cómo se arma. Solo en pantalla: no cambia el ensamble ni lo que se exporta.")}</p>
      </Section>
      <Section title={tl("Relaciones")}>
        <div class="flex gap-1.5">
          <div class="flex-1">
            <Select options={(Object.keys(MATE_LABELS) as MateKind[]).map((k) => ({ value: k, label: MATE_LABELS[k] }))} value={kind()} onChange={(v) => setKind(v as MateKind)} />
          </div>
          <Button size="sm" disabled={(asm()?.instances.length ?? 0) < 2 || !!picking()} onClick={newMate}>
            {picking() === "a" ? tl("Cara 1…") : picking() === "b" ? tl("Cara 2…") : tl("Agregar")}
          </Button>
        </div>
        <For each={asm()?.mates ?? []} fallback={<p class="text-[11px] text-text-dim">{tl("Agregar: elegir el tipo y hacer clic en una cara de cada pieza")}</p>}>
          {(m) => {
            const set = (f: (x: Mate) => void) => void store.editAssembly((a) => f(a.mates.find((x) => x.id === m.id)!));
            return (
              <div class="space-y-1 rounded border border-border p-1.5 text-xs" data-mate={m.id}>
                <div class="flex items-center gap-1.5">
                  <span class="flex-1 truncate text-text" title={`${instName(m.a.instance)} ↔ ${instName(m.b.instance)}`}>
                    {m.name}
                  </span>
                  <Checkbox small label={tl("Invertir")} checked={m.flip} onChange={(c) => set((x) => (x.flip = c))} />
                  <IconButton aria-label={tl("Quitar {name}", { name: m.name })} size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.mates = a.mates.filter((x) => x.id !== m.id)))}>
                    <Icons.Trash size={11} />
                  </IconButton>
                </div>
                <p class="text-[11px] text-text-dim">
                  {instName(m.a.instance)} ↔ {instName(m.b.instance)}
                </p>
                <Show when={m.kind === "revolute" || m.kind === "cylindrical" || m.kind === "pin_slot"}>
                  <div class="flex items-center gap-1.5">
                    <Checkbox small label={tl("Ángulo")} checked={m.angle != null} onChange={(c) => set((x) => (x.angle = c ? 0 : null))} />
                    <Show when={m.angle != null}>
                      <div class="flex-1">
                        <Slider value={m.angle ?? 0} min={-180} max={180} step={1} onChange={(v) => set((x) => (x.angle = v))} />
                      </div>
                      <span class="w-10 text-right font-mono">{Math.round(m.angle ?? 0)}°</span>
                    </Show>
                  </div>
                </Show>
                <Show when={m.kind === "slider" || m.kind === "cylindrical" || m.kind === "pin_slot"}>
                  <div class="flex items-center gap-1.5">
                    <Checkbox small label={tl("Distancia")} checked={m.distance != null} onChange={(c) => set((x) => (x.distance = c ? 0 : null))} />
                    <Show when={m.distance != null}>
                      <div class="flex-1">
                        <Num value={m.distance ?? 0} suffix="mm" onCommit={(v) => set((x) => (x.distance = v))} />
                      </div>
                    </Show>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
        <div class="space-y-1 border-t border-border pt-1.5" data-relations>
          <Tooltip content={tl("Con un sol, satélites, corona y portasatélites creados con «Armar un planetario»: pone la corona fija, el sol y el portasatélites girando en ella, y cada satélite girando en el portasatélites, con sus acoplamientos")}>
            <Button
              size="sm"
              onClick={async () => setNote(await assemblePlanetary(store))}
            >
              {tl("Armar el planetario en el ensamble")}
            </Button>
          </Tooltip>
          <div class="flex items-center gap-1.5">
            <span class="flex-1 text-[11px] text-text-dim">{tl("Acoplamientos (engranes, cremallera, tornillo)")}</span>
            <Tooltip content={tl("Busca los engranes enlazados que están en el ensamble y acopla sus uniones con la razón real de dientes")}>
              <Button
                size="sm"
                disabled={(asm()?.mates.length ?? 0) < 2}
                onClick={() =>
                  void Promise.all([store.assemblySuggest(), store.assemblyGearWarnings()]).then(([found, warnings]) => {
                    const alert = warnings.map(tBackend).join(" ");
                    if (found.length === 0) return setNote(alert || tl("No hay engranes enlazados con una unión giratoria cada uno"));
                    setNote(alert || undefined);
                    void store.editAssembly((a) => {
                      a.relations = [...(a.relations ?? []), ...found];
                    });
                  })
                }
              >
                {tl("Detectar engranes")}
              </Button>
            </Tooltip>
          </div>
          <Show when={note()}>{(n) => <p class="text-[11px] text-warning">{n()}</p>}</Show>
          <For each={asm()?.relations ?? []}>
            {(r, i) => {
              const mateName = (id: number) => asm()?.mates.find((m) => m.id === id)?.name ?? `#${id}`;
              const text = () =>
                r.type === "gear"
                  ? tl("{a} ↔ {b}: razón {ratio}", { a: mateName(r.a), b: mateName(r.b), ratio: fmt(r.ratio, 4) })
                  : r.type === "rack_pinion"
                    ? tl("{a} ↔ {b}: {mm} mm por vuelta", { a: mateName(r.revolute), b: mateName(r.slider), mm: fmt(Math.abs(r.per_turn), 3) })
                    : tl("{a}: paso {mm} mm", { a: mateName(r.cylindrical), mm: fmt(r.pitch, 3) });
              return (
                <div class="flex items-center gap-1.5 rounded border border-border px-1.5 py-1 text-xs" data-relation={i()}>
                  <span class="flex-1 truncate" title={text()}>{text()}</span>
                  <IconButton aria-label={tl("Quitar acoplamiento")} size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.relations = (a.relations ?? []).filter((_, k) => k !== i())))}>
                    <Icons.Trash size={11} />
                  </IconButton>
                </div>
              );
            }}
          </For>
          <RelationAdder mates={asm()?.mates ?? []} onAdd={(r) => void store.editAssembly((a) => (a.relations = [...(a.relations ?? []), r]))} />
          <p class="text-[11px] text-text-dim">{tl("Con el ángulo de una unión marcado, la otra lo sigue.")}</p>
        </div>
        <Show when={sol()}>
          {(s) => (
            <p class={clsx("text-[11px]", s().converged ? "text-text-dim" : "text-warning")} aria-label={tl("Estado del ensamble")}>
              {s().converged
                ? s().dof === 0
                  ? tl("Todo definido: no queda nada que se mueva")
                  : s().dof === 1
                    ? tl("Queda 1 grado libre")
                    : tl("Quedan {dof} grados libres", { dof: s().dof })
                : tl("No se pueden cumplir todas las relaciones (desvío {v1})", { v1: fmt(s().residual, 3) })}
            </p>
          )}
        </Show>
      </Section>
      <Show when={(asm()?.instances.length ?? 0) > 0}>
        <BomSection store={store} />
        {/* En su propia pestaña, el plano del ensamble: las instancias en su lugar, la lista y los globos */}
        <Show when={store.activeTab()?.kind === "assembly"}>
          <Button size="sm" fullWidth onClick={() => props.ui.setDrawingOpen(true)}>
            {tl("Plano 2D del ensamble (lista de materiales y globos)")}
          </Button>
        </Show>
      </Show>
      <Section title={tl("Choques")}>
        <Button size="sm" fullWidth disabled={(asm()?.instances.length ?? 0) < 2} onClick={() => void store.assemblyInterference().then(setHits)}>
          {tl("Revisar choques")}
        </Button>
        <Show when={hits()}>
          {(list) => (
            <For each={list()} fallback={<p class="text-[11px] text-success">{tl("Ninguna pieza se pisa con otra")}</p>}>
              {([a, b, v]) => (
                <p class="text-[11px] text-warning" data-hit>
                  {instName(a)} y {instName(b)}: {fmt(v / 1000, 3)} {tl("cm³ en común")}
                </p>
              )}
            </For>
          )}
        </Show>
        <Button
          size="sm"
          fullWidth
          disabled={(asm()?.instances.length ?? 0) < 2}
          onClick={() =>
            void store
              .assemblyClearance()
              .then((g) => {
                setGapError("");
                setGaps(g);
              })
              .catch((e) => {
                setGaps(null);
                setGapError(String(e));
              })
          }
        >
          {tl("Revisar holguras")}
        </Button>
        <Show when={gapError()}>
          <p class="text-[11px] text-warning">{gapError()}</p>
        </Show>
        <Show when={gaps()}>
          {(list) => (
            <For each={list()} fallback={<p class="text-[11px] text-success">{tl("Todas las piezas guardan la holgura de la impresora")}</p>}>
              {([a, b, d]) => (
                <p class="text-[11px] text-warning" data-gap>
                  {instName(a)} y {instName(b)}: {tl("{v1} mm de holgura", { v1: fmt(d, 2) })}
                </p>
              )}
            </For>
          )}
        </Show>
      </Section>
    </div>
  );
};

/** Lista de materiales del ensamble: piezas, cantidades, material y masa; exportable a CSV */
const BomSection: Component<{ store: CadStore }> = (props) => {
  const rows = createMemo(() => {
    const doc = props.store.doc();
    const parts = props.store.result()?.parts ?? [];
    const out: { name: string; qty: number; material: string; mass: number | null; volume: number }[] = [];
    for (const inst of doc?.assembly?.instances ?? []) {
      const p = parts.find((x) => samePart(x.id, inst.part));
      if (!p) continue;
      const row = out.find((r) => r.name === p.name);
      if (row) row.qty += 1;
      else out.push({ name: p.name, qty: 1, material: partMaterial(doc, p)?.name ?? "", mass: partMass(doc, p), volume: p.volume });
    }
    return out;
  });
  const total = () => (rows().every((r) => r.mass !== null) ? rows().reduce((a, r) => a + r.mass! * r.qty, 0) : null);
  const exportCsv = async () => {
    const path = await save({ filters: [{ name: "CSV", extensions: ["csv"] }], defaultPath: "lista_de_materiales.csv" });
    if (!path) return;
    const q = (s: string) => `"${s.replace(/"/g, '""')}"`;
    const lines = [
      "Elemento;Pieza;Cantidad;Material;Masa unitaria (g);Volumen unitario (cm³)",
      ...rows().map((r, i) => [i + 1, q(r.name), r.qty, q(r.material), r.mass === null ? "" : r.mass.toFixed(2).replace(".", ","), (r.volume / 1000).toFixed(3).replace(".", ",")].join(";")),
    ];
    await invoke("cad_write_text", { path, content: lines.join("\n") + "\n" });
  };
  return (
    <Section title={tl("Lista de materiales")}>
      <table class="w-full text-[11px]" aria-label={tl("Lista de materiales")}>
        <thead>
          <tr class="text-text-dim">
            <th class="text-left font-normal">{tl("Pieza")}</th>
            <th class="text-right font-normal">{tl("Cant.")}</th>
            <th class="text-left font-normal pl-2">{tl("Material")}</th>
            <th class="text-right font-normal">{tl("Masa")}</th>
          </tr>
        </thead>
        <tbody>
          <For each={rows()}>
            {(r) => (
              <tr class="text-text" data-bom-row>
                <td class="truncate max-w-[7rem]">{r.name}</td>
                <td class="text-right font-mono">{r.qty}</td>
                <td class="pl-2 truncate max-w-[5rem]">{r.material || "—"}</td>
                <td class="text-right font-mono">{r.mass === null ? "—" : fmtMass(r.mass * r.qty)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={total() !== null}>
        <p class="text-xs text-text">{tl("Masa total:")} {fmtMass(total()!)}</p>
      </Show>
      <Button size="sm" variant="ghost" fullWidth onClick={() => void exportCsv()}>
        {tl("Exportar CSV")}
      </Button>
    </Section>
  );
};

// ─── Escaneo ──────────────────────────────────────────────────────────────

/** Comparar el escaneo con el diseño: colores en el visor y resumen */
const DeviationCard: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const [tolerance, setTolerance] = createSignal(0.2);
  // Calce: holgura pedida entre la pieza y la piel del escaneo
  const [calce, setCalce] = createSignal(false);
  const [gapMin, setGapMin] = createSignal(0.1);
  const [gapMax, setGapMax] = createSignal(0.5);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const run = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const dev = await props.store.deviation(calce() ? Math.max(gapMax(), 0.001) : tolerance());
      props.ui.setFit(calce() ? { min: gapMin(), max: Math.max(gapMax(), gapMin()) } : null);
      props.ui.setDeviation(dev);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  // Si cambia el diseño, los colores viejos ya no valen
  createEffect(on(() => props.store.result()?.version, () => (props.ui.setDeviation(null), props.ui.setFit(null)), { defer: true }));
  const d = () => props.ui.deviation();
  return (
    <div class="space-y-1.5 rounded-md border border-border p-2" aria-label={tl("Comparar con el escaneo")}>
      <span class="text-xs text-text-muted">{tl("Comparar el diseño con el escaneo")}</span>
      <label class="flex items-center gap-1.5 text-xs text-text-muted" title={tl("Colorea el escaneo por la holgura con la pieza: choque, apretado, en rango o suelto")}>
        <input type="checkbox" checked={calce()} onChange={(e) => setCalce(e.currentTarget.checked)} />
        {tl("Verificar el calce (holgura)")}
      </label>
      <Show
        when={calce()}
        fallback={<Num label={tl("Tolerancia")} value={tolerance()} step={0.05} suffix="mm" onCommit={(v) => setTolerance(Math.max(0.001, v))} />}
      >
        <div class="grid grid-cols-2 gap-1.5">
          <Num label={tl("Holgura mínima")} value={gapMin()} step={0.05} suffix="mm" onCommit={(v) => setGapMin(Math.max(0, v))} />
          <Num label={tl("Holgura máxima")} value={gapMax()} step={0.05} suffix="mm" onCommit={(v) => setGapMax(Math.max(0.001, v))} />
        </div>
      </Show>
      <Button size="sm" fullWidth disabled={busy() || !props.store.result()?.body} onClick={() => void run()}>
        {busy() ? tl("Midiendo…") : tl("Comparar")}
      </Button>
      <Show when={error()}>
        <p class="text-[11px] text-error">{error()}</p>
      </Show>
      <Show when={props.ui.fit() && props.ui.fitReport()}>
        {(rep) => (
          <div class="space-y-0.5 text-xs" aria-label={tl("Informe del calce")}>
            <div class="flex h-2 overflow-hidden rounded">
              <For each={(["interference", "tight", "ok", "loose"] as const).filter((k) => rep().share[k] > 0)}>
                {(k) => <div style={{ width: `${rep().share[k] * 100}%`, background: `rgb(${FIT_COLORS[k].map((c) => Math.round(c * 255)).join(",")})` }} />}
              </For>
            </div>
            <Line label={tl("Choca")}>{fmt(rep().share.interference * 100, 1)} %</Line>
            <Line label={tl("Apretado")}>{fmt(rep().share.tight * 100, 1)} %</Line>
            <Line label={tl("En rango")}>{fmt(rep().share.ok * 100, 1)} %</Line>
            <Line label={tl("Suelto")}>{fmt(rep().share.loose * 100, 1)} %</Line>
            <Line label={tl("Menor holgura")}>{fmt(rep().min.value, 3)} mm</Line>
            <p class="text-[10px] text-text-dim font-mono">{rep().min.at.map((v) => fmt(v, 1)).join(", ")}</p>
            <Line label={tl("Mayor holgura")}>{fmt(rep().max.value, 3)} mm</Line>
            <p class="text-[10px] text-text-dim font-mono">{rep().max.at.map((v) => fmt(v, 1)).join(", ")}</p>
            <Show when={rep().interference.length > 0}>
              <Button size="sm" variant="ghost" fullWidth onClick={() => props.ui.setScanHighlight(rep().interference)}>
                {tl("Marcar los choques")}
              </Button>
            </Show>
            <Button size="sm" variant="ghost" fullWidth onClick={() => (props.ui.setDeviation(null), props.ui.setFit(null))}>
              {tl("Quitar los colores")}
            </Button>
          </div>
        )}
      </Show>
      <Show when={!props.ui.fit() && d()}>
        {(dev) => (
          <div class="space-y-0.5 text-xs">
            <div class="h-2 rounded" style={{ background: "linear-gradient(to right, rgb(51,89,242), rgb(77,191,230), rgb(64,191,89), rgb(242,204,51), rgb(230,51,38))" }} />
            <div class="flex justify-between text-[10px] text-text-dim">
              <span>{tl("adentro")}</span>
              <span>± {fmt(dev().stats.tolerance)} mm</span>
              <span>{tl("afuera")}</span>
            </div>
            <Line label={tl("Dentro de tolerancia")}>{fmt(dev().stats.within * 100, 1)} %</Line>
            <Line label={tl("Media (sesgo)")}>{fmt(dev().stats.mean, 3)} mm</Line>
            <Line label={tl("Media absoluta")}>{fmt(dev().stats.meanAbs, 3)} mm</Line>
            <Line label="P95">{fmt(dev().stats.p95, 3)} mm</Line>
            <Line label={tl("Máxima")}>{fmt(dev().stats.maxAbs, 3)} mm</Line>
            <Button size="sm" variant="ghost" fullWidth onClick={() => props.ui.setDeviation(null)}>
              {tl("Quitar los colores")}
            </Button>
          </div>
        )}
      </Show>
    </div>
  );
};

const ScanResultCard: Component<{
  result: { pick: ScanPick; triangle: number };
  onAdd: (f: Parameters<CadStore["scanAdd"]>[0]) => void;
  onTurned: (origin: P3, direction: P3) => void;
  onSkin: (direction: P3, count: number) => void;
}> = (props) => {
  const [depth, setDepth] = createSignal<number>();
  const [cuts, setCuts] = createSignal(8);
  /** Eje de revolución de la zona (cilindro, cono o toro) */
  const axisOf = (): { origin: P3; direction: P3 } | undefined => {
    const p = props.result.pick;
    return p.kind === "cylinder" ? { origin: p.origin, direction: p.direction } : p.kind === "cone" ? { origin: p.apex, direction: p.direction } : p.kind === "torus" ? { origin: p.center, direction: p.axis } : undefined;
  };
  const [op, setOp] = createSignal<BodyOp>("join");
  return (
    <div class="rounded-md border border-cyan/50 p-2 space-y-2 text-xs">
      <Show
        when={props.result.pick.kind === "plane" && props.result.pick}
        fallback={(() => {
          const any = props.result.pick;
          if (any.kind === "cone")
            return (
              <>
                <p class="text-text">
                  {any.hole ? tl("Cono hueco") : tl("Cono")} {tl("· semiángulo")} {fmt((any.half_angle * 180) / Math.PI, 2)}° {tl("· largo")} {fmt(any.end - any.start)} mm
                </p>
                <p class="text-text-dim">
                  {tl("Radios")} {fmt(any.start * Math.tan(any.half_angle), 2)} → {fmt(any.end * Math.tan(any.half_angle), 2)} mm · {tl("Error")} {fmt(any.rms, 3)} mm
                </p>
                <Button size="sm" variant="primary" fullWidth onClick={() => props.onAdd({ kind: "cone", triangle: props.result.triangle })}>
                  {any.hole ? tl("Agregar el cono hueco (resta)") : tl("Agregar el cono")}
                </Button>
              </>
            );
          if (any.kind === "torus")
            return (
              <>
                <p class="text-text">
                  {tl("Toro · radio mayor")} {fmt(any.major, 3)} {tl("mm · radio menor")} {fmt(any.minor, 3)} mm
                </p>
                <p class="text-text-dim">
                  {tl("Error")} {fmt(any.rms, 3)} mm
                </p>
                <Button size="sm" variant="primary" fullWidth onClick={() => props.onAdd({ kind: "torus", triangle: props.result.triangle })}>
                  {tl("Agregar el toro")}
                </Button>
              </>
            );
          const c = any as Extract<ScanPick, { kind: "cylinder" }>;
          return (
            <>
              <p class="text-text">
                {c.hole ? tl("Agujero") : tl("Cilindro")} {tl("· radio")} {fmt(c.radius, 3)} {tl("mm · largo")} {fmt(c.length)} mm
              </p>
              <p class="text-text-dim">
                {tl("Error")} {fmt(c.rms, 3)} {tl("mm · cubre")} {fmt(c.coverage * 100, 0)} {tl("% de la vuelta")}
              </p>
              <Button size="sm" variant="primary" fullWidth onClick={() => props.onAdd({ kind: "cylinder", triangle: props.result.triangle })}>
                {c.hole ? tl("Agregar el agujero (resta)") : tl("Agregar el cilindro")}
              </Button>
            </>
          );
        })()}
      >
        {(p) => (
          <>
            <p class="text-text">
              {tl("Plano · área")} {fmt(p().area, 1)} mm² · {p().boundary.length} {tl("contornos")}
            </p>
            <p class="text-text-dim">
              {tl("Error")} {fmt(p().rms, 3)} {tl("mm · espesor medido")} {p().depth != null ? `${fmt(p().depth!)} mm` : tl("no se pudo medir")}
            </p>
            <Num label={tl("Profundidad")} suffix="mm" value={depth() ?? p().depth ?? 10} onCommit={setDepth} />
            <Row label={tl("Con el sólido")}>
              <Select options={OP_OPTIONS} value={op()} onChange={(v) => setOp(v as BodyOp)} />
            </Row>
            <div class="grid grid-cols-2 gap-1.5">
              <Button size="sm" onClick={() => props.onAdd({ kind: "plane_outline", triangle: props.result.triangle, extrude: false })}>
                {tl("Solo el sketch")}
              </Button>
              <Button
                size="sm"
                variant="primary"
                onClick={() => props.onAdd({ kind: "plane_outline", triangle: props.result.triangle, extrude: true, depth: depth() ?? p().depth ?? 10, op: op() })}
              >
                {tl("Sketch y extruir")}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => props.onAdd({ kind: "work_plane", triangle: props.result.triangle })}>
                {tl("Plano de trabajo")}
              </Button>
            </div>
          </>
        )}
      </Show>
      <Show when={axisOf()}>
        {(ax) => (
          <div class="space-y-1.5 border-t border-border pt-1.5">
            <Button
              size="sm"
              variant="ghost"
              fullWidth
              title={tl("Corta el escaneo por un plano que pasa por este eje y gira el perfil (piezas torneadas)")}
              onClick={() => props.onTurned(ax().origin, ax().direction)}
            >
              {tl("Revolución desde el corte")}
            </Button>
            <div class="flex items-end gap-1.5">
              <Num label={tl("Cortes")} value={cuts()} step={1} onCommit={(v) => setCuts(Math.max(2, Math.round(v)))} />
              <Button size="sm" variant="ghost" title={tl("Cortes paralelos perpendiculares a este eje y el recubrimiento que pasa por todos")} onClick={() => props.onSkin(ax().direction, cuts())}>
                {tl("Recubrir cortes")}
              </Button>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
};

// ─── Sólido ───────────────────────────────────────────────────────────────

/** Piezas del diseño: color, nombre, visibilidad y exportar cada una */
const PartsSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const parts = () => props.store.result()?.parts ?? [];
  const [renaming, setRenaming] = createSignal<string>();
  const key = (p: PartView) => `${p.id.feature}:${p.id.index}`;
  // Pieza abierta (sus datos y material abajo de la lista; se resalta en el visor)
  const open = props.ui.openPart;
  const setOpen = props.ui.setOpenPart;
  const opened = () => parts().find((p) => key(p) === open());
  createEffect(
    on(opened, (p, prev) => {
      if (p) props.ui.setHighlight({ faces: Array.from({ length: p.faces[1] - p.faces[0] }, (_, k) => p.faces[0] + k), edges: [] });
      else if (prev) props.ui.setHighlight({ faces: [], edges: [] });
    }),
  );
  return (
    <Show when={parts().length > 0}>
      <Section title={`Piezas (${parts().length})`}>
        <div class="space-y-0.5">
          <For each={parts()}>
            {(p, i) => {
              const hidden = () => partHidden(props.store.doc(), p);
              return (
                <div data-part={key(p)} class={clsx("group flex items-center gap-1.5 px-1 py-0.5 rounded text-xs hover:bg-surface", hidden() && "opacity-50")}>
                  <input
                    type="color"
                    aria-label={tl("Color de {name}", { name: p.name })}
                    class="w-4 h-4 shrink-0 rounded cursor-pointer bg-transparent border-0 p-0"
                    value={partColor(props.store.doc(), p, i())}
                    onChange={(e) => void props.store.setPartProps(p.id, { color: e.currentTarget.value })}
                  />
                  <Show
                    when={renaming() === key(p)}
                    fallback={
                      <span
                        class={clsx("flex-1 truncate cursor-pointer", open() === key(p) ? "text-accent" : "text-text")}
                        title={tl("Clic: ver y elegir material · doble clic: renombrar")}
                        onClick={() => setOpen(open() === key(p) ? undefined : key(p))}
                        onDblClick={() => setRenaming(key(p))}
                      >
                        {p.name}
                      </span>
                    }
                  >
                    <input
                      aria-label={tl("Nombre de la pieza")}
                      class="flex-1 min-w-0 px-1 rounded bg-surface/40 border border-accent text-xs text-text outline-none"
                      value={p.name}
                      ref={(el) => setTimeout(() => el.select())}
                      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                      onBlur={(e) => {
                        const name = e.currentTarget.value.trim();
                        setRenaming(undefined);
                        if (name && name !== p.name) void props.store.setPartProps(p.id, { name });
                      }}
                    />
                  </Show>
                  <span class="font-mono text-[10px] text-text-dim group-hover:hidden">
                    {p.surface
                      ? `superficie · ${fmt(p.area / 100)} cm²`
                      : partMass(props.store.doc(), p) !== null
                        ? fmtMass(partMass(props.store.doc(), p)!)
                        : `${fmt(p.volume / 1000)} cm³`}
                  </span>
                  <span class="hidden group-hover:flex">
                    <IconButton
                      aria-label={tl("Borrar {name}", { name: p.name })}
                      size="sm"
                      variant="ghost"
                      onClick={() => void props.store.addFeature({ type: "delete_parts", parts: [p.id] })}
                    >
                      <Icons.Trash size={11} />
                    </IconButton>
                  </span>
                  <IconButton
                    aria-label={hidden() ? tl("Mostrar {name}", { name: p.name }) : tl("Ocultar {name}", { name: p.name })}
                    size="sm"
                    variant="ghost"
                    onClick={() => void props.store.setPartProps(p.id, { hidden: !hidden() })}
                  >
                    {hidden() ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
        <Show when={opened()}>
          {(p) => {
            const own = () => props.store.doc()?.parts?.find((x) => samePart(x.part, p().id))?.material ?? null;
            return (
              <div class="space-y-1.5 rounded border border-border p-2 text-xs" aria-label={tl("Datos de {name}", { name: p().name })}>
                <MaterialPicker value={own()} none={tl("El del diseño")} onChange={(m) => void props.store.setPartProps(p().id, { material: m })} />
                <Show when={!p().surface} fallback={<Line label={tl("Tipo")}>{tl("Superficie (sin espesor)")}</Line>}>
                  <Line label={tl("Volumen")}>{fmt(p().volume / 1000)} cm³</Line>
                </Show>
                <Line label={tl("Área")}>{fmt(p().area / 100)} cm²</Line>
                <Show when={!p().surface && partMass(props.store.doc(), p()) !== null}>
                  <Line label={tl("Masa")}>
                    {fmtMass(partMass(props.store.doc(), p())!)} ({partMaterial(props.store.doc(), p())!.name})
                  </Line>
                </Show>
                <Line label={tl("Centro")}>{p().center.map((v) => fmt(v, 2)).join(", ")} mm</Line>
              </div>
            );
          }}
        </Show>
      </Section>
    </Show>
  );
};

/** Material: ninguno (o el heredado), uno de la lista o una densidad propia */
const MaterialPicker: Component<{ value: Material | null; none: string; onChange: (m: Material | null) => void }> = (props) => {
  const choice = () => {
    const m = props.value;
    if (!m) return "none";
    return MATERIALS.some((x) => x.name === m.name && x.density === m.density) ? m.name : "custom";
  };
  return (
    <>
      <Row label={tl("Material")}>
        <Select
          options={[
            { value: "none", label: props.none },
            ...MATERIALS.map((m) => ({ value: m.name, label: `${m.name} (${m.density} kg/m³)` })),
            { value: "custom", label: tl("Densidad propia") },
          ]}
          value={choice()}
          onChange={(v) => {
            if (v === choice()) return;
            if (v === "none") props.onChange(null);
            else if (v === "custom") props.onChange({ name: tl("Otro"), density: props.value?.density ?? 1000 });
            else props.onChange(MATERIALS.find((m) => m.name === v)!);
          }}
        />
      </Row>
      <Show when={choice() === "custom"}>
        <Num label={tl("Densidad")} suffix="kg/m³" step={10} value={props.value!.density} onCommit={(v) => v > 0 && props.onChange({ name: props.value!.name, density: v })} />
      </Show>
    </>
  );
};

/** "12,30 g" o "1,234 kg" */
const fmtMass = (g: number) => (g >= 1000 ? `${fmt(g / 1000, 3)} kg` : `${fmt(g, 2)} g`);

const Line = (p: { label: string; children: JSX.Element }) => (
  <div class="flex justify-between gap-2">
    <span class="text-text-muted whitespace-nowrap">{p.label}</span>
    <span class="font-mono text-text text-right">{p.children}</span>
  </div>
);

const BodySection: Component<{ store: CadStore; ui: CadUi }> = (props) => (
  <Show when={props.store.result()?.body}>
    {(b) => {
      const size = () => b().bbox_max.map((v, i) => v - b().bbox_min[i]);
      const material = () => props.store.doc()?.material ?? null;
      const setMaterial = (m: Material | null) => void props.store.commit((d) => (d.material = m));
      const total = () => designMass(props.store.doc(), props.store.result());
      return (
        <Section title={tl("Sólido")}>
          <div class="space-y-1 text-xs">
            <Line label={tl("Volumen")}>{fmt(b().volume / 1000)} cm³</Line>
            <Line label={tl("Área")}>{fmt(b().area / 100)} cm²</Line>
            <Line label={tl("Tamaño X × Y × Z")}>{size().map((v) => fmt(v, 1)).join(" × ")} mm</Line>
            <Line label={tl("Caras / aristas")}>
              {b().faces} / {b().edges}
            </Line>
            <Show when={!b().valid}>
              <p class="text-warning">{tl("La forma tiene defectos: puede fallar al exportar o imprimir")}</p>
            </Show>
          </div>
          <MaterialPicker value={material()} none={tl("Sin material")} onChange={setMaterial} />
          <div class="space-y-1 text-xs">
            <Show when={total().mass !== null}>
              <Line label={tl("Masa")}>{fmtMass(total().mass!)}</Line>
            </Show>
            <Line label={tl("Centro de masa")}>{(total().center ?? b().center).map((v) => fmt(v, 2)).join(", ")} mm</Line>
            {/* Con una sola densidad: kg·mm² = mm⁵ × kg/m³ × 1e−9 */}
            <Show when={total().mass !== null && total().uniform && material()}>
              <Line label={tl("Inercia")}>
                {b()
                  .inertia.map((v) => fmt(v * material()!.density * 1e-9, 3))
                  .join(" · ")}{" "}
                kg·mm²
              </Line>
            </Show>
          </div>
          <Checkbox small label={tl("Ver el centro de masa")} checked={props.ui.showCenterOfMass()} onChange={(c) => props.ui.setShowCenterOfMass(c)} />
          <Button size="sm" fullWidth onClick={() => props.ui.setDrawingOpen(true)}>
            {tl("Plano 2D (vistas, ocultas, cajetín)")}
          </Button>
        </Section>
      );
    }}
  </Show>
);

// ─── Sketch en edición ────────────────────────────────────────────────────

type SketchPatternKind = "linear" | "circular" | "curve" | "table" | "fill";

const SKETCH_PATTERNS: { value: SketchPatternKind; label: string }[] = [
  { value: "linear", get label() { return tl("Lineal"); } },
  { value: "circular", get label() { return tl("Circular"); } },
  { value: "curve", get label() { return tl("En curva"); } },
  { value: "table", get label() { return tl("Por tabla"); } },
  { value: "fill", get label() { return tl("De relleno"); } },
];

// ─── Sketch 3D ───────────────────────────────────────────────────────────────

type Tool3d = "select" | "line" | "arc" | "spline" | "point";

const TOOL3D: { id: Tool3d; label: string; prompt: string }[] = [
  { id: "line", get label() { return tl("Línea"); }, get prompt() { return tl("Clic en el plano activo, en un punto o en un vértice del sólido; cada clic sigue la línea"); } },
  { id: "arc", get label() { return tl("Arco"); }, get prompt() { return tl("Arco por tres puntos: comienzo, uno del medio y fin"); } },
  { id: "spline", get label() { return tl("Spline"); }, get prompt() { return tl("Clic en cada punto de paso; «Terminar trazo» la crea"); } },
  { id: "point", get label() { return tl("Punto"); }, get prompt() { return tl("Clic donde va el punto"); } },
  { id: "select", get label() { return tl("Elegir"); }, get prompt() { return tl("Clic en puntos y curvas para restringirlos (Mayús suma)"); } },
];

const C3D_LABELS: Record<Constraint3d["type"], string> = {
  get coincident() { return tl("Coincidentes"); },
  get fixed() { return tl("Fijo"); },
  get attach() { return tl("Sobre el modelo"); },
  get on_plane() { return tl("Sobre el plano"); },
  get along_axis() { return tl("Paralela al eje"); },
  get parallel() { return tl("Paralelas"); },
  get perpendicular() { return tl("Perpendiculares"); },
  get equal() { return tl("Mismo largo"); },
  get tangent() { return tl("Tangentes"); },
  get midpoint() { return tl("Punto medio"); },
  get length() { return tl("Largo"); },
  get distance() { return tl("Distancia"); },
  get angle() { return tl("Ángulo"); },
};

/** Ids de puntos y entidades que nombra una restricción del sketch 3D */
function ids3d(c: Constraint3d): number[] {
  return (["a", "b", "point", "line"] as const).flatMap((k) => (k in c ? [(c as Record<string, unknown>)[k] as number] : []));
}

const sub3 = (a: P3, b: P3): P3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const len3 = (a: P3) => Math.hypot(a[0], a[1], a[2]);

/** Editor del sketch 3D: dibuja con clics en el visor (sobre el plano activo o los vértices del sólido) y restringe lo elegido */
const Sketch3dEditor: Component<{ store: CadStore; ui: CadUi; feature: Feature }> = (props) => {
  const ui = props.ui;
  const id = () => props.feature.id;
  const [tool, setTool] = createSignal<Tool3d>("line");
  const [planeKey, setPlaneKey] = createSignal<"xy" | "xz" | "yz">("xy");
  const [offset, setOffset] = createSignal(0);
  // Puntos del trazo en curso (línea: el último; arco: comienzo y medio; spline: todos)
  const [trace, setTrace] = createSignal<number[]>([]);
  const view = () => props.store.result()?.sketches3d?.find((v) => v.id === id());
  const draftSketch = (): Sketch3d | undefined => {
    const f = props.store.doc()?.features.find((x) => x.id === id());
    return f?.kind.type === "sketch3d" ? f.kind.sketch : undefined;
  };
  const sketch = (): Sketch3d => view()?.sketch ?? draftSketch() ?? { points: [], entities: [], constraints: [], next_id: 1 };
  const report = () => view()?.report;
  const plane = (): Plane => {
    const b = BASE_PLANES[planeKey()];
    return { ...b, origin: b.normal.map((v) => v * offset()) as P3 };
  };
  const coords = (s: Sketch3d, pid: number): P3 | undefined => {
    const p = s.points.find((q) => q.id === pid);
    return p ? [p.x, p.y, p.z] : undefined;
  };
  /**
   * Cambia el sketch del borrador con los puntos donde los dejó el solver (lo
   * que sigue arranca de la geometría que cumple todo; si la vista previa
   * todavía no llegó, el borrador manda)
   */
  const change = (mutate: (s: Sketch3d) => void) =>
    props.store.updateFeature(id(), (f) => {
      if (f.kind.type !== "sketch3d") return;
      const s = structuredClone(f.kind.sketch);
      const solved = new Map((view()?.sketch.points ?? []).map((p) => [p.id, p]));
      for (const p of s.points) {
        const q = solved.get(p.id);
        if (q) [p.x, p.y, p.z] = [q.x, q.y, q.z];
      }
      mutate(s);
      f.kind.sketch = s;
    });
  const fresh = (s: Sketch3d) => {
    const used = Math.max(0, ...s.points.map((p) => p.id + 1), ...s.entities.map((e) => e.id + 1));
    const n = Math.max(s.next_id, used);
    s.next_id = n + 1;
    return n;
  };
  /** El punto del clic: uno que ya está o uno nuevo (atado al vértice del sólido si cayó en uno) */
  const place = (s: Sketch3d, hit: Hit3d, spec: PointSpec | undefined): number => {
    if (hit.point !== undefined && s.points.some((p) => p.id === hit.point)) return hit.point;
    const pid = fresh(s);
    s.points.push({ id: pid, x: hit.at[0], y: hit.at[1], z: hit.at[2] });
    if (spec) s.constraints.push({ type: "attach", point: pid, target: spec });
    return pid;
  };
  /** Línea nueva; si va casi por un eje, queda paralela a él (como al dibujar en un plano) */
  const addLine = (s: Sketch3d, a: number, b: number) => {
    const lid = fresh(s);
    s.entities.push({ id: lid, geometry: { type: "line", start: a, end: b } });
    const [pa, pb] = [coords(s, a), coords(s, b)];
    if (!pa || !pb) return;
    const d = sub3(pb, pa);
    const l = len3(d);
    const axis = (["x", "y", "z"] as const).find((_, k) => l > 1e-9 && Math.abs(d[k]) / l > Math.cos((3 * Math.PI) / 180));
    if (axis) s.constraints.push({ type: "along_axis", line: lid, axis });
  };
  const onClick = async (hit: Hit3d, e: PointerEvent) => {
    const t = tool();
    if (t === "select") {
      const target = hit.point ?? hit.entity;
      if (target === undefined) return ui.setSel3d([]);
      ui.setSel3d((cur) => (e.shiftKey ? (cur.includes(target) ? cur.filter((x) => x !== target) : [...cur, target]) : cur.length === 1 && cur[0] === target ? [] : [target]));
      return;
    }
    const spec = hit.vertex && hit.point === undefined ? await props.store.vertexSpec(hit.at).catch(() => undefined) : undefined;
    const tr = trace();
    let made = -1;
    // El cambio al borrador es inmediato (la vista previa llega después): el trazo sigue ya
    const pending = change((s) => {
      made = place(s, hit, spec);
      if (t === "line" && tr.length && tr[0] !== made) addLine(s, tr[0], made);
      if (t === "arc" && tr.length === 2 && !tr.includes(made)) s.entities.push({ id: fresh(s), geometry: { type: "arc", start: tr[0], mid: tr[1], end: made } });
      if (t === "point" && hit.point === undefined) s.entities.push({ id: fresh(s), geometry: { type: "point", point: made } });
    });
    if (t === "line") setTrace([made]);
    else if (t === "arc") setTrace(tr.length === 2 ? [] : [...tr, made]);
    else if (t === "spline") setTrace([...tr, made]);
    await pending;
  };
  /** Corta el trazo; una spline en curso se crea */
  const endTrace = () => {
    const tr = trace();
    if (tool() === "spline" && tr.length >= 2) void change((s) => s.entities.push({ id: fresh(s), geometry: { type: "spline", points: tr } }));
    setTrace([]);
  };
  createEffect(
    on([tool, planeKey, offset], () => {
      setTrace([]);
      if (tool() !== "select") ui.setSel3d([]);
    }),
  );
  // El visor dibuja con el plano activo; el comienzo del tramo, para la línea guía
  createEffect(() => {
    const tr = trace();
    const from = tr.length && tool() !== "select" ? coords(sketch(), tr[tr.length - 1]) : undefined;
    const t = TOOL3D.find((x) => x.id === tool())!;
    ui.setPick({ kind: "sketch3d", prompt: t.prompt, owner: `${id()}:sketch3d`, feature: id(), plane: plane(), from, click: (h, e) => void onClick(h, e) });
  });
  onCleanup(() => {
    if (ui.pick().kind === "sketch3d") ui.setPick({ kind: "none" });
    ui.setSel3d([]);
  });

  const selPoints = () => ui.sel3d().filter((x) => sketch().points.some((p) => p.id === x));
  const selEntities = () => ui.sel3d().flatMap((x) => sketch().entities.filter((e) => e.id === x));
  const selLines = () => selEntities().filter((e) => e.geometry.type === "line");
  const lineVec = (s: Sketch3d, lid: number): P3 | undefined => {
    const g = s.entities.find((e) => e.id === lid)?.geometry;
    if (g?.type !== "line") return undefined;
    const [a, b] = [coords(s, g.start), coords(s, g.end)];
    return a && b ? sub3(b, a) : undefined;
  };
  const round = (v: number) => Math.round(v * 1e4) / 1e4;
  const add = (c: Constraint3d | Constraint3d[]) => {
    void change((s) => s.constraints.push(...[c].flat()));
  };
  /** Restricciones posibles para lo elegido */
  const options = (): { label: string; make: () => Constraint3d[] }[] => {
    const s = sketch();
    const P = selPoints();
    const L = selLines().map((e) => e.id);
    const E = selEntities();
    const out: { label: string; make: () => Constraint3d[] }[] = [];
    if (P.length === 2 && E.length === 0) {
      out.push({ label: tl("Coincidentes"), make: () => [{ type: "coincident", a: P[0], b: P[1] }] });
      out.push({ label: tl("Distancia"), make: () => [{ type: "distance", a: P[0], b: P[1], value: round(len3(sub3(coords(s, P[1])!, coords(s, P[0])!))) }] });
    }
    if (P.length >= 1 && E.length === 0) {
      out.push({ label: tl("Sobre el plano activo"), make: () => P.map((p) => ({ type: "on_plane", point: p, plane: { type: planeKey() }, offset: offset() }) as Constraint3d) });
      out.push({ label: tl("Fijo"), make: () => P.map((p) => ({ type: "fixed", point: p, at: coords(s, p)! }) as Constraint3d) });
    }
    if (L.length === 1 && P.length === 0 && E.length === 1) {
      for (const axis of ["x", "y", "z"] as const) out.push({ label: tl("Paralela a {axis}", { axis: axis.toUpperCase() }), make: () => [{ type: "along_axis", line: L[0], axis }] });
      out.push({ label: tl("Largo"), make: () => [{ type: "length", line: L[0], value: round(len3(lineVec(s, L[0])!)) }] });
    }
    if (L.length === 2 && E.length === 2 && P.length === 0) {
      out.push({ label: tl("Paralelas"), make: () => [{ type: "parallel", a: L[0], b: L[1] }] });
      out.push({ label: tl("Perpendiculares"), make: () => [{ type: "perpendicular", a: L[0], b: L[1] }] });
      out.push({ label: tl("Mismo largo"), make: () => [{ type: "equal", a: L[0], b: L[1] }] });
      out.push({
        label: tl("Ángulo"),
        make: () => {
          const [u, v] = [lineVec(s, L[0])!, lineVec(s, L[1])!];
          const cos = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) / (len3(u) * len3(v) || 1);
          return [{ type: "angle", a: L[0], b: L[1], degrees: round((Math.acos(Math.max(-1, Math.min(1, cos))) * 180) / Math.PI) }];
        },
      });
    }
    if (E.length === 2 && P.length === 0 && E.every((e) => e.geometry.type === "line" || e.geometry.type === "arc") && E.some((e) => e.geometry.type === "arc")) {
      out.push({ label: tl("Tangentes"), make: () => [{ type: "tangent", a: E[0].id, b: E[1].id }] });
    }
    if (P.length === 1 && L.length === 1 && E.length === 1) out.push({ label: tl("Punto medio"), make: () => [{ type: "midpoint", point: P[0], line: L[0] }] });
    return out;
  };
  /** Borra lo elegido: entidades, puntos sueltos y lo que los nombra */
  const removeSelected = () => {
    const gone = new Set(ui.sel3d());
    void change((s) => {
      s.entities = s.entities.filter((e) => !gone.has(e.id));
      const used = new Set(s.entities.flatMap((e) => (e.geometry.type === "spline" ? e.geometry.points : e.geometry.type === "point" ? [e.geometry.point] : e.geometry.type === "arc" ? [e.geometry.start, e.geometry.mid, e.geometry.end] : [e.geometry.start, e.geometry.end])));
      s.points = s.points.filter((p) => used.has(p.id) || !gone.has(p.id));
      const left = new Set([...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)]);
      s.constraints = s.constraints.filter((c) => ids3d(c).every((x) => left.has(x)));
    });
    ui.setSel3d([]);
  };
  const statusText = () => {
    const r = report();
    if (!r) return "";
    if (r.status === "well_constrained") return tl("Totalmente definido");
    if (r.status === "under_constrained") return tl("{dof} grados de libertad", { dof: r.dof });
    return tl("Hay restricciones en conflicto (en rojo); lo demás se cumple");
  };
  const onePoint = () => (selPoints().length === 1 && selEntities().length === 0 ? sketch().points.find((p) => p.id === selPoints()[0]) : undefined);

  return (
    <div class="space-y-2" data-sketch3d>
      <div class="flex flex-wrap gap-1" role="radiogroup" aria-label={tl("Herramienta del sketch 3D")}>
        <For each={TOOL3D}>
          {(t) => (
            <button
              role="radio"
              aria-checked={tool() === t.id}
              class={clsx("px-2 py-0.5 rounded text-xs border", tool() === t.id ? "bg-accent/20 border-accent text-text" : "border-border text-text-muted hover:text-text")}
              title={t.prompt}
              onClick={() => setTool(t.id)}
            >
              {t.label}
            </button>
          )}
        </For>
      </div>
      <Row label={tl("Plano activo")}>
        <Select
          options={(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] }))}
          value={planeKey()}
          onChange={(v) => setPlaneKey(v as "xy" | "xz" | "yz")}
        />
      </Row>
      <Num label={tl("Desplazado")} suffix="mm" step={1} value={offset()} onCommit={setOffset} />
      <div class="flex flex-wrap gap-1.5">
        <Button size="sm" variant="ghost" disabled={trace().length === 0} onClick={endTrace}>
          {tl("Terminar trazo")}
        </Button>
        <Show when={ui.pick().kind !== "sketch3d"}>
          <Button size="sm" onClick={() => setTool(tool())}>
            {tl("Dibujar en el visor")}
          </Button>
        </Show>
      </div>
      <p class={clsx("text-xs", report()?.status === "well_constrained" ? "text-success" : report()?.status === "over_constrained" ? "text-error" : "text-text-muted")} data-sketch3d-status>
        {sketch().points.length} {tl("puntos ·")} {sketch().entities.length} {tl("curvas")}
        <Show when={statusText()}> · {statusText()}</Show>
      </p>

      <Show when={ui.sel3d().length > 0}>
        <div class="space-y-1.5 rounded border border-border p-1.5">
          <p class="text-[11px] text-text-dim">{tl("Elegido:")} {ui.sel3d().length}</p>
          <Show when={onePoint()}>
            {(p) => (
              <div class="grid grid-cols-3 gap-1">
                <For each={["x", "y", "z"] as const}>
                  {(k, i) => (
                    <Num
                      label={k.toUpperCase()}
                      step={1}
                      value={p()[k]}
                      onCommit={(v) =>
                        void change((s) => {
                          const q = s.points.find((x) => x.id === p().id);
                          if (q) q[(["x", "y", "z"] as const)[i()]] = v;
                        })
                      }
                    />
                  )}
                </For>
              </div>
            )}
          </Show>
          <div class="flex flex-wrap gap-1">
            <For each={options()}>
              {(o) => (
                <Button size="sm" onClick={() => add(o.make())}>
                  {o.label}
                </Button>
              )}
            </For>
            <Show when={selEntities().length > 0}>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const ids = new Set(selEntities().map((e) => e.id));
                  const on = !selEntities().every((e) => e.construction);
                  void change((s) => s.entities.forEach((e) => ids.has(e.id) && (e.construction = on)));
                }}
              >
                {tl("Construcción")}
              </Button>
            </Show>
            <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={removeSelected}>
              {tl("Borrar")}
            </Button>
          </div>
        </div>
      </Show>

      <Show when={sketch().constraints.length > 0}>
        <div class="space-y-0.5 max-h-56 overflow-y-auto" aria-label={tl("Restricciones del sketch 3D")}>
          <For each={sketch().constraints}>
            {(c, i) => {
              const conflict = () => report()?.conflicting.includes(i()) ?? false;
              const value = () => ("value" in c ? c.value : "degrees" in c ? c.degrees : undefined);
              return (
                <div
                  class={clsx("flex items-center gap-2 px-1.5 py-0.5 rounded text-xs hover:bg-surface", conflict() ? "bg-error/15 text-error" : "text-text-muted")}
                  data-constraint3d={c.type}
                >
                  <button class="flex-1 truncate text-left hover:text-text" title={tl("Elegir lo que restringe")} onClick={() => ui.setSel3d(ids3d(c))}>
                    {c.type === "along_axis" ? tl("Paralela a {axis}", { axis: c.axis.toUpperCase() }) : C3D_LABELS[c.type]}
                  </button>
                  <Show when={value() !== undefined}>
                    <div class="w-20">
                      <Num
                        step={1}
                        value={value()!}
                        onCommit={(v) =>
                          void change((s) => {
                            const x = s.constraints[i()];
                            if (x && "value" in x) x.value = v;
                            else if (x && "degrees" in x) x.degrees = v;
                          })
                        }
                      />
                    </div>
                  </Show>
                  <IconButton size="sm" aria-label={tl("Quitar")} onClick={() => void change((s) => s.constraints.splice(i(), 1))}>
                    <Icons.X size={11} />
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <p class="text-[11px] text-text-dim leading-relaxed">
        {tl("Clic en un vértice del sólido: el punto queda atado a él. Las líneas casi paralelas a un eje quedan paralelas. Sirve de camino para un Barrido (elegirlo como curva); un perfil en un plano que la cruce se ubica con «Perforar» en su sketch.")}
      </p>
    </div>
  );
};

/** Señal del panel del sketch que se recuerda entre sesiones (lo último usado en cada herramienta) */
function persisted<T>(key: string, initial: T) {
  const k = `pinocchio.sketchPanel.${key}`;
  let start = initial;
  try {
    const v = localStorage.getItem(k);
    if (v != null) start = JSON.parse(v) as T;
  } catch {
    // Sin almacenamiento: el de siempre
  }
  const [get, set] = createSignal<T>(start);
  const save = (v: T) => {
    set(() => v);
    try {
      localStorage.setItem(k, JSON.stringify(v));
    } catch {
      // Sin almacenamiento: vale para esta sesión
    }
  };
  return [get, save] as const;
}

const SketchPanel: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  /** Copia al sketch el corte del escaneo por su plano (líneas y círculos reconocidos), elegido */
  const useScanOutline = () => {
    const o = ui.scanOutline();
    if (!o) return;
    let made: number[] = [];
    const clip = { points: o.sketch.points.map((p) => ({ id: p.id, x: p.x, y: p.y })), entities: o.sketch.entities, constraints: o.sketch.constraints, texts: [] };
    ui.change((sk) => (made = insertClip(sk, clip, (p) => p)));
    ui.setSelection(made);
  };
  const [cornerRadius, setCornerRadius] = persisted("cornerRadius", 5);
  /** Entidades del mismo tipo y la misma medida (±0,5 %) que `id` */
  const similarTo = (id: number): number[] => {
    const sk = sketch();
    const pt = (pid: number) => sk.points.find((p) => p.id === pid)!;
    const size = (e: SketchEntity): number | undefined => {
      const g = e.geometry;
      if (g.type === "line") return Math.hypot(pt(g.end).x - pt(g.start).x, pt(g.end).y - pt(g.start).y);
      if (g.type === "circle") return g.radius;
      if (g.type === "arc") return Math.hypot(pt(g.start).x - pt(g.center).x, pt(g.start).y - pt(g.center).y);
      return undefined;
    };
    const ref = sk.entities.find((e) => e.id === id)!;
    const r = size(ref);
    return sk.entities
      .filter((e) => e.geometry.type === ref.geometry.type && !!e.construction === !!ref.construction)
      .filter((e) => r === undefined || Math.abs((size(e) ?? 0) - r) <= Math.max(1e-9, r * 0.005))
      .map((e) => e.id);
  };
  /** Botón con su vista previa a la vista */
  let previewing: HTMLButtonElement | undefined;
  const [splitParts, setSplitParts] = persisted("splitParts", 3);
  const [copyTimes, setCopyTimes] = persisted("copyTimes", 1);
  // Equidistante: cuántas copias, el original a construcción y la distancia como cota
  const [offsetCopies, setOffsetCopies] = persisted("offsetCopies", 1);
  const [offsetToConstruction, setOffsetToConstruction] = createSignal(false);
  const [offsetDim, setOffsetDim] = createSignal(true);
  // Redondeo y chaflán: dejar la esquina como punto virtual y recortar las líneas
  const [keepCorner, setKeepCorner] = createSignal(true);
  const [trimCorner, setTrimCorner] = createSignal(true);
  /** Esquinas elegidas: puntos donde se juntan exactamente dos líneas */
  const corners = () => selPoints().filter((p) => linesAt(sketch(), p).length === 2);
  const [offsetDist, setOffsetDist] = persisted("offsetDist", 2);
  const [offsetBoth, setOffsetBoth] = createSignal(false);
  const [offsetCaps, setOffsetCaps] = createSignal<OffsetCaps>("open");
  const [chamferDist, setChamferDist] = persisted("chamferDist", 2);
  const [chamferMode, setChamferMode] = createSignal<"equal" | "distance" | "angle">("equal");
  const [chamferSecond, setChamferSecond] = createSignal(45);
  const [repairTol, setRepairTol] = createSignal(0.05);
  const [moveBy, setMoveBy] = createSignal<P2>([10, 0]);
  const [turnBy, setTurnBy] = createSignal(90);
  const [scaleBy, setScaleBy] = createSignal(2);
  const [patternCount, setPatternCount] = persisted("patternCount", 3);
  const [patternKind, setPatternKind] = createSignal<SketchPatternKind>("linear");
  const [patternStep, setPatternStep] = persisted<P2>("patternStep", [20, 0]);
  // Lineal: el paso es entre copias o la distancia total (de la primera a la última)
  const [patternTotal, setPatternTotal] = persisted("patternTotal", false);
  // Simetría: quitar el original (mover en vez de copiar)
  const [mirrorMove, setMirrorMove] = createSignal(false);
  const [patternRows, setPatternRows] = createSignal(1);
  const [patternStep2, setPatternStep2] = createSignal<P2>([0, 20]);
  const [patternAngle, setPatternAngle] = persisted("patternAngle", 360);
  const [patternRotate, setPatternRotate] = createSignal(true);
  const [curveRotate, setCurveRotate] = createSignal(false);
  const [tableRows, setTableRows] = createSignal<P2[]>([
    [10, 0],
    [20, 5],
  ]);
  const [fillSpacing, setFillSpacing] = createSignal(10);
  const [fillMargin, setFillMargin] = createSignal(1);
  const [fillHex, setFillHex] = createSignal(false);
  // Patrón que se está editando (sus datos se cargaron en los campos de arriba)
  const [editingPattern, setEditingPattern] = createSignal<number>();
  const editing = () => sketch().patterns?.find((p) => p.id === editingPattern());
  const selPattern = () => patternAt(sketch(), selEntities()[0]?.id);
  /** Carga los datos del patrón en los campos para cambiarlos y rehacerlo */
  const loadPattern = (p: SketchPatternRecord) => {
    const sp = p.spec;
    setPatternKind(sp.kind);
    if (sp.kind === "table") setTableRows(sp.offsets.map((o) => [...o] as P2));
    else setPatternCount(sp.count);
    if (sp.kind === "linear") {
      setPatternTotal(false);
      setPatternStep(sp.offset);
      setPatternRows(sp.rows);
      setPatternStep2(sp.offset2);
    } else if (sp.kind === "circular") {
      setPatternAngle(sp.span);
      setPatternRotate(sp.rotate);
    } else if (sp.kind === "curve") setCurveRotate(sp.rotate);
    setEditingPattern(p.id);
    ui.setMessage(tl("Cambiar los datos y «Rehacer el patrón»"));
  };
  // Restricciones que faltan: se piden con el botón; cambiar el sketch deja la lista vieja
  const [missing, setMissing] = createSignal<SketchSuggestion[]>();
  const [defineRelations, setDefineRelations] = createSignal(true);
  const [assistBusy, setAssistBusy] = createSignal(false);
  createEffect(on(() => ui.sketchHistory(), () => setMissing(undefined), { defer: true }));
  const s = () => ui.session()!;
  const sketch = (): Sketch => s().sketch;
  const entity = (id: number) => sketch().entities.find((e) => e.id === id);
  const point = (id: number) => sketch().points.find((p) => p.id === id);
  const sel = () => ui.selection();
  const selEntities = () => sel().map(entity).filter((e) => e !== undefined);
  const selPoints = () => sel().filter((id) => point(id));
  const lines = () => selEntities().filter((e) => e!.geometry.type === "line");



  const add = (c: SketchConstraint | SketchConstraint[]) => ui.change((s) => s.constraints.push(...[c].flat()));
  const askMissing = async () => {
    setAssistBusy(true);
    try {
      setMissing(await props.store.suggestSketch(sketch()));
    } catch (e) {
      ui.setMessage(String(e));
    } finally {
      setAssistBusy(false);
    }
  };
  /** Agrega sugeridas y vuelve a preguntar qué falta (con lo nuevo puesto) */
  const applyMissing = (list: SketchSuggestion[]) => {
    ui.setHoverIds([]);
    add(list.map((m) => m.constraint));
    void askMissing();
  };
  const defineAll = async () => {
    setAssistBusy(true);
    try {
      const more = await props.store.defineSketch(sketch(), defineRelations());
      if (more.length === 0) ui.setMessage(tl("No encontré qué agregar: lo que queda libre no se define con cotas simples"));
      else {
        add(more);
        setMissing(undefined);
      }
    } catch (e) {
      ui.setMessage(String(e));
    } finally {
      setAssistBusy(false);
    }
  };
  /** Curvas de operaciones anteriores al sketch (hélices, sketches 3D o envueltos) que pueden perforarlo */
  const pierceCurves = () => {
    const doc = props.store.doc();
    const at = doc?.features.findIndex((f) => f.id === s().feature) ?? -1;
    const before = new Set((doc?.features ?? []).slice(0, Math.max(0, at)).map((f) => f.id));
    const names = new Map((doc?.features ?? []).map((f) => [f.id, f.name]));
    return (props.store.result()?.references ?? [])
      .filter((r) => (r.kind === "curve" || r.kind === "curves") && before.has(r.id))
      .map((r) => ({ value: String(r.id), label: names.get(r.id) ?? tl("Curva {id}", { id: r.id }) }));
  };
  /** Ata el punto a donde la curva cruza el plano del sketch (el cruce más cercano) */
  const pierce = (pid: number, curve: number): string | undefined => {
    const ref = props.store.result()?.references.find((r) => r.id === curve);
    if (!ref || (ref.kind !== "curve" && ref.kind !== "curves")) return tl("Esa curva no está calculada");
    const plane = s().plane;
    const n = plane.normal;
    const side = (p: P3) => (p[0] - plane.origin[0]) * n[0] + (p[1] - plane.origin[1]) * n[1] + (p[2] - plane.origin[2]) * n[2];
    const hits: P2[] = [];
    for (const line of ref.kind === "curve" ? [ref.points] : ref.lines) {
      for (let i = 1; i < line.length; i++) {
        const [a, b] = [line[i - 1], line[i]];
        const [da, db] = [side(a), side(b)];
        if ((da > 0) === (db > 0) && Math.abs(da) > 1e-9 && Math.abs(db) > 1e-9) continue;
        const t = Math.abs(da - db) < 1e-15 ? 0 : da / (da - db);
        const w: P3 = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
        const d: P3 = [w[0] - plane.origin[0], w[1] - plane.origin[1], w[2] - plane.origin[2]];
        const x = plane.x_dir;
        const y: P3 = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
        hits.push([d[0] * x[0] + d[1] * x[1] + d[2] * x[2], d[0] * y[0] + d[1] * y[1] + d[2] * y[2]]);
      }
    }
    const p = point(pid);
    if (!p || hits.length === 0) return tl("Esa curva no cruza el plano del sketch");
    const at = hits.reduce((best, h) => (Math.hypot(h[0] - p.x, h[1] - p.y) < Math.hypot(best[0] - p.x, best[1] - p.y) ? h : best));
    const rounded: P2 = [Math.round(at[0] * 1e6) / 1e6, Math.round(at[1] * 1e6) / 1e6];
    ui.change((sk) => {
      sk.constraints = sk.constraints.filter((c) => !(c.type === "pierce" && c.point === pid));
      sk.constraints.push({ type: "pierce", point: pid, curve, at: rounded });
    });
    return undefined;
  };
  /** Grados libres de una entidad (undefined: ya definida) */
  const entityDof = (id: number) => s().report?.entity_dof?.find((d) => d[0] === id)?.[1];

  const suggestions = () => constraintSuggestions(sketch(), sel(), s().report?.free_points);

  return (
    <div
      class="space-y-4"
      // Pasar el mouse por un botón que cambia el sketch muestra en el visor lo que va a quedar
      onPointerOver={(e) => {
        const b = (e.target as HTMLElement).closest("button[data-preview]") as HTMLButtonElement | null;
        if (!b || b === previewing || b.disabled) return;
        previewing = b;
        ui.previewOf(() => b.click());
      }}
      onPointerOut={(e) => {
        const b = (e.target as HTMLElement).closest("button[data-preview]");
        if (b && !b.contains(e.relatedTarget as Node)) {
          previewing = undefined;
          ui.clearPreview();
        }
      }}
    >
      <Section title={tl("Sketch")}>
        <p class="text-[11px] text-text-dim leading-relaxed">
          {tl("Dibujar en el visor. Con «Elegir» se marcan puntos y líneas (Mayús suma) para restringirlos; los puntos se arrastran y el sketch se recalcula respetando las medidas.")}
        </p>
        <Show when={s().report}>
          {(r) => (
            <p class={clsx("text-xs", r().status === "well_constrained" ? "text-success" : r().status === "under_constrained" ? "text-text-muted" : "text-error")}>
              {r().status === "well_constrained"
                ? tl("Totalmente definido")
                : r().status === "under_constrained"
                  ? tl("{dof} grados de libertad", { dof: r().dof })
                  : r().status === "over_constrained"
                    ? r().partial
                      ? tl("Hay restricciones en conflicto (marcadas en rojo); lo demás se cumple")
                      : tl("Hay restricciones en conflicto (marcadas en rojo)")
                    : tl("No se pudo resolver")}
              {" · "}
              {s().regions.length} {tl("regiones cerradas")}
            </p>
          )}
        </Show>
        <Show when={ui.scanOutline()?.sketch.entities.length}>
          <div class="flex items-center gap-2 rounded-md border border-border p-2" aria-label={tl("Corte del escaneo")}>
            <span class="flex-1 text-[11px] text-text-dim leading-snug">{tl("El escaneo corta este plano (línea naranja): sirve de guía y de anclaje.")}</span>
            <Button size="sm" onClick={useScanOutline}>
              {tl("Usar el corte")}
            </Button>
          </div>
        </Show>
        <div class="space-y-1.5 border-b border-border pb-2" aria-label={tl("Ayudas al dibujar")}>
          <div class="flex flex-wrap gap-x-3">
            <Checkbox small label={tl("Rejilla")} checked={ui.assist().grid} onChange={(c) => ui.setAssist({ grid: c })} />
            <Checkbox small label={tl("Anclar a la rejilla")} checked={ui.assist().gridSnap} onChange={(c) => ui.setAssist({ gridSnap: c })} />
          </div>
          <div class="flex flex-wrap gap-x-3">
            <Checkbox small label={tl("Ángulos fijos")} checked={ui.assist().angleStep > 0} onChange={(c) => ui.setAssist({ angleStep: c ? 15 : 0 })} />
          </div>
          <Show when={ui.assist().angleStep > 0}>
            <Num label={tl("Paso del ángulo")} suffix="°" step={5} min={1} value={ui.assist().angleStep} onCommit={(v) => v > 0 && v <= 90 && ui.setAssist({ angleStep: v })} />
          </Show>
          <Num label={tl("Paso de la rejilla")} suffix="mm" step={1} min={0} value={ui.assist().spacing} onCommit={(v) => v > 0 && ui.setAssist({ spacing: v })} />
          <div class="flex flex-wrap gap-x-3">
            <Show when={!s().sheet}>
              <Checkbox small label={tl("Corte en el plano")} checked={ui.assist().section} onChange={(c) => ui.setAssist({ section: c })} />
              <Checkbox small label={tl("Ocultar el modelo")} checked={ui.assist().hideModel} onChange={(c) => ui.setAssist({ hideModel: c })} />
            </Show>
          </div>
          <Show
            when={ui.symmetryAxis() !== undefined && entity(ui.symmetryAxis()!)}
            fallback={
              <Show when={lines().length === 1 && selEntities().length === 1}>
                <Button size="sm" title={tl("Lo que se dibuje desde ahora sale reflejado en la línea elegida y atado por simetría")} onClick={() => ui.setSymmetryAxis(lines()[0]!.id)}>
                  {tl("Simetría al dibujar (eje: la línea elegida)")}
                </Button>
              </Show>
            }
          >
            <div class="flex items-center gap-2 text-xs" data-symmetry-axis={ui.symmetryAxis()}>
              <span class="flex-1 text-accent">{tl("Simetría al dibujar activa")}</span>
              <Button size="sm" variant="ghost" onClick={() => ui.setSymmetryAxis(undefined)}>
                {tl("Apagar")}
              </Button>
            </div>
          </Show>
        </div>
        <div class="space-y-1.5" aria-label={tl("Revisar el contorno")}>
          <ProblemList ui={ui} sketch={sketch()} />
          <Num label={tl("Tolerancia")} suffix="mm" step={0.01} min={0} value={repairTol()} onCommit={setRepairTol} />
          <div class="flex flex-wrap gap-1.5">
            <Button
              size="sm"
              title={tl("Junta los extremos a menos de la tolerancia, quita lo más corto que ella, parte las curvas en los cruces y las T y junta los puntos sueltos con el que tengan encima")}
              onClick={() => {
                let r: ReturnType<typeof repairSketch> | undefined;
                ui.change((sk) => (r = repairSketch(sk, repairTol())));
                const n = r!.joined + r!.removed + r!.split + r!.points;
                ui.setMessage(
                  n
                    ? tl("Reparado: {joined} extremos juntados, {removed} entidades cortísimas quitadas, {split} cruces partidos, {points} puntos sueltos juntados", { joined: r!.joined, removed: r!.removed, split: r!.split, points: r!.points })
                    : tl("No había nada que reparar con esa tolerancia"),
                );
              }}
            >
              {tl("Reparar")}
            </Button>
            <Button
              size="sm"
              title={tl("Quita las líneas, círculos y arcos repetidos y junta los que se pisan en parte (lo que las nombraba pasa a la que queda)")}
              onClick={() => {
                let n = 0;
                let m = 0;
                ui.change((sk) => {
                  n = removeDuplicates(sk);
                  m = mergeOverlaps(sk);
                });
                const parts = [n && tn(n, "{n} repetida quitada", "{n} repetidas quitadas"), m && tn(m, "{n} encimada juntada", "{n} encimadas juntadas")].filter(Boolean);
                ui.setMessage(parts.length ? parts.join(", ") : tl("No hay entidades repetidas ni encimadas"));
              }}
            >
              {tl("Eliminar duplicados")}
            </Button>
            <Button
              size="sm"
              title={tl("Une con una línea los extremos sueltos, de a pares, primero los más cercanos")}
              onClick={() => {
                let made: number[] = [];
                ui.change((sk) => (made = closeContour(sk)));
                ui.setMessage(made.length ? (made.length === 1 ? tl("1 línea nueva cierra el contorno") : tl("{n} líneas nuevas cierran el contorno", { n: made.length })) : tl("No hay extremos sueltos para unir"));
              }}
            >
              {tl("Cerrar contorno")}
            </Button>
          </div>
        </div>
      </Section>

      <Section title={tl("Definir")}>
        <Show when={selEntities().length === 1 && entityDof(selEntities()[0]!.id)}>
          {(n) => (
            <p class="text-xs text-text-muted" data-entity-dof={n()}>
              {tl("Lo elegido tiene")} {tn(n(), "{n} grado libre", "{n} grados libres")} {tl("(las flechas amarillas muestran hacia dónde se mueve)")}
            </p>
          )}
        </Show>
        <div class="flex flex-wrap gap-1.5">
          <Button size="sm" disabled={assistBusy()} title={tl("Relaciones casi cumplidas (casi horizontal, casi coincidentes…) y cotas que faltan")} onClick={() => void askMissing()}>
            {tl("Sugerir restricciones")}
          </Button>
          <Button
            size="sm"
            disabled={assistBusy() || s().report?.status === "well_constrained"}
            title={tl("Agrega lo que falta para que quede totalmente definido: cotas con lo que mide ahora (nada se mueve) desde el origen")}
            onClick={() => void defineAll()}
          >
            {tl("Definir todo")}
          </Button>
        </div>
        <Checkbox small label={tl("Con las relaciones casi cumplidas")} checked={defineRelations()} onChange={setDefineRelations} />
        <Show when={missing()}>
          {(list) => (
            <Show when={list().length > 0} fallback={<p class="text-xs text-text-dim">{tl("No falta nada evidente")}</p>}>
              <div class="space-y-0.5 max-h-56 overflow-y-auto" aria-label={tl("Restricciones sugeridas")}>
                <For each={list()}>
                  {(m) => (
                    <div
                      class="flex items-center gap-2 px-1.5 py-0.5 rounded text-xs text-text-muted hover:bg-surface"
                      data-suggestion={m.constraint.type}
                      onMouseEnter={() => ui.setHoverIds(constraintIds(m.constraint))}
                      onMouseLeave={() => ui.setHoverIds([])}
                    >
                      <span class="w-4 shrink-0 text-center text-[10px]">{GLYPHS[m.constraint.type] ?? (isDimension(m.constraint) ? "↔" : "·")}</span>
                      <span class="flex-1 truncate" title={m.why}>
                        {CONSTRAINT_LABELS[m.constraint.type]}
                        <span class="text-text-dim"> · {m.why}</span>
                      </span>
                      <Button size="sm" variant="ghost" onClick={() => applyMissing([m])}>
                        {tl("Agregar")}
                      </Button>
                    </div>
                  )}
                </For>
              </div>
              <Button size="sm" onClick={() => applyMissing(list())}>
                {tl("Agregar todas (")}{list().length})
              </Button>
            </Show>
          )}
        </Show>
      </Section>

      <Section title={`Elegido (${sel().length})`}>
        <Show when={sel().length > 0} fallback={<p class="text-xs text-text-dim">{tl("Nada elegido")}</p>}>
          <Show when={selEntities().length >= 1}>
            <Button
              size="sm"
              variant="ghost"
              title={tl("Todas las del mismo tipo y la misma medida (largo de las líneas, radio de círculos y arcos) que la primera elegida")}
              onClick={() => ui.setSelection(similarTo(selEntities()[0]!.id))}
            >
              {tl("Elegir similares")}
            </Button>
          </Show>
          <div class="flex flex-wrap gap-1.5">
            {/* Index: si el sketch cambia entre apretar y soltar (se confirma una cota abierta) el botón sigue siendo el mismo y el clic no se pierde */}
            <Index each={suggestions()}>
              {(sug) => (
                <Button size="sm" onClick={() => add(sug().make())}>
                  {sug().label}
                </Button>
              )}
            </Index>
          </div>
          <Show when={selPoints().length === 1 && selEntities().length === 0 && pierceCurves().length > 0}>
            <div class="flex flex-wrap items-center gap-1.5" aria-label={tl("Perforar")}>
              <span class="text-xs text-text-muted">{tl("Perforar:")}</span>
              <For each={pierceCurves()}>
                {(c) => (
                  <Button
                    size="sm"
                    title={tl("El punto va donde la curva cruza el plano del sketch (y la sigue si cambia)")}
                    onClick={() => {
                      const msg = pierce(selPoints()[0], +c.value);
                      if (msg) ui.setMessage(msg);
                    }}
                  >
                    {c.label}
                  </Button>
                )}
              </For>
            </div>
          </Show>
          <Show when={corners().length > 0}>
            <div class="flex flex-wrap gap-x-3">
              <Checkbox small label={tl("Esquina virtual")} checked={keepCorner()} onChange={setKeepCorner} />
              <Checkbox small label={tl("Recortar")} checked={trimCorner()} onChange={setTrimCorner} />
            </div>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label={tl("Radio")} suffix="mm" step={0.5} value={cornerRadius()} onCommit={setCornerRadius} />
              </div>
              <Button data-preview=""
                size="sm"
                title={tl("Con varias esquinas elegidas, todas con el mismo radio (una cota y las demás iguales)")}
                onClick={() => {
                  let msg: string | undefined;
                  const points = corners();
                  ui.change((s) => {
                    const arcs: number[] = [];
                    for (const p of points) {
                      msg = filletCorner(s, p, cornerRadius(), { keepCorner: keepCorner(), trim: trimCorner() });
                      if (msg) return;
                      arcs.push(s.entities[s.entities.length - 1].id);
                    }
                    // Varias: una sola cota de radio y las demás iguales (cambian juntas)
                    const first = arcs[0];
                    s.constraints = s.constraints.filter((k) => !(k.type === "radius" && arcs.slice(1).includes(k.entity)));
                    for (const a of arcs.slice(1)) s.constraints.push({ type: "equal", a: first, b: a });
                  });
                  if (msg) ui.setMessage(msg);
                  else ui.setSelection([]);
                }}
              >
                {corners().length > 1 ? tl("Redondear {length} esquinas", { length: corners().length }) : tl("Redondear esquina")}
              </Button>
            </div>
            <div class="space-y-1" aria-label={tl("Chaflán de la esquina")}>
              <Num label={tl("Chaflán")} suffix="mm" step={0.5} value={chamferDist()} onCommit={setChamferDist} />
              <div class="flex items-center gap-1.5">
                <div class="w-24 shrink-0">
                  <Select
                    options={[
                      { value: "equal", label: tl("Igual") },
                      { value: "distance", label: tl("Dos dist.") },
                      { value: "angle", label: tl("Ángulo") },
                    ]}
                    value={chamferMode()}
                    onChange={(v) => {
                      setChamferMode(v as "equal" | "distance" | "angle");
                      setChamferSecond(v === "angle" ? 45 : chamferDist());
                    }}
                  />
                </div>
                <Show when={chamferMode() !== "equal"}>
                  <div class="flex-1 min-w-0">
                    <input
                      type="number"
                      aria-label={chamferMode() === "angle" ? tl("Ángulo del chaflán") : tl("Segunda distancia del chaflán")}
                      title={chamferMode() === "angle" ? tl("Ángulo (°) entre el chaflán y la primera línea") : tl("Distancia (mm) sobre la segunda línea")}
                      value={chamferSecond()}
                      step={chamferMode() === "angle" ? 5 : 0.5}
                      class="w-full min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent [appearance:textfield]"
                      onChange={(e) => {
                        const v = parseFloat(e.currentTarget.value);
                        if (!Number.isNaN(v)) setChamferSecond(v);
                      }}
                    />
                  </div>
                </Show>
                <Button data-preview=""
                  size="sm"
                  title={tl("La primera distancia va sobre la primera línea de la esquina; el ángulo, entre el chaflán y esa línea")}
                  onClick={() => {
                    let msg: string | undefined;
                    const second = chamferMode() === "angle" ? { angle: chamferSecond() } : { distance: chamferMode() === "equal" ? chamferDist() : chamferSecond() };
                    const points = corners();
                    ui.change((sk) => {
                      for (const p of points) {
                        msg = chamferCorner(sk, p, chamferDist(), second, keepCorner());
                        if (msg) return;
                      }
                    });
                    if (msg) ui.setMessage(msg);
                    else ui.setSelection([]);
                  }}
                >
                  {tl("Achaflanar")}
                </Button>
              </div>
            </div>
          </Show>
          <Show
            when={
              selEntities().length === 2 &&
              selPoints().length === 0 &&
              selEntities()[0]!.geometry.type === selEntities()[1]!.geometry.type &&
              ["line", "arc"].includes(selEntities()[0]!.geometry.type)
            }
          >
            <Button data-preview=""
              size="sm"
              title={tl("Dos líneas alineadas o dos arcos del mismo círculo con un extremo común pasan a ser una sola entidad")}
              onClick={() => {
                const [a, b] = selEntities().map((e) => e!.id);
                let msg: string | undefined;
                ui.change((sk) => (msg = joinEntities(sk, a, b)));
                ui.setMessage(msg);
                if (!msg) ui.setSelection([a]);
              }}
            >
              {tl("Unir")}
            </Button>
          </Show>
          <Show when={selEntities().length === 1 && selPoints().length === 0 && ["line", "arc"].includes(selEntities()[0]!.geometry.type)}>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label={tl("Partes")} step={1} min={2} value={splitParts()} onCommit={(v) => setSplitParts(Math.max(2, Math.round(v)))} />
              </div>
              <Button data-preview=""
                size="sm"
                title={tl("Parte la línea o el arco en partes iguales (las líneas quedan con el mismo largo atado)")}
                onClick={() => {
                  const id = selEntities()[0]!.id;
                  let msg: string | undefined;
                  ui.change((sk) => (msg = splitEqual(sk, id, splitParts())));
                  ui.setMessage(msg);
                  if (!msg) ui.setSelection([]);
                }}
              >
                {tl("Partir en")} {splitParts()}
              </Button>
            </div>
          </Show>
          <Show when={selEntities().length > 0}>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label={tl("Distancia")} suffix="mm" step={0.5} value={offsetDist()} onCommit={setOffsetDist} />
              </div>
              <Button data-preview=""
                size="sm"
                title={tl("En un lazo cerrado, positivo es hacia afuera; en una cadena abierta, a la izquierda del recorrido")}
                onClick={() => {
                  let msg: string | undefined;
                  const regions = s().regions;
                  // Una sola entidad elegida: toda su cadena conectada (un clic basta para el contorno entero)
                  const picked = selEntities().map((e) => e!.id);
                  const ids = picked.length === 1 ? connectedChain(sketch(), picked[0]) : picked;
                  const chain = orderChain(sketch(), ids);
                  // Lazos de líneas y círculos: como siempre (positivo hacia afuera); cadenas abiertas, con arcos o a los dos lados: por la cadena
                  const byChain = typeof chain !== "string" && (offsetBoth() || !chain.closed || selEntities().some((e) => e!.geometry.type === "arc"));
                  ui.change((sk) => {
                    // Una copia por vez, cada una más lejos (d, 2d, 3d…)
                    for (let k = 1; k <= offsetCopies(); k++) {
                      const d = offsetDist() * k;
                      const before = new Set(sk.entities.map((e) => e.id));
                      if (!byChain) msg = offsetEntities(sk, regions, ids, d);
                      else {
                        const r = offsetChain(sk, ids, d, { both: offsetBoth(), caps: offsetCaps() });
                        if (typeof r === "string") msg = r;
                      }
                      if (msg) return;
                      // La distancia como cota editable: de una línea original a su paralela nueva
                      if (offsetDim()) {
                        const pt = (id: number) => sk.points.find((q) => q.id === id)!;
                        const dir = (e: SketchEntity) => {
                          const g = e.geometry as { start: number; end: number };
                          const [a, b] = [pt(g.start), pt(g.end)];
                          const l = Math.hypot(b.x - a.x, b.y - a.y) || 1;
                          return [(b.x - a.x) / l, (b.y - a.y) / l];
                        };
                        const orig = sk.entities.find((e) => ids.includes(e.id) && e.geometry.type === "line");
                        const twin = orig && sk.entities.find((e) => !before.has(e.id) && e.geometry.type === "line" && Math.abs(dir(e)[0] * dir(orig)[1] - dir(e)[1] * dir(orig)[0]) < 1e-6);
                        if (orig && twin) sk.constraints.push({ type: "point_line_distance", point: (twin.geometry as { start: number }).start, line: orig.id, value: +Math.abs(d).toFixed(6) });
                      }
                    }
                    // El original pasa a construcción
                    if (offsetToConstruction()) for (const e of sk.entities) if (ids.includes(e.id)) e.construction = true;
                  });
                  ui.setMessage(msg);
                }}
              >
                {tl("Equidistante")}
              </Button>
            </div>
            <div class="flex flex-wrap gap-x-3" aria-label={tl("Más opciones del equidistante")}>
              <Checkbox small label={tl("Original a construcción")} checked={offsetToConstruction()} onChange={setOffsetToConstruction} />
              <Checkbox small label={tl("Distancia como cota")} checked={offsetDim()} onChange={setOffsetDim} />
            </div>
            <div class="flex items-center gap-2" aria-label={tl("Opciones del equidistante")}>
              <Checkbox small label={tl("A los dos lados")} checked={offsetBoth()} onChange={setOffsetBoth} />
              <div class="w-20">
                <Num label={tl("Copias")} step={1} min={1} value={offsetCopies()} onCommit={(v) => setOffsetCopies(Math.max(1, Math.round(v)))} />
              </div>
              <div class="flex-1 min-w-0">
                <Select
                  options={[
                    { value: "open", label: tl("Extremos abiertos") },
                    { value: "round", label: tl("Extremos redondos") },
                    { value: "line", label: tl("Extremos rectos") },
                  ]}
                  value={offsetCaps()}
                  onChange={(v) => setOffsetCaps(v as OffsetCaps)}
                />
              </div>
            </div>
          </Show>
          <Show when={selEntities().length === 1 && selEntities()[0]!.geometry.type === "spline"}>
            <Button
              size="sm"
              title={tl("Une el último punto con el primero (o la abre entre ellos)")}
              onClick={() => {
                const id = selEntities()[0]!.id;
                ui.change((sk) => {
                  const g = sk.entities.find((e) => e.id === id)?.geometry;
                  if (g?.type === "spline") g.closed = !g.closed;
                });
              }}
            >
              {(selEntities()[0]!.geometry as { closed?: boolean }).closed ? tl("Abrir la spline") : tl("Cerrar la spline")}
            </Button>
            <Button
              size="sm"
              title={tl("Puntos que dan la dirección de salida y de llegada; se arrastran")}
              onClick={() => {
                const id = selEntities()[0]!.id;
                let msg: string | undefined;
                ui.change((sk) => (msg = toggleSplineHandles(sk, id)));
                ui.setMessage(msg);
              }}
            >
              {tl("Manijas en los extremos sí/no")}
            </Button>
          </Show>
          <SplineTools ui={ui} />
          <Show when={selEntities().length > 0}>
            <div class="space-y-1.5 border-t border-border pt-1.5">
              <Show when={lines().length > 0 && selEntities().length > 1}>
                <Button data-preview=""
                  size="sm"
                  title={tl("El eje es la primera línea elegida; lo demás se copia reflejado")}
                  onClick={() => {
                    const ids = selEntities().map((e) => e!.id);
                    const axis = lines()[0]!.id;
                    let msg: string | undefined;
                    ui.change((sk) => {
                      msg = mirrorEntities(sk, ids, axis);
                      // Mover: el original se va (queda solo el reflejo)
                      if (!msg && mirrorMove()) for (const id of ids) if (id !== axis) removeEntity(sk, id);
                    });
                    ui.setMessage(msg);
                    if (!msg) ui.setSelection([]);
                  }}
                >
                  {tl("Simetría (eje: primera línea elegida)")}
                </Button>
              </Show>
              {/* Sin una línea elegida: en los ejes del sketch (una línea de construcción por el origen) */}
              <Show when={lines().length === 0}>
                <div class="flex flex-wrap items-center gap-1.5" aria-label={tl("Simetría en los ejes")}>
                  <For each={[["x", tl("Simetría en el eje X")], ["y", tl("Simetría en el eje Y")]] as const}>
                    {([axis, label]) => (
                      <Button data-preview=""
                        size="sm"
                        title={tl("El reflejo atado por simetría a una línea de construcción por el origen")}
                        onClick={() => {
                          const ids = selEntities().map((e) => e!.id);
                          let msg: string | undefined;
                          ui.change((sk) => {
                            const o = ensureOrigin(sk);
                            const span = Math.max(10, ...sk.points.map((q) => Math.max(Math.abs(q.x), Math.abs(q.y)))) * 1.2;
                            const a = addPoint(sk, axis === "x" ? [-span, 0] : [0, -span]);
                            const b = addPoint(sk, axis === "x" ? [span, 0] : [0, span]);
                            const line = addEntity(sk, { type: "line", start: a, end: b });
                            const e = sk.entities.find((x) => x.id === line)!;
                            e.construction = true;
                            sk.constraints.push({ type: axis === "x" ? "horizontal" : "vertical", line }, { type: "point_on_line", point: o, line });
                            msg = mirrorEntities(sk, ids, line);
                            if (!msg && mirrorMove()) for (const id of ids) removeEntity(sk, id);
                          });
                          ui.setMessage(msg);
                          if (!msg) ui.setSelection([]);
                        }}
                      >
                        {label}
                      </Button>
                    )}
                  </For>
                </div>
              </Show>
              <Checkbox small label={tl("Quitar el original (mover)")} checked={mirrorMove()} onChange={setMirrorMove} />
              <Show when={selPattern()}>
                {(sp) => (
                  <div class="space-y-1 rounded border border-border p-1.5" data-pattern-box>
                    <div class="text-xs text-text-muted">
                      {patternLabel(sp().pattern)}
                      {sp().copy >= 0 ? tl(" · copia {n}", { n: sp().copy + 2 }) : tl(" · el original")}
                    </div>
                    <div class="flex flex-wrap gap-1.5">
                      <Button size="sm" title={tl("Carga sus datos en los campos de abajo para cambiarlos")} onClick={() => loadPattern(sp().pattern)}>
                        {tl("Editar el patrón")}
                      </Button>
                      <Show when={sp().copy >= 0}>
                        <Button
                          data-preview=""
                          size="sm"
                          title={tl("La copia queda de construcción (no forma regiones) y sigue así al rehacer el patrón")}
                          onClick={() => {
                            const { pattern, copy } = sp();
                            ui.change((sk) => void toggleSkip(sk, pattern.id, copy));
                          }}
                        >
                          {sp().pattern.skip.includes(sp().copy) ? tl("Volver a poner la copia") : tl("Omitir esta copia")}
                        </Button>
                      </Show>
                      <Button
                        size="sm"
                        title={tl("Rehace el patrón con el original como está ahora (si se le agregó algo, las copias también lo tienen)")}
                        onClick={() => {
                          const id = sp().pattern.id;
                          let msg: string | undefined;
                          ui.change((sk) => (msg = editPattern(sk, id)));
                          ui.setMessage(msg);
                        }}
                      >
                        {tl("Rehacer con el original")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        title={tl("Las copias quedan sueltas, con sus restricciones; el patrón ya no se puede editar")}
                        onClick={() => {
                          const id = sp().pattern.id;
                          ui.change((sk) => dropPattern(sk, id));
                          if (editingPattern() === id) setEditingPattern(undefined);
                        }}
                      >
                        {tl("Soltar el patrón")}
                      </Button>
                    </div>
                  </div>
                )}
              </Show>
              <Show when={editing()}>
                <div class="flex items-center justify-between text-[11px] text-accent">
                  {tl("Editando:")} {patternLabel(editing()!)}
                  <button class="underline text-text-muted" onClick={() => setEditingPattern(undefined)}>
                    {tl("Cancelar")}
                  </button>
                </div>
              </Show>
              <div class="flex items-end gap-1.5">
                <Show when={patternKind() !== "table" && patternKind() !== "fill"}>
                  <div class="flex-1">
                    <Num label={tl("Cantidad")} min={2} value={patternCount()} onCommit={(v) => setPatternCount(Math.max(2, Math.round(v)))} />
                  </div>
                </Show>
                <div class={patternKind() === "table" || patternKind() === "fill" ? "flex-1" : undefined}>
                  <Select options={SKETCH_PATTERNS} value={patternKind()} onChange={(v) => setPatternKind(v as SketchPatternKind)} />
                </div>
              </div>
              <Switch>
                <Match when={patternKind() === "linear"}>
                  <div class="flex gap-1.5">
                    <Num label="X" suffix="mm" step={1} value={patternStep()[0]} onCommit={(v) => setPatternStep([v, patternStep()[1]])} />
                    <Num label="Y" suffix="mm" step={1} value={patternStep()[1]} onCommit={(v) => setPatternStep([patternStep()[0], v])} />
                  </div>
                  <Checkbox small label={tl("Distancia total (de la primera a la última)")} checked={patternTotal()} onChange={setPatternTotal} />
                  <Num label={tl("Filas")} min={1} value={patternRows()} onCommit={(v) => setPatternRows(Math.max(1, Math.round(v)))} />
                  <Show when={patternRows() > 1}>
                    <div class="flex gap-1.5" aria-label={tl("Paso entre filas")}>
                      <Num label="X" suffix="mm" step={1} value={patternStep2()[0]} onCommit={(v) => setPatternStep2([v, patternStep2()[1]])} />
                      <Num label="Y" suffix="mm" step={1} value={patternStep2()[1]} onCommit={(v) => setPatternStep2([patternStep2()[0], v])} />
                    </div>
                  </Show>
                </Match>
                <Match when={patternKind() === "circular"}>
                  <Num label={tl("Ángulo")} suffix="°" step={15} value={patternAngle()} onCommit={setPatternAngle} />
                  <Checkbox small label={tl("Girar las copias")} checked={patternRotate()} onChange={setPatternRotate} />
                  <p class="text-[11px] text-text-dim">
                    {selPoints().length === 1 ? tl("Alrededor del punto elegido") : tl("Alrededor del origen (o elegir también un punto)")};{" "}
                    {Math.abs(patternAngle()) >= 360 ? tl("repartidos en la vuelta") : tl("de punta a punta del ángulo (negativo: horario)")}
                  </p>
                </Match>
                <Match when={patternKind() === "curve"}>
                  <Checkbox small label={tl("Girar con la curva")} checked={curveRotate()} onChange={setCurveRotate} />
                  <p class="text-[11px] text-text-dim">
                    {tl("El camino es la primera entidad elegida (línea, arco, círculo, spline o elipse); las copias se reparten de punta a punta. En líneas, arcos y círculos siguen al camino si cambia.")}
                  </p>
                </Match>
                <Match when={patternKind() === "table"}>
                  <div class="space-y-1" aria-label={tl("Tabla de copias")}>
                    <span class="text-xs text-text-muted">{tl("Desplazamiento de cada copia (mm)")}</span>
                    <For each={tableRows()}>
                      {(row, i) => (
                        <div class="flex items-center gap-1" data-table-row={i()}>
                          <Num label="X" step={1} value={row[0]} onCommit={(v) => setTableRows(tableRows().map((r, j) => (j === i() ? [v, r[1]] : r)))} />
                          <Num label="Y" step={1} value={row[1]} onCommit={(v) => setTableRows(tableRows().map((r, j) => (j === i() ? [r[0], v] : r)))} />
                          <IconButton aria-label={tl("Quitar la fila {v1}", { v1: i() + 1 })} size="sm" variant="ghost" onClick={() => setTableRows(tableRows().filter((_, j) => j !== i()))}>
                            <Icons.Trash size={11} />
                          </IconButton>
                        </div>
                      )}
                    </For>
                    <Button
                      size="sm"
                      fullWidth
                      onClick={() => {
                        const last = tableRows()[tableRows().length - 1] ?? [0, 0];
                        setTableRows([...tableRows(), [last[0] + 10, last[1]]]);
                      }}
                    >
                      {tl("Agregar fila")}
                    </Button>
                  </div>
                </Match>
                <Match when={patternKind() === "fill"}>
                  <Num label={tl("Separación")} suffix="mm" step={1} min={0} value={fillSpacing()} onCommit={(v) => setFillSpacing(Math.max(0.01, v))} />
                  <Num label={tl("Margen al borde")} suffix="mm" step={0.5} min={0} value={fillMargin()} onCommit={(v) => setFillMargin(Math.max(0, v))} />
                  <Checkbox small label={tl("Grilla hexagonal")} checked={fillHex()} onChange={setFillHex} />
                  <p class="text-[11px] text-text-dim">
                    {tl("La región es el lazo cerrado de la primera entidad elegida; entran las copias enteras a no menos del margen del borde. La grilla pasa por el original.")}
                  </p>
                </Match>
              </Switch>
              <Button data-preview=""
                size="sm"
                onClick={() => {
                  const ids = selEntities().map((e) => e!.id);
                  const n = patternCount();
                  let msg: string | undefined;
                  const kind = patternKind();
                  const edit = editing();
                  // Con distancia total: el paso es esa distancia repartida entre las copias
                  const step: P2 = patternTotal() ? [patternStep()[0] / (n - 1), patternStep()[1] / (n - 1)] : patternStep();
                  // Lo que se arma (el relleno no queda anotado: depende de la región)
                  let spec: PatternSpec | undefined;
                  if (kind === "linear") spec = { kind, count: n, offset: step, rows: patternRows(), offset2: patternStep2() };
                  else if (kind === "circular") {
                    const center = edit?.spec.kind === "circular" ? edit.spec.center : selPoints().length === 1 ? selPoints()[0] : sketch().origin;
                    if (center === undefined) msg = tl("Elegir el punto alrededor del cual repetir");
                    else spec = { kind, count: n, center, span: patternAngle(), rotate: patternRotate() };
                  } else if (kind === "curve") spec = { kind, count: n, path: edit?.spec.kind === "curve" ? edit.spec.path : ids[0], rotate: curveRotate() };
                  else if (kind === "table") spec = { kind, offsets: tableRows() };
                  if (edit && spec) {
                    ui.change((sk) => (msg = editPattern(sk, edit.id, spec)));
                    if (!msg && !ui.previewing()) setEditingPattern(undefined);
                  } else if (spec) ui.change((sk) => (msg = makePattern(sk, ids, spec!)));
                  else if (kind === "fill") {
                    const first = ids[0];
                    const region = s().regions.find((r) => r.outer.pieces.some((p) => p.entity === first));
                    const border = new Set(region?.outer.pieces.map((p) => p.entity));
                    if (!region) msg = tl("La primera entidad elegida tiene que ser el borde de una región cerrada");
                    else
                      ui.change((sk) => (msg = fillPattern(sk, ids.filter((id) => !border.has(id)), region, fillSpacing(), fillHex(), fillMargin())));
                  }
                  ui.setMessage(msg);
                  if (!msg) ui.setSelection([]);
                }}
              >
                {editing() ? tl("Rehacer el patrón") : tl("Repetir en patrón")}
              </Button>
            </div>
          </Show>
          {/* Con números (con el mouse: herramientas Mover, Copiar, Girar y Escalar de la barra) */}
          <div class="space-y-1.5 border-t border-border pt-1.5" aria-label={tl("Transformar")}>
            <span class="text-xs text-text-muted">{tl("Transformar lo elegido (girar y escalar desde su centro)")}</span>
            <Checkbox
              small
              label={tl("Soltar de lo que no se mueve")}
              checked={ui.detachOnMove()}
              onChange={ui.setDetachOnMove}
            />
            <div class="flex items-end gap-1.5">
              <Num label="X" suffix="mm" step={1} value={moveBy()[0]} onCommit={(v) => setMoveBy([v, moveBy()[1]])} />
              <Num label="Y" suffix="mm" step={1} value={moveBy()[1]} onCommit={(v) => setMoveBy([moveBy()[0], v])} />
            </div>
            <div class="flex gap-1.5">
              <Button size="sm" onClick={() => ui.setMessage(ui.transformSelected(translation(moveBy())))}>
                {tl("Mover")}
              </Button>
              <Button data-preview="" size="sm" onClick={() => ui.setMessage(ui.transformSelected(translation(moveBy()), { copy: true, times: copyTimes() }))}>
                {copyTimes() > 1 ? tl("Copiar {v1} veces", { v1: copyTimes() }) : tl("Copiar corrido")}
              </Button>
              <div class="w-20">
                <Num label={tl("Veces")} step={1} min={1} value={copyTimes()} onCommit={(v) => setCopyTimes(Math.max(1, Math.round(v)))} />
              </div>
            </div>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label={tl("Ángulo")} suffix="°" step={15} value={turnBy()} onCommit={setTurnBy} />
              </div>
              <Button
                size="sm"
                title={tl("Alrededor del centro de lo elegido; positivo: antihorario")}
                onClick={() => {
                  const c = selectionCenter(sketch(), sel());
                  ui.setMessage(c ? ui.transformSelected(rotation(c, turnBy())) : tl("Elegir primero lo que se gira"));
                }}
              >
                {tl("Girar")}
              </Button>
            </div>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label={tl("Factor")} step={0.1} min={0.001} value={scaleBy()} onCommit={(v) => v > 0 && setScaleBy(v)} />
              </div>
              <Button
                size="sm"
                title={tl("Desde el centro de lo elegido; las cotas toman la medida nueva")}
                onClick={() => {
                  const c = selectionCenter(sketch(), sel());
                  ui.setMessage(c ? ui.transformSelected(scaling(c, scaleBy()), { scale: scaleBy() }) : tl("Elegir primero lo que se escala"));
                }}
              >
                {tl("Escalar")}
              </Button>
            </div>
          </div>
          <div class="flex gap-1.5">
            <Button size="sm" variant="ghost" onClick={() => ui.toggleConstruction()} disabled={selEntities().length === 0}>
              {tl("Construcción sí/no (Q)")}
            </Button>
            <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={() => ui.deleteSelection()} disabled={selEntities().length === 0}>
              {tl("Borrar")}
            </Button>
          </div>
        </Show>
      </Section>

      <Show when={!s().sheet}>
        <SketchModelRefs ui={ui} store={props.store} />
      </Show>

      <SketchEntities ui={ui} />

      <SketchFiles ui={ui} store={props.store} />
      <SketchOrganize ui={ui} store={props.store} />

      <Section title={tl("Restricciones")}>
        <Show when={sketch().constraints.length > 0} fallback={<p class="text-xs text-text-dim">{tl("Sin restricciones")}</p>}>
          <div class="flex flex-wrap gap-1" aria-label={tl("Elegir restricciones")}>
            <Button size="sm" variant="ghost" onClick={() => ui.setSelectedConstraints(sketch().constraints.flatMap((c, i) => (isDimension(c) ? [] : [i])))}>
              {tl("Todas las restricciones")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => ui.setSelectedConstraints(sketch().constraints.flatMap((c, i) => (isDimension(c) ? [i] : [])))}>
              {tl("Todas las cotas")}
            </Button>
            <Show when={ui.selectedConstraints().length > 0}>
              <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={() => ui.removeConstraints(ui.selectedConstraints())}>
                {tl("Quitar elegidas (")}{ui.selectedConstraints().length})
              </Button>
            </Show>
          </div>
          <div class="space-y-0.5 max-h-72 overflow-y-auto">
            <For each={sketch().constraints}>
              {(c, i) => {
                const conflict = () => s().report?.conflicting.includes(i()) ?? false;
                const value = () => constraintValue(c);
                return (
                  <div
                    class={clsx(
                      "flex items-center gap-2 px-1.5 py-0.5 rounded text-xs hover:bg-surface",
                      conflict() ? "bg-error/15 text-error" : ui.selectedConstraints().includes(i()) ? "bg-cyan/15 text-text" : "text-text-muted",
                    )}
                    data-constraint-row={i()}
                    onMouseEnter={() => ui.setHoverIds(constraintIds(c))}
                    onMouseLeave={() => ui.setHoverIds([])}
                  >
                    <button
                      class={clsx(
                        "w-4 h-4 shrink-0 rounded border text-[10px] leading-none",
                        ui.selectedConstraints().includes(i()) ? "bg-cyan text-bg border-cyan" : "border-border text-text-dim hover:text-text",
                      )}
                      title={tl("Elegir la restricción (Mayús suma; Supr la quita)")}
                      onClick={(e) =>
                        ui.setSelectedConstraints((cur) =>
                          e.shiftKey ? (cur.includes(i()) ? cur.filter((x) => x !== i()) : [...cur, i()]) : cur.length === 1 && cur[0] === i() ? [] : [i()],
                        )
                      }
                    >
                      {GLYPHS[c.type] ?? (isDimension(c) ? "↔" : "·")}
                    </button>
                    <button
                      class="flex-1 truncate text-left hover:text-text"
                      title={tl("Elegir lo que restringe")}
                      onClick={() => ui.setSelection(constraintIds(c))}
                    >
                      {CONSTRAINT_LABELS[c.type]}
                    </button>
                    <Show when={value() !== undefined}>
                      <Show
                        when={!isReference(c)}
                        fallback={<span class="w-28 px-1.5 font-mono text-text-dim" title={tl("Cota de referencia: mide, no restringe")}>({+value()!.toFixed(3)})</span>}
                      >
                        <div class="w-28">
                          <Formula
                            value={value()!}
                            expr={(c as { expr?: string }).expr}
                            onCommit={(text) => ui.setConstraintText(i(), text)}
                          />
                        </div>
                      </Show>
                      <Tooltip content={isReference(c) ? tl("Pasar a cota que restringe") : tl("Dejar como cota de referencia (solo mide)")}>
                        <button
                          class={clsx("px-1 rounded text-[10px] border", isReference(c) ? "border-accent text-accent" : "border-border text-text-dim hover:text-text")}
                          onClick={() => ui.toggleReference(i())}
                        >
                          {tl("Ref")}
                        </button>
                      </Tooltip>
                    </Show>
                    <IconButton aria-label={tl("Quitar restricción")} size="sm" variant="ghost" onClick={() => ui.removeConstraint(i())}>
                      <Icons.X size={10} />
                    </IconButton>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Section>

      <div class="flex gap-2">
        <Button size="sm" variant="primary" onClick={() => void ui.finishSketch()}>
          {tl("Terminar sketch")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => ui.cancelSketch()}>
          {tl("Descartar cambios")}
        </Button>
      </div>
    </div>
  );
};

const SELECT_KINDS: { kind: SelectKind; label: string }[] = [
  { kind: "lines", get label() { return tl("Líneas"); } },
  { kind: "round", get label() { return tl("Círculos y arcos"); } },
  { kind: "construction", get label() { return tl("Construcción"); } },
  { kind: "points", get label() { return tl("Puntos"); } },
  { kind: "free", get label() { return tl("Sin definir"); } },
  { kind: "all", get label() { return tl("Todo"); } },
];

/** Usar del modelo: silueta, intersección, otro sketch y romper el vínculo */
const SketchModelRefs: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  const [busy, setBusy] = createSignal(false);
  const s = () => ui.session()!;
  const linked = () => s().sketch.uses ?? [];
  const selectedLinked = () => ui.selection().filter((id) => linked().some((u) => u.entity === id)).length;
  // Sketches de antes en el historial (los de después no están calculados todavía)
  const earlier = () => {
    const fs = props.store.doc()?.features ?? [];
    const at = fs.findIndex((f) => f.id === s().feature);
    return fs.slice(0, Math.max(0, at)).filter((f) => f.kind.type === "sketch" && !f.suppressed);
  };
  const run = async (job: () => Promise<string>) => {
    setBusy(true);
    try {
      ui.setMessage(await job());
    } finally {
      setBusy(false);
    }
  };
  return (
    <Section title={tl("Usar del modelo")}>
      <p class="text-[11px] text-text-dim leading-relaxed">
        {tl("Con Usar (J), clic en una arista o en una cara del sólido (la cara trae todo su contorno). Lo usado queda ligado: si el modelo cambia, se mueve con él.")}
      </p>
      <div class="flex flex-wrap gap-1.5">
        <Button size="sm" disabled={busy()} title={tl("El contorno del sólido visto desde la normal del plano, con los agujeros pasantes")} onClick={() => void run(() => ui.useModel("silhouette"))}>
          {tl("Silueta")}
        </Button>
        <Button size="sm" disabled={busy()} title={tl("Las curvas donde el plano del sketch corta el sólido")} onClick={() => void run(() => ui.useModel("section"))}>
          {tl("Intersección")}
        </Button>
      </div>
      <Show when={earlier().length > 0}>
        <span class="text-xs text-text-muted">{tl("De otro sketch")}</span>
        <div class="flex flex-wrap gap-1.5">
          <For each={earlier()}>
            {(f) => (
              <Button size="sm" variant="ghost" disabled={busy()} title={tl("Todas sus curvas y puntos, proyectados a este plano")} onClick={() => void run(() => ui.useSketch(f.id))}>
                {f.name}
              </Button>
            )}
          </For>
        </div>
      </Show>
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs text-text-muted">{linked().length === 1 ? "1 curva ligada" : `${linked().length} curvas ligadas`}</span>
        <Button size="sm" variant="ghost" disabled={selectedLinked() === 0} title={tl("Lo elegido deja de seguir al modelo y queda editable")} onClick={() => ui.setMessage(ui.breakLinks())}>
          {tl("Romper vínculo")}
        </Button>
      </div>
    </Section>
  );
};

/** Entidades del sketch: elegir por tipo, lista y propiedades de lo elegido */
const SketchEntities: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const sketch = (): Sketch => ui.session()!.sketch;
  const pos = (id: number): P2 => {
    const p = sketch().points.find((q) => q.id === id);
    return p ? [p.x, p.y] : [0, 0];
  };
  const fmtN = (v: number) => String(+v.toFixed(3));
  /** Una fila por entidad (un texto es una sola fila) */
  const rows = createMemo(() => {
    const s = sketch();
    const inText = new Map<number, number>();
    for (const t of s.texts ?? []) for (const e of t.entities) inText.set(e, t.id);
    const out: { key: string; label: string; measure: string; ids: number[]; construction: boolean }[] = [];
    for (const t of s.texts ?? []) out.push({ key: `t${t.id}`, label: `Texto «${t.text}»`, measure: `${fmtN(t.size)} mm`, ids: [...t.entities], construction: false });
    for (const e of s.entities) {
      if (inText.has(e.id)) continue;
      const g = e.geometry;
      let measure = "";
      if (g.type === "line") measure = fmtN(Math.hypot(pos(g.end)[0] - pos(g.start)[0], pos(g.end)[1] - pos(g.start)[1]));
      else if (g.type === "circle") measure = `Ø ${fmtN(2 * g.radius)}`;
      else if (g.type === "arc") measure = `R ${fmtN(Math.hypot(pos(g.start)[0] - pos(g.center)[0], pos(g.start)[1] - pos(g.center)[1]))}`;
      else if (g.type === "point") measure = pos(g.point).map(fmtN).join(", ");
      out.push({ key: `e${e.id}`, label: ENTITY_NAMES[g.type], measure, ids: [g.type === "point" ? g.point : e.id], construction: !!e.construction });
    }
    return out;
  });
  const MAX_ROWS = 300;
  const sel = () => ui.selection();
  const selected = (ids: number[]) => ids.every((id) => sel().includes(id));
  /** Lo elegido, si es una sola entidad (o un solo punto) */
  const single = () => {
    const ids = sel();
    if (ids.length !== 1) return undefined;
    const e = sketch().entities.find((x) => x.id === ids[0]);
    if (e) return e.geometry.type === "point" ? ({ kind: "point", point: e.geometry.point } as const) : ({ kind: "entity", e } as const);
    return sketch().points.some((p) => p.id === ids[0]) ? ({ kind: "point", point: ids[0] } as const) : undefined;
  };
  const PointFields = (p: { point: number; label?: string }) => (
    <div class="flex gap-1.5">
      <Num label={`${p.label ?? ""}X`} suffix="mm" step={1} value={pos(p.point)[0]} onCommit={(v) => ui.movePoint(p.point, [v, pos(p.point)[1]])} />
      <Num label={`${p.label ?? ""}Y`} suffix="mm" step={1} value={pos(p.point)[1]} onCommit={(v) => ui.movePoint(p.point, [pos(p.point)[0], v])} />
    </div>
  );
  return (
    <Section title={`Entidades (${rows().length})`}>
      <div class="flex flex-wrap gap-1" aria-label={tl("Elegir por tipo")}>
        <For each={SELECT_KINDS}>
          {(k) => (
            <Button size="sm" variant="ghost" onClick={() => ui.setSelection(selectByKind(sketch(), k.kind, ui.session()?.report?.free_entities ?? []))}>
              {k.label}
            </Button>
          )}
        </For>
      </div>
      <p class="text-[11px] text-text-dim">{tl("Doble clic en el visor elige la cadena unida; Ctrl+A, todo.")}</p>
      <div class="max-h-48 overflow-y-auto rounded border border-border" aria-label={tl("Lista de entidades")}>
        <For each={rows().slice(0, MAX_ROWS)}>
          {(r) => (
            <button
              class={clsx(
                "w-full flex items-center justify-between gap-2 px-2 py-0.5 text-[11px] text-left",
                selected(r.ids) ? "bg-accent/20 text-text" : "text-text-muted hover:bg-surface hover:text-text",
              )}
              onClick={(e) => ui.setSelection((cur) => (e.shiftKey ? (selected(r.ids) ? cur.filter((x) => !r.ids.includes(x)) : [...cur, ...r.ids]) : r.ids))}
              onMouseEnter={() => ui.setHoverIds(r.ids)}
              onMouseLeave={() => ui.setHoverIds([])}
            >
              <span class={r.construction ? "italic text-text-dim" : undefined}>
                {r.label}
                {r.construction ? " (constr.)" : ""}
              </span>
              <span class="font-mono text-text-dim">{r.measure}</span>
            </button>
          )}
        </For>
        <Show when={rows().length > MAX_ROWS}>
          <p class="px-2 py-0.5 text-[11px] text-text-dim">… y {rows().length - MAX_ROWS} {tl("más")}</p>
        </Show>
      </div>
      {/* Propiedades: escribir un valor mueve la geometría (las restricciones mandan) */}
      <Show when={single()}>
        {(it) => (
          <div class="space-y-1.5 border-t border-border pt-1.5" aria-label={tl("Propiedades")}>
            <Switch>
              <Match when={it().kind === "point" && it()}>{(p) => <PointFields point={(p() as { point: number }).point} />}</Match>
              <Match when={it().kind === "entity" && (it() as { e: SketchEntity }).e}>
                {(e) => {
                  const g = () => e().geometry;
                  return (
                    <Switch>
                      <Match when={g().type === "line" && (g() as Extract<Geometry, { type: "line" }>)}>
                        {(l) => {
                          const len = () => Math.hypot(pos(l().end)[0] - pos(l().start)[0], pos(l().end)[1] - pos(l().start)[1]);
                          return (
                            <>
                              <Num
                                label={tl("Largo")}
                                suffix="mm"
                                step={1}
                                value={len()}
                                onCommit={(v) => {
                                  const [a, b] = [pos(l().start), pos(l().end)];
                                  const k = len() > 0 ? v / len() : 0;
                                  if (v > 0) ui.movePoint(l().end, [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]);
                                }}
                              />
                              <PointFields point={l().start} label="Inicio " />
                              <PointFields point={l().end} label="Fin " />
                            </>
                          );
                        }}
                      </Match>
                      <Match when={g().type === "circle" && (g() as Extract<Geometry, { type: "circle" }>)}>
                        {(c) => (
                          <>
                            <PointFields point={c().center} label="Centro " />
                            <Num label={tl("Diámetro")} suffix="mm" step={1} value={2 * c().radius} onCommit={(v) => ui.setCircleRadius(e().id, v / 2)} />
                          </>
                        )}
                      </Match>
                      <Match when={g().type === "arc" && (g() as Extract<Geometry, { type: "arc" }>)}>
                        {(a) => {
                          const r = () => Math.hypot(pos(a().start)[0] - pos(a().center)[0], pos(a().start)[1] - pos(a().center)[1]);
                          return (
                            <>
                              <PointFields point={a().center} label="Centro " />
                              <Num
                                label={tl("Radio")}
                                suffix="mm"
                                step={1}
                                value={r()}
                                onCommit={(v) => {
                                  const [c, st] = [pos(a().center), pos(a().start)];
                                  const k = r() > 0 ? v / r() : 0;
                                  if (v > 0) ui.movePoint(a().start, [c[0] + (st[0] - c[0]) * k, c[1] + (st[1] - c[1]) * k]);
                                }}
                              />
                            </>
                          );
                        }}
                      </Match>
                    </Switch>
                  );
                }}
              </Match>
            </Switch>
          </div>
        )}
      </Show>
    </Section>
  );
};

/**
 * Herramientas de splines del panel: manijas y quitar puntos (punto de una
 * spline elegido), ajuste a puntos, simplificar, convertir a polos y el peine
 * de curvatura.
 */
/** Elige un archivo con el selector del sistema y lo lee como texto o como URL de datos */
function pickFile(accept: string, as: "text" | "data"): Promise<{ name: string; content: string; size: number } | undefined> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.setAttribute("data-sketch-file", accept);
    input.style.display = "none";
    input.onchange = () => {
      const f = input.files?.[0];
      input.remove();
      if (!f) return resolve(undefined);
      const r = new FileReader();
      r.onload = () => resolve({ name: f.name, content: String(r.result), size: f.size });
      r.onerror = () => resolve(undefined);
      if (as === "text") r.readAsText(f);
      else r.readAsDataURL(f);
    };
    document.body.appendChild(input);
    input.click();
  });
}

/** Imagen como URL de datos chica: las grandes se achican a 1600 px en JPEG (el calco viaja con el sketch) */
async function shrinkImage(data: string, size: number): Promise<{ data: string; w: number; h: number }> {
  const img = new Image();
  await new Promise<void>((ok, fail) => {
    img.onload = () => ok();
    img.onerror = () => fail(new Error(tl("no es una imagen")));
    img.src = data;
  });
  const [w, h] = [img.naturalWidth || 1, img.naturalHeight || 1];
  if (size <= 400_000 && Math.max(w, h) <= 1600) return { data, w, h };
  const k = Math.min(1, 1600 / Math.max(w, h));
  const c = document.createElement("canvas");
  c.width = Math.round(w * k);
  c.height = Math.round(h * k);
  c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
  return { data: c.toDataURL("image/jpeg", 0.85), w, h };
}

/** Curvas por ecuación y desde archivo, DXF y SVG, calcos */
const SketchFiles: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  const sketch = () => ui.session()!.sketch;
  const [kind, setKind] = createSignal<"explicit" | "parametric">("explicit");
  const [fx, setFx] = createSignal("10 * sin(x / 10)");
  const [fy, setFy] = createSignal("20 * sin(t)");
  const [from, setFrom] = createSignal(0);
  const [to, setTo] = createSignal(60);
  const [samples, setSamples] = createSignal(40);
  const [asLines, setAsLines] = createSignal(false);
  const [calLength, setCalLength] = createSignal(100);
  const params = () => Object.fromEntries((props.store.result()?.parameters ?? []).filter((p) => p.value != null).map((p) => [p.key, p.value!]));
  const name = () => (ui.session()?.sheet ? tl("hoja") : (props.store.doc()?.features.find((f) => f.id === ui.session()?.feature)?.name ?? "sketch"));

  /** Puntos → una spline por ellos (cerrada si vuelve al comienzo) o una polilínea */
  const addPoints = (pts: P2[], lines: boolean) => {
    const closed = pts.length > 2 && Math.hypot(pts[0][0] - pts[pts.length - 1][0], pts[0][1] - pts[pts.length - 1][1]) < 1e-9;
    let made: number[] = [];
    ui.change((sk) => {
      made = lines
        ? addShapes(sk, pts.slice(1).map((p, i) => ({ kind: "line" as const, a: pts[i], b: p })))
        : addShapes(sk, [{ kind: "spline", points: pts, closed }]);
    });
    ui.setSelection(made);
  };
  const addCurve = () => {
    const pts = sampleCurve({ kind: kind(), fx: fx(), fy: fy(), from: from(), to: to(), samples: samples() }, params());
    if (typeof pts === "string") return ui.setMessage(pts);
    ui.setMessage(undefined);
    addPoints(pts, false);
  };
  const fromCoordinates = async () => {
    const f = await pickFile(".csv,.txt,.xyz,.dat,text/plain", "text");
    if (!f) return;
    const pts = parseCoordinates(f.content);
    if (typeof pts === "string") return ui.setMessage(pts);
    addPoints(pts, asLines());
    ui.setMessage(tl("{length} puntos de {name}", { length: pts.length, name: f.name }));
  };
  const importDxf = async () => {
    const f = await pickFile(".dxf", "text");
    if (!f) return;
    const r = parseDxf(f.content);
    if (typeof r === "string") return ui.setMessage(r);
    let made: number[] = [];
    ui.change((sk) => (made = addShapes(sk, r.shapes)));
    ui.setSelection(made);
    const skipped = Object.entries(r.skipped).map(([k, n]) => `${n} ${k}`);
    ui.setMessage((skipped.length ? tl("{n} entidades de {name} (salteadas: {skipped})", { n: made.length, name: f.name, skipped: skipped.join(", ") }) : tl("{n} entidades de {name}", { n: made.length, name: f.name })));
  };
  const exportAs = async (ext: "dxf" | "svg") => {
    const path = await save({ filters: [{ name: ext.toUpperCase(), extensions: [ext] }], defaultPath: `${name().replace(/[^\p{L}\p{N}_-]+/gu, "_") || "sketch"}.${ext}` });
    if (!path) return;
    try {
      const bytes = await invoke<number>("cad_write_text", { path, content: ext === "dxf" ? sketchToDxf(sketch()) : sketchToSvg(sketch()) });
      ui.setMessage(`Exportado (${Math.max(1, Math.round(bytes / 1024))} KB)`);
    } catch (e) {
      ui.setMessage(String(e));
    }
  };
  const addImage = async () => {
    const f = await pickFile("image/*", "data");
    if (!f) return;
    try {
      const img = await shrinkImage(f.content, f.size);
      // 100 mm de ancho para empezar (después se calibra con dos puntos)
      ui.change((sk) => {
        const id = Math.max(0, ...(sk.images ?? []).map((i) => i.id)) + 1;
        sk.images = [...(sk.images ?? []), { id, name: f.name, data: img.data, at: [0, 0], width: 100, height: (100 * img.h) / img.w, angle: 0, opacity: 0.6 }];
      });
      ui.setMessage(tl("Calco agregado: calibrarlo con dos puntos de medida conocida"));
    } catch (e) {
      ui.setMessage(String(e));
    }
  };
  const setImage = (id: number, patch: Partial<SketchImage>) =>
    ui.change((sk) => {
      sk.images = (sk.images ?? []).map((i) => (i.id === id ? { ...i, ...patch } : i));
    });
  /** Escala el calco para que lo marcado mida `calLength` (desde el primer punto marcado) */
  const applyCalibration = () => {
    const cal = ui.calibration();
    if (!cal?.a || !cal.b) return;
    const d = Math.hypot(cal.b[0] - cal.a[0], cal.b[1] - cal.a[1]);
    if (!(d > 1e-9) || !(calLength() > 0)) return ui.setMessage(tl("Marcar dos puntos distintos y una medida positiva"));
    const k = calLength() / d;
    const a = cal.a;
    ui.change((sk) => {
      sk.images = (sk.images ?? []).map((i) =>
        i.id === cal.image ? { ...i, width: i.width * k, height: i.height * k, at: [a[0] + (i.at[0] - a[0]) * k, a[1] + (i.at[1] - a[1]) * k] as P2 } : i,
      );
    });
    ui.setCalibration(undefined);
    ui.setMessage(`Calco escalado ×${+k.toFixed(4)}`);
  };

  return (
    <Section title={tl("Archivos y fórmulas")}>
      <div class="space-y-1.5" aria-label={tl("Curva por ecuación")}>
        <Select
          options={[
            { value: "explicit", label: "y = f(x)" },
            { value: "parametric", label: "x(t), y(t)" },
          ]}
          value={kind()}
          onChange={(v) => setKind(v as "explicit" | "parametric")}
        />
        <input
          aria-label={kind() === "explicit" ? "y(x)" : "x(t)"}
          class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
          value={fx()}
          placeholder={kind() === "explicit" ? "y = …x…" : "x = …t…"}
          onChange={(e) => setFx(e.currentTarget.value)}
        />
        <Show when={kind() === "parametric"}>
          <input
            aria-label="y(t)"
            class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
            value={fy()}
            placeholder="y = …t…"
            onChange={(e) => setFy(e.currentTarget.value)}
          />
        </Show>
        <Num label={kind() === "explicit" ? tl("x desde") : tl("t desde")} step={1} value={from()} onCommit={setFrom} />
        <Num label={tl("hasta")} step={1} value={to()} onCommit={setTo} />
        <Num label={tl("Puntos")} step={5} min={2} value={samples()} onCommit={(v) => setSamples(Math.max(2, Math.round(v)))} />
        <p class="text-[11px] text-text-dim">
          {tl("Con + − * / ^, sin cos tan sqrt abs exp ln log, pi y los parámetros del diseño; ángulos en radianes.")}
        </p>
        <Button size="sm" onClick={addCurve}>
          {tl("Agregar curva")}
        </Button>
      </div>
      <div class="space-y-1.5 border-t border-border pt-1.5">
        <div class="flex flex-wrap items-center gap-1.5">
          <Button size="sm" title={tl("Un punto por fila: x, y (o x; y con coma decimal); la z se ignora")} onClick={() => void fromCoordinates()}>
            {tl("Curva desde archivo…")}
          </Button>
          <Checkbox small label={tl("Como polilínea")} checked={asLines()} onChange={setAsLines} />
        </div>
        <div class="flex flex-wrap gap-1.5">
          <Button size="sm" title={tl("Líneas, arcos, círculos, polilíneas, splines, elipses y puntos (DWG no: guardarlo como DXF)")} onClick={() => void importDxf()}>
            {tl("Importar DXF…")}
          </Button>
          <Button size="sm" onClick={() => void exportAs("dxf")}>
            {tl("Exportar DXF")}
          </Button>
          <Button size="sm" onClick={() => void exportAs("svg")}>
            {tl("Exportar SVG")}
          </Button>
        </div>
      </div>
      <div class="space-y-1.5 border-t border-border pt-1.5" aria-label={tl("Calcos")}>
        <Button size="sm" title={tl("Una imagen de referencia para dibujar encima (solo se ve al editar el sketch)")} onClick={() => void addImage()}>
          {tl("Agregar calco…")}
        </Button>
        <For each={sketch().images ?? []}>
          {(im) => (
            <div class="space-y-1 rounded border border-border p-1.5" data-sketch-image={im.id}>
              <div class="flex items-center gap-1.5 text-xs">
                <span class="flex-1 truncate text-text-muted" title={im.name}>
                  {im.name}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  title={tl("Clic en dos puntos de la imagen y después su medida real")}
                  onClick={() => {
                    ui.setCalibration({ image: im.id });
                    ui.setTool("calibrate");
                    ui.setMessage(tl("Clic en dos puntos de la imagen a una distancia conocida"));
                  }}
                >
                  {tl("Calibrar")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => ui.change((sk) => (sk.images = (sk.images ?? []).filter((i) => i.id !== im.id)))}>
                  {tl("Quitar")}
                </Button>
              </div>
              <Show when={ui.calibration()?.image === im.id && ui.calibration()?.b}>
                <div class="flex items-end gap-1.5">
                  <div class="flex-1 min-w-0">
                    <Num label={tl("Eso mide")} suffix="mm" step={1} value={calLength()} onCommit={setCalLength} />
                  </div>
                  <Button size="sm" variant="primary" onClick={applyCalibration}>
                    {tl("Aplicar la medida")}
                  </Button>
                </div>
              </Show>
              <Num label={tl("Ancho")} suffix="mm" step={10} value={im.width} onCommit={(v) => v > 0 && setImage(im.id, { width: v, height: (im.height * v) / im.width })} />
              <Num label={tl("Giro")} suffix="°" step={5} value={im.angle ?? 0} onCommit={(v) => setImage(im.id, { angle: v })} />
              <div class="flex gap-1.5">
                <Num label="X" suffix="mm" step={5} value={im.at[0]} onCommit={(v) => setImage(im.id, { at: [v, im.at[1]] })} />
                <Num label="Y" suffix="mm" step={5} value={im.at[1]} onCommit={(v) => setImage(im.id, { at: [im.at[0], v] })} />
              </div>
              <Slider value={im.opacity ?? 0.6} min={0.05} max={1} step={0.05} onChange={(v) => setImage(im.id, { opacity: v })} />
            </div>
          )}
        </For>
      </div>
    </Section>
  );
};

/** Capas, bloques y copiar el sketch entero */
const SketchOrganize: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  const [gearPick, setGearPick] = createSignal<string>("");
  const sketch = () => ui.session()!.sketch;
  const [blockName, setBlockName] = createSignal("");
  const [insertAngle, setInsertAngle] = createSignal(0);
  const [insertScale, setInsertScale] = createSignal(1);
  const sel = () => ui.selection();
  /** La copia de bloque elegida (por una de sus curvas o su ancla) */
  const selectedCopy = () => (sketch().texts ?? []).find((t) => t.block != null && sel().some((id) => t.anchor === id || t.entities.includes(id)));
  const copies = (block: number) => (sketch().texts ?? []).filter((t) => t.block === block).length;
  const setLayerProp = (id: number, patch: Partial<SketchLayer>) =>
    ui.change((sk) => {
      sk.layers = (sk.layers ?? []).map((l) => (l.id === id ? { ...l, ...patch } : l));
    });

  return (
    <Section title={tl("Organización")}>
      <div class="space-y-1" aria-label={tl("Capas")}>
        <div class="flex items-center gap-1.5">
          <span class="flex-1 text-xs text-text-muted">{tl("Capas")}</span>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              let id = 0;
              ui.change((sk) => (id = addLayer(sk, tl("Capa {v1}", { v1: (sk.layers?.length ?? 0) + 1 }))));
              ui.setActiveLayer(id);
            }}
          >
            {tl("Nueva capa")}
          </Button>
        </div>
        <For each={sketch().layers ?? []}>
          {(l) => (
            <div class="flex items-center gap-1 text-xs" data-layer={l.id}>
              <input
                type="radio"
                name="capa-activa"
                aria-label={tl("Dibujar en {name}", { name: l.name })}
                title={tl("Lo que se dibuja cae en esta capa")}
                checked={ui.activeLayer() === l.id}
                onChange={() => ui.setActiveLayer(l.id)}
              />
              <input
                class="flex-1 min-w-0 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                value={l.name}
                onChange={(e) => setLayerProp(l.id, { name: e.currentTarget.value || l.name })}
              />
              <button aria-label={`Ver ${l.name}`} aria-pressed={!l.hidden} class={clsx("px-1 rounded hover:bg-surface", l.hidden && "text-text-dim line-through")} onClick={() => setLayerProp(l.id, { hidden: !l.hidden })}>
                {tl("Ver")}
              </button>
              <button aria-label={tl("Bloquear {name}", { name: l.name })} aria-pressed={!!l.locked} class={clsx("px-1 rounded hover:bg-surface", l.locked ? "text-warning" : "text-text-dim")} onClick={() => setLayerProp(l.id, { locked: !l.locked })}>
                {l.locked ? tl("Bloq.") : tl("Libre")}
              </button>
              <IconButton
                aria-label={tl("Borrar {name}", { name: l.name })}
                size="sm"
                variant="ghost"
                onClick={() => {
                  ui.change((sk) => removeLayer(sk, l.id));
                  if (ui.activeLayer() === l.id) ui.setActiveLayer(undefined);
                }}
              >
                <Icons.X size={11} />
              </IconButton>
            </div>
          )}
        </For>
        <Show when={(sketch().layers?.length ?? 0) > 0}>
          <label class="flex items-center gap-1.5 text-xs text-text-muted">
            <input type="radio" name="capa-activa" checked={ui.activeLayer() === undefined} onChange={() => ui.setActiveLayer(undefined)} />
            {tl("Dibujar sin capa")}
          </label>
          <Show when={sel().length > 0}>
            <div class="flex flex-wrap items-center gap-1 text-xs" aria-label={tl("Pasar lo elegido a una capa")}>
              <span class="text-text-muted">{tl("Pasar lo elegido a:")}</span>
              <For each={sketch().layers ?? []}>
                {(l) => (
                  <Button size="sm" variant="ghost" onClick={() => ui.change((sk) => setLayer(sk, sel(), l.id))}>
                    {l.name}
                  </Button>
                )}
              </For>
              <Button size="sm" variant="ghost" onClick={() => ui.change((sk) => setLayer(sk, sel(), null))}>
                {tl("Sin capa")}
              </Button>
            </div>
          </Show>
        </Show>
      </div>
      <div class="space-y-1.5 border-t border-border pt-1.5" aria-label={tl("Bloques")}>
        <Show when={sel().length > 0 && !selectedCopy()}>
          <div class="flex items-center gap-1.5">
            <input
              aria-label={tl("Nombre del bloque")}
              class="flex-1 min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
              placeholder={tl("Bloque {v1}", { v1: (sketch().blocks?.length ?? 0) + 1 })}
              value={blockName()}
              onInput={(e) => setBlockName(e.currentTarget.value)}
            />
            <Button
              size="sm"
              title={tl("Lo elegido pasa a ser un bloque (origen: el punto elegido o el centro); se inserta varias veces y editarlo cambia todas las copias")}
              onClick={() => {
                const ids = sel();
                const nm = blockName().trim() || tl("Bloque {v1}", { v1: (sketch().blocks?.length ?? 0) + 1 });
                let msg: string | undefined;
                ui.change((sk) => {
                  const r = createBlock(sk, ids, nm);
                  if (typeof r === "string") msg = r;
                });
                ui.setMessage(msg);
                if (!msg) setBlockName("");
              }}
            >
              {tl("Hacer bloque")}
            </Button>
          </div>
        </Show>
        <Show when={selectedCopy()}>
          {(t) => (
            <Button size="sm" title={tl("La copia queda suelta (con las restricciones del bloque) y deja de cambiar con él")} onClick={() => ui.change((sk) => explodeBlock(sk, t().id))}>
              {tl("Desarmar la copia de «")}{t().text}»
            </Button>
          )}
        </Show>
        <For each={sketch().blocks ?? []}>
          {(b) => (
            <div class="flex items-center gap-1 text-xs" data-block={b.id}>
              <span class="flex-1 truncate text-text-muted">
                {b.name} <span class="text-text-dim">· {copies(b.id)} {tl("copias")}</span>
              </span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  ui.setBlockToInsert({ block: b.id, angle: insertAngle(), scale: insertScale() });
                  ui.setTool("block_insert");
                  ui.setMessage(tl("Clic donde va el origen de «{name}» (Esc termina)", { name: b.name }));
                }}
              >
                {tl("Insertar")}
              </Button>
              <Button size="sm" variant="ghost" disabled={!!ui.blockEdit()} onClick={() => ui.editBlock(b.id)}>
                {tl("Editar")}
              </Button>
              <Show when={copies(b.id) === 0}>
                <IconButton aria-label={tl("Borrar el bloque {name}", { name: b.name })} size="sm" variant="ghost" onClick={() => ui.change((sk) => (sk.blocks = (sk.blocks ?? []).filter((x) => x.id !== b.id)))}>
                  <Icons.X size={11} />
                </IconButton>
              </Show>
            </div>
          )}
        </For>
        <Show when={(props.store.result()?.gears ?? []).length > 0}>
          <div class="flex items-center gap-1.5 text-xs" aria-label={tl("Perfil de engrane")}>
            <div class="flex-1 min-w-0">
              <Select
                options={[
                  { value: "", label: tl("Perfil de un engrane…") },
                  ...(props.store.result()?.gears ?? []).filter((x) => x.part.spec.kind !== "worm").map((x) => ({ value: String(x.feature), label: props.store.doc()?.features.find((f) => f.id === x.feature)?.name ?? `#${x.feature}` })),
                ]}
                value={gearPick()}
                onChange={setGearPick}
              />
            </div>
            <Button
              size="sm"
              disabled={!gearPick()}
              title={tl("Trae el contorno del engrane (flancos exactos, pie y cabeza) como un bloque rígido: para cortar con láser, extruir a mano o exportar a DXF")}
              onClick={() => {
                const g = (props.store.result()?.gears ?? []).find((x) => String(x.feature) === gearPick());
                if (!g) return;
                void props.store.gearBlock(g.part).then(
                  (block) => {
                    let id = 0;
                    ui.change((sk) => {
                      const same = (sk.blocks ?? []).find((b) => b.name === block.name);
                      if (same) {
                        // Mismo engrane: se actualiza el bloque (y sus copias) con las medidas de ahora
                        Object.assign(same, { points: block.points, entities: block.entities, constraints: block.constraints });
                        refreshBlock(sk, same.id);
                        id = same.id;
                      } else {
                        id = Math.max(0, ...(sk.blocks ?? []).map((b) => b.id)) + 1;
                        sk.blocks = [...(sk.blocks ?? []), { ...block, id }];
                      }
                    });
                    ui.setBlockToInsert({ block: id, angle: insertAngle(), scale: 1 });
                    ui.setTool("block_insert");
                    ui.setMessage(tl("Clic donde va el centro de «{name}» (Esc termina)", { name: block.name }));
                  },
                  (e) => ui.setMessage(String(e)),
                );
              }}
            >
              {tl("Traer")}
            </Button>
          </div>
        </Show>
        <Show when={(sketch().blocks?.length ?? 0) > 0}>
          <div class="space-y-1" aria-label={tl("Copias nuevas")}>
            <Num label={tl("Giro")} suffix="°" step={15} value={insertAngle()} onCommit={(v) => {
              setInsertAngle(v);
              const bi = ui.blockToInsert();
              if (bi) ui.setBlockToInsert({ ...bi, angle: v });
            }} />
            <Num label={tl("Escala")} suffix="×" step={0.5} value={insertScale()} onCommit={(v) => {
              if (!(v > 0)) return;
              setInsertScale(v);
              const bi = ui.blockToInsert();
              if (bi) ui.setBlockToInsert({ ...bi, scale: v });
            }} />
          </div>
        </Show>
      </div>
      <div class="border-t border-border pt-1.5">
        <Button
          size="sm"
          title={tl("Para pegarlo como un sketch nuevo en otro plano o cara («Pegar sketch» en la barra de Diseño)")}
          onClick={() => {
            const copy: Sketch = structuredClone(sketch());
            // Lo ligado al modelo y lo que nombra otras operaciones no viaja
            breakLinks(copy, copy.entities.map((e) => e.id));
            copy.constraints = copy.constraints.filter((c) => c.type !== "pierce");
            delete copy.x_axis;
            delete copy.flip_normal;
            ui.setSketchCopy(copy);
            ui.setMessage(tl("Sketch copiado: «Pegar sketch» en la barra de Diseño"));
          }}
        >
          {tl("Copiar el sketch entero")}
        </Button>
      </div>
    </Section>
  );
};

const SplineTools: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const [tol, setTol] = createSignal(0.05);
  const sketch = () => ui.session()!.sketch;
  const sel = () => ui.selection();
  const ents = () => sel().map((id) => sketch().entities.find((e) => e.id === id)).filter((e) => e !== undefined);
  const pts = () => sel().filter((id) => sketch().points.some((p) => p.id === id));
  const splines = () => ents().filter((e) => e.geometry.type === "spline" || e.geometry.type === "bspline");
  /** Spline (por puntos o por polos) a la que pertenece un punto elegido */
  const owner = () => {
    if (pts().length !== 1 || ents().length) return undefined;
    const p = pts()[0];
    const e = sketch().entities.find((x) => (x.geometry.type === "spline" && x.geometry.points.includes(p)) || (x.geometry.type === "bspline" && x.geometry.poles.includes(p)));
    return e && { entity: e.id, point: p, kind: e.geometry.type };
  };
  const convertible = () => ents().length > 0 && ents().every((e) => ["line", "arc", "spline"].includes(e.geometry.type)) && !ents().every((e) => e.geometry.type === "line");
  const run = (f: (sk: Sketch) => string | number | undefined, ok?: (r: string | number | undefined) => string | undefined) => {
    let r: string | number | undefined;
    ui.change((sk) => void (r = f(sk)));
    ui.setMessage(typeof r === "string" ? r : ok?.(r));
  };
  const comb = () => {
    const id = ui.combEntity();
    return id !== undefined ? splineCurvature(sketch(), id, 1) : undefined;
  };
  return (
    <Show when={owner() || splines().length || pts().length >= 3 || convertible() || ui.combEntity() !== undefined}>
      <div class="space-y-1.5 border-t border-border pt-1.5" data-spline-tools>
        <div class="text-[11px] text-text-dim">{tl("Splines")}</div>
        <div class="flex flex-wrap gap-1.5">
          <Show when={owner()}>
            {(o) => (
              <>
                <Show when={o().kind === "spline"}>
                  <Button size="sm" title={tl("Un punto que da la dirección de la curva en ese punto (se arrastra)")} onClick={() => run((sk) => toggleSplineHandle(sk, o().entity, o().point))}>
                    {tl("Manija en el punto sí/no")}
                  </Button>
                </Show>
                <Button size="sm" onClick={() => run((sk) => removeSplinePoint(sk, o().entity, o().point), () => void ui.setSelection([]))}>
                  {o().kind === "spline" ? tl("Quitar el punto de la spline") : tl("Quitar el polo")}
                </Button>
              </>
            )}
          </Show>
          <Show when={pts().length >= 3 && !ents().length}>
            <Button
              size="sm"
              title={tl("Spline por polos que pasa por el primero y el último y se acerca a los demás (en el orden en que se eligieron)")}
              onClick={() =>
                run(
                  (sk) => fitSplineToPoints(sk, pts(), tol()),
                  (id) => (typeof id === "number" ? void ui.setSelection([id]) : undefined),
                )
              }
            >
              {tl("Spline de ajuste")}
            </Button>
          </Show>
          <Show when={splines().length === 1}>
            <Button
              size="sm"
              title={tl("Saca puntos (o polos) mientras la curva no se aparte más que la tolerancia")}
              onClick={() => run((sk) => simplifySpline(sk, splines()[0].id, tol()), (n) => (typeof n === "number" ? (n ? tl("Se sacaron {n}", { n }) : tl("No hay nada que sacar con esa tolerancia")) : undefined))}
            >
              {tl("Simplificar")}
            </Button>
            <Button size="sm" aria-pressed={ui.combEntity() === splines()[0].id} onClick={() => ui.setCombEntity(ui.combEntity() === splines()[0].id ? undefined : splines()[0].id)}>
              {tl("Peine de curvatura")}
            </Button>
          </Show>
          <Show when={convertible()}>
            <Button
              size="sm"
              title={tl("Líneas, arcos y splines por puntos unidos por los extremos pasan a una spline por polos (los arcos, con un error de ~0,03 % del radio)")}
              onClick={() =>
                run(
                  (sk) => convertToBSpline(sk, ents().map((e) => e.id)),
                  (id) => (typeof id === "number" ? void ui.setSelection([id]) : undefined),
                )
              }
            >
              {tl("Convertir a spline por polos")}
            </Button>
          </Show>
          <Show when={splines().length === 1 && splines()[0].geometry.type === "bspline"}>
            <Button
              size="sm"
              title={tl("Pasa la spline por polos a una por puntos de paso sobre la curva (se aparta a lo sumo 0,5 % de su tamaño)")}
              onClick={() => run((sk) => bsplineToThrough(sk, splines()[0].id))}
            >
              {tl("A puntos de paso")}
            </Button>
          </Show>
        </div>
        <label class="flex items-center gap-1 text-[11px] text-text-muted">
          {tl("Tolerancia")}
          <input
            type="number"
            min="0.0001"
            step="0.01"
            value={tol()}
            aria-label={tl("Tolerancia de las splines")}
            class="w-16 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
            onChange={(e) => setTol(Math.max(1e-4, parseFloat(e.currentTarget.value) || 0.05))}
          />
          mm
        </label>
        <Show when={comb()}>
          {(c) => (
            <p class="text-[11px] text-text-muted" data-curvature>
              {tl("Radio mínimo")} {Number.isFinite(c().minRadius) ? `${+c().minRadius.toFixed(3)} mm` : "— (recta)"} · {c().inflections.length} {tl("inflexiones")}
              <button class="ml-1 underline hover:text-text" onClick={() => ui.setCombEntity(undefined)}>
                {tl("ocultar")}
              </button>
            </p>
          )}
        </Show>
      </div>
    </Show>
  );
};

/**
 * Informe de la revisión del contorno, navegable: cada problema es un
 * renglón; el clic elige lo que lo causa y centra la vista ahí.
 */
function ProblemList(props: { ui: CadUi; sketch: Sketch }) {
  const ui = props.ui;
  const items = createMemo(() => sketchIssues(props.sketch, ui.session()?.report));
  return (
    <Show when={items().length}>
      <details class="text-xs" data-problem-list>
        <summary class="cursor-pointer text-text-muted">{tl("Ver la lista (")}{items().length})</summary>
        <div class="max-h-40 overflow-auto mt-1 space-y-0.5">
          <For each={items()}>
            {(it) => (
              <button
                class={clsx("block w-full text-left px-1 py-0.5 rounded hover:bg-surface/60", it.kind === "error" ? "text-error" : it.kind === "warning" ? "text-warning" : "text-text-muted")}
                data-problem={it.kind}
                onClick={() => {
                  ui.setSelection(it.ids);
                  ui.viewer()?.focusSketch?.(it.at, 20);
                  // Un conflicto que se había dejado vuelve a tener su aviso
                  if (it.constraint !== undefined) ui.reviewConflict(it.constraint);
                }}
              >
                {it.text}
              </button>
            )}
          </For>
        </div>
      </details>
    </Show>
  );
}
