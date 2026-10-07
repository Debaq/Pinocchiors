import { Component, For, Show, type JSX } from "solid-js";
import { clsx } from "clsx";
import { Panel, Slider } from "../ui";
import {
  ARTHROPOD_ICONS,
  FEET_ICONS,
  LIMB_BITS,
  LIMB_ICONS,
  NECK_ICONS,
  PART_ICONS,
  POSTURE_ICONS,
  RADIAL_ICONS,
  SHAPE_ICONS,
} from "./bodyIcons";
import type { BodyPlan, BodyShape } from "../../lib/bodyPlan";
import type { SkeletonPreset } from "../panels/SkeletonPanel";

type CountField =
  | "neck"
  | "tail"
  | "trunk"
  | "ears"
  | "wings"
  | "limbs"
  | "limb_segments"
  | "antennae"
  | "horns"
  | "tusks"
  | "tentacles"
  | "abdomen"
  | "mantle";
type FlagField = "fins" | "pincers" | "jaw" | "flukes" | "fangs" | "palps" | "eye_stalks" | "segmented";

const SHAPES: { id: BodyShape; label: string; hint: string }[] = [
  { id: "biped", label: "Bípedo", hint: "Humanoides y primates" },
  { id: "digitigrade", label: "Bípedo horizontal", hint: "Dinosaurios, aves corredoras, canguro" },
  { id: "quadruped", label: "Cuadrúpedo", hint: "Mamíferos de cuatro patas, reptiles grandes" },
  { id: "radial", label: "Radial", hint: "Pulpo, calamar, estrella de mar" },
  { id: "fish", label: "Pez", hint: "Peces, delfín, ballena" },
  { id: "arthropod", label: "Artrópodo", hint: "Insectos, arañas, cangrejos, escorpiones" },
  { id: "serpent", label: "Serpiente", hint: "Serpiente, gusano, anguila" },
  { id: "tree", label: "Árbol", hint: "Plantas, cuerdas, objetos con ramas" },
];

const FEET: { id: NonNullable<BodyPlan["feet"]>; label: string; hint: string }[] = [
  { id: "simple", label: "Simple", hint: "Dos tramos: la plantilla básica (elefante)" },
  { id: "plantigrade", label: "Planta", hint: "Apoya la planta entera: oso, mapache" },
  { id: "digitigrade", label: "Dedos", hint: "Muñeca y corvejón altos: perro, felino" },
  { id: "unguligrade", label: "Casco", hint: "Carpo, corvejón y menudillo: caballo, camélidos, ciervo" },
];

const NECKS: { id: NonNullable<BodyPlan["neck_shape"]>; label: string; hint: string }[] = [
  { id: "rising", label: "Sube", hint: "Sale de los hombros hacia arriba y adelante" },
  { id: "swan", label: "En U", hint: "Baja delante de los hombros y vuelve a subir: camello" },
  { id: "upright", label: "Vertical", hint: "Recto hacia arriba: llama, alpaca" },
  { id: "level", label: "Al frente", hint: "Horizontal, la cabeza adelante: reptiles, tortuga" },
];

/** Cantidad al activar una parte que estaba en 0 */
const DEFAULT_COUNT: Partial<Record<CountField, number>> = {
  ears: 1,
  horns: 2,
  tusks: 2,
  trunk: 8,
  wings: 3,
  antennae: 2,
  tentacles: 1,
  abdomen: 2,
};

const RADIAL_POSES: { id: NonNullable<BodyPlan["radial_pose"]>; label: string; hint: string }[] = [
  { id: "spread", label: "Extendidos", hint: "Brazos sobre el suelo alrededor del cuerpo: pulpo, calamar" },
  { id: "hanging", label: "Colgando", hint: "Brazos que cuelgan de una campana: medusa" },
  { id: "flat", label: "En estrella", hint: "Brazos planos y el cuerpo pegado al suelo: estrella de mar" },
  { id: "up", label: "Hacia arriba", hint: "Tentáculos que se abren desde un disco sobre una columna: anémona" },
];

export interface BodyBuilderProps {
  /** Plan del esqueleto actual; sin plan solo se elige la forma base */
  plan?: BodyPlan;
  onChange?: (plan: BodyPlan) => void;
  /** Empezar un cuerpo desde cero con esta forma */
  onShape?: (shape: BodyShape) => void;
  /** Plantillas con plan (`plan:<id>`) para partir de una */
  presets: SkeletonPreset[];
  selectedPreset?: string;
  onPreset?: (presetId: string) => void;
  disabled?: boolean;
}

/** Nombre de la extremidad de cada bit (lado del personaje) */
const limbLabel = (bit: number, quadruped: boolean) =>
  quadruped
    ? { 1: "Delantera izq.", 2: "Delantera der.", 4: "Trasera izq.", 8: "Trasera der." }[bit]!
    : { 1: "Brazo izq.", 2: "Brazo der.", 4: "Pierna izq.", 8: "Pierna der." }[bit]!;

// ─── Piezas ─────────────────────────────────────────────────────────────────

/** Tarjetas de al menos 84 px: tantas por fila como entren en la columna */
const GRID = "grid gap-2 grid-cols-[repeat(auto-fill,minmax(84px,1fr))]";

/** Sección plegable (recuerda si quedó abierta) */
const Section: Component<{ title: string; children: JSX.Element }> = (props) => (
  <Panel title={props.title} id={`body.${props.title}`} dense>
    <div class="space-y-3 pb-3">{props.children}</div>
  </Panel>
);

/** Botón con pictograma, para elegir una opción o prender una parte */
const Tile: Component<{
  icon: Component;
  label: string;
  hint: string;
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  children?: JSX.Element;
}> = (props) => (
  <div
    class={clsx(
      "min-w-0 rounded-md border transition-colors duration-100 flex flex-col items-center",
      props.active ? "border-accent bg-accent/10 text-text" : "border-border text-text-muted hover:border-border-hover hover:text-text",
      props.disabled && "opacity-50 pointer-events-none"
    )}
  >
    <button
      type="button"
      class="w-full flex flex-col items-center gap-1 px-1 pt-2 pb-1.5"
      title={props.hint}
      aria-pressed={props.active}
      onClick={() => props.onClick()}
    >
      <span class="[&>svg]:w-14 [&>svg]:h-14">
        <props.icon />
      </span>
      <span class="w-full text-[11px] leading-tight text-center break-words">{props.label}</span>
    </button>
    {props.children}
  </div>
);

/** − n + dentro de una tarjeta */
const Stepper: Component<{ value: number; min: number; max: number; onChange: (n: number) => void; disabled?: boolean }> = (props) => {
  const button = "w-4 h-5 rounded flex items-center justify-center hover:bg-surface/60 disabled:opacity-30";
  return (
    <div class="flex items-center gap-0.5 pb-1 text-[11px] font-mono text-text">
      <button type="button" class={button} disabled={props.disabled || props.value <= props.min} onClick={() => props.onChange(props.value - 1)} title="Menos segmentos">
        −
      </button>
      <span class="w-4 text-center">{props.value}</span>
      <button type="button" class={button} disabled={props.disabled || props.value >= props.max} onClick={() => props.onChange(props.value + 1)} title="Más segmentos">
        +
      </button>
    </div>
  );
};

/** Fila con nombre y − n + (cantidades que no se apagan) */
const CountRow: Component<{ label: string; value: number; min: number; max: number; onChange: (n: number) => void; disabled?: boolean }> = (props) => (
  <div class="flex items-center justify-between gap-2 text-xs text-text">
    <span>{props.label}</span>
    <div class="rounded-md border border-border px-1 pt-1">
      <Stepper value={props.value} min={props.min} max={props.max} onChange={props.onChange} disabled={props.disabled} />
    </div>
  </div>
);

// ─── Constructor ────────────────────────────────────────────────────────────

/**
 * Constructor de cuerpos (pestaña Crear del editor de esqueleto): forma
 * base, plantillas de partida, postura, patas, cuello y partes que se
 * prenden o apagan. Cada cambio rehace la plantilla.
 */
export const BodyBuilder: Component<BodyBuilderProps> = (props) => {
  const plan = () => props.plan;
  const update = (partial: Partial<BodyPlan>) => {
    const current = plan();
    if (current) props.onChange?.({ ...current, ...partial });
  };
  const count = (field: CountField) => plan()?.[field] ?? 0;
  const shape = () => plan()?.shape;
  const headed = () => ["biped", "digitigrade", "quadruped"].includes(shape() ?? "");
  // Lo último que tuvo cada parte antes de apagarla, para prenderla igual
  const last = new Map<CountField, number>();

  /** Tarjeta de una parte con segmentos: clic la prende o la apaga */
  const Part: Component<{ field: CountField; icon: string; label: string; hint: string; min?: number; max: number }> = (p) => (
    <Tile
      icon={PART_ICONS[p.icon] ?? ARTHROPOD_ICONS[p.icon]}
      label={p.label}
      hint={p.hint}
      active={count(p.field) > 0}
      disabled={props.disabled}
      onClick={() => {
        const n = count(p.field);
        if (n > 0) last.set(p.field, n);
        update({ [p.field]: n > 0 ? 0 : (last.get(p.field) ?? DEFAULT_COUNT[p.field] ?? Math.max(p.min ?? 1, 1)) } as Partial<BodyPlan>);
      }}
    >
      <Show when={count(p.field) > 0}>
        <Stepper value={count(p.field)} min={p.min ?? 1} max={p.max} onChange={(n) => update({ [p.field]: n } as Partial<BodyPlan>)} disabled={props.disabled} />
      </Show>
    </Tile>
  );

  /** Tarjeta de una parte sin segmentos */
  const Flag: Component<{ field: FlagField; icon: string; label: string; hint: string }> = (p) => (
    <Tile
      icon={PART_ICONS[p.icon] ?? ARTHROPOD_ICONS[p.icon]}
      label={p.label}
      hint={p.hint}
      active={!!plan()?.[p.field]}
      disabled={props.disabled}
      onClick={() => update({ [p.field]: !plan()?.[p.field] } as Partial<BodyPlan>)}
    />
  );

  const percent = (v: number) => `${Math.round(v * 100)} %`;
  const Ratio: Component<{ field: "leg_length" | "arm_length" | "tail_length" | "body_width"; label: string; max: number }> = (p) => (
    <Slider
      label={p.label}
      value={plan()?.[p.field] ?? 1}
      onChange={(v) => update({ [p.field]: Math.round(v * 100) / 100 } as Partial<BodyPlan>)}
      min={0.5}
      max={p.max}
      step={0.05}
      formatValue={percent}
      disabled={props.disabled}
    />
  );

  const neckRow = () => (
    <CountRow label="Segmentos del cuello" value={Math.max(count("neck"), 1)} min={1} max={10} onChange={(neck) => update({ neck })} disabled={props.disabled} />
  );

  const templates = () => props.presets.filter((p) => p.id.startsWith("plan:"));

  return (
    <div>

        <Section title="Forma base">
          <div class={GRID}>
            <For each={SHAPES}>
              {(s) => (
                <Tile
                  icon={SHAPE_ICONS[s.id]}
                  label={s.label}
                  hint={s.hint}
                  active={shape() === s.id}
                  disabled={props.disabled}
                  onClick={() => shape() !== s.id && props.onShape?.(s.id)}
                />
              )}
            </For>
          </div>
        </Section>

        <Section title="Partir de una plantilla">
          <div class="flex flex-wrap gap-1.5">
            <For each={templates()}>
              {(t) => (
                <button
                  type="button"
                  title={t.description}
                  disabled={props.disabled}
                  class={clsx(
                    "px-2 h-6 rounded-full border text-[11px] transition-colors",
                    t.id === props.selectedPreset
                      ? "border-accent bg-accent/15 text-accent"
                      : "border-border text-text-muted hover:border-border-hover hover:text-text"
                  )}
                  onClick={() => props.onPreset?.(t.id)}
                >
                  {t.name}
                </button>
              )}
            </For>
          </div>
        </Section>

        <Show when={shape() === "quadruped"}>
          <Section title="Postura">
            <div class={GRID}>
              <Tile
                icon={POSTURE_ICONS.under}
                label="Bajo el cuerpo"
                hint="Patas verticales bajo el cuerpo: mamíferos"
                active={!plan()?.sprawl}
                disabled={props.disabled}
                onClick={() => update({ sprawl: false })}
              />
              <Tile
                icon={POSTURE_ICONS.sprawl}
                label="Al costado"
                hint="Patas abiertas y cuerpo cerca del suelo: lagarto, cocodrilo, tortuga, salamandra"
                active={!!plan()?.sprawl}
                disabled={props.disabled}
                // Abiertas solo hay simple o con planta (dedos y casco se arman como planta)
                onClick={() => update({ sprawl: true, feet: (plan()?.feet ?? "simple") === "simple" ? "simple" : "plantigrade" })}
              />
            </div>
          </Section>
          <Section title="Patas">
            <div class={GRID}>
              <For each={plan()?.sprawl ? FEET.slice(0, 2) : FEET}>
                {(f) => (
                  <Tile
                    icon={FEET_ICONS[f.id]}
                    label={f.label}
                    hint={f.hint}
                    active={(plan()?.feet ?? "simple") === f.id}
                    disabled={props.disabled}
                    onClick={() => update({ feet: f.id })}
                  />
                )}
              </For>
            </div>
          </Section>
          <Section title="Cuello">
            <div class={GRID}>
              <For each={NECKS}>
                {(n) => (
                  <Tile
                    icon={NECK_ICONS[n.id]}
                    label={n.label}
                    hint={n.hint}
                    active={(plan()?.neck_shape ?? "rising") === n.id}
                    disabled={props.disabled}
                    onClick={() => update({ neck_shape: n.id })}
                  />
                )}
              </For>
            </div>
            {neckRow()}
          </Section>
        </Show>
        <Show when={headed() && shape() !== "quadruped"}>
          <Section title="Cuello">{neckRow()}</Section>
        </Show>

        <Show when={headed()}>
          <Section title="Extremidades">
            <div class={GRID}>
              <For each={LIMB_BITS.filter((bit) => shape() !== "digitigrade" || bit > 2 || count("wings") === 0)}>
                {(bit) => {
                  const quad = () => shape() === "quadruped";
                  const label = () => limbLabel(bit, quad());
                  return (
                    <Tile
                      icon={LIMB_ICONS[quad() ? "quadruped" : "biped"][bit]}
                      label={label()}
                      hint={`${label()}: apágala si al modelo le falta (queda el ${bit <= 2 ? "hombro" : "cadera"} para el muñón)`}
                      active={((plan()?.missing ?? 0) & bit) === 0}
                      disabled={props.disabled}
                      onClick={() => update({ missing: (plan()?.missing ?? 0) ^ bit })}
                    />
                  );
                }}
              </For>
            </div>
          </Section>
          <Section title="Cabeza">
            <div class={GRID}>
              <Part field="ears" icon="ears" label="Orejas" hint="Orejas que se mueven" max={3} />
              <Part field="horns" icon="horns" label="Cuernos" hint="Par de cuernos o astas sobre la cabeza" max={5} />
              <Part field="tusks" icon="tusks" label="Colmillos" hint="Colmillos hacia adelante y abajo" max={4} />
              <Part field="trunk" icon="trunk" label="Trompa" hint="Trompa: más segmentos la doblan más suave" max={12} />
              <Flag field="jaw" icon="jaw" label="Mandíbula" hint="Un hueso que abre la boca" />
            </div>
            <CountRow
              label="Cabezas (cada una con su cuello)"
              value={Math.max(plan()?.heads ?? 1, 1)}
              min={1}
              max={5}
              onChange={(heads) => update({ heads })}
              disabled={props.disabled}
            />
          </Section>
        </Show>

        <Show when={headed() || shape() === "fish"}>
          <Section title="Cuerpo">
            <div class={GRID}>
              <Show when={shape() === "quadruped"}>
                <Tile icon={PART_ICONS.hump1} label="Una joroba" hint="Dromedario" active={(plan()?.humps ?? 0) === 1} disabled={props.disabled} onClick={() => update({ humps: plan()?.humps === 1 ? 0 : 1 })} />
                <Tile icon={PART_ICONS.hump2} label="Dos jorobas" hint="Camello bactriano" active={(plan()?.humps ?? 0) >= 2} disabled={props.disabled} onClick={() => update({ humps: (plan()?.humps ?? 0) >= 2 ? 0 : 2 })} />
              </Show>
              <Part
                field="tail"
                icon={shape() === "biped" ? "bipedTail" : shape() === "fish" ? "fishTail" : "tail"}
                label="Cola"
                hint="Cola: más segmentos la doblan más suave (prensil: muchos y larga)"
                max={16}
              />
              <Show when={shape() !== "fish"}>
                <Part field="wings" icon="wings" label="Alas" hint="Un par de alas" max={5} />
              </Show>
              <Show when={shape() === "fish"}>
                <Flag field="fins" icon="fins" label="Aletas" hint="Aletas pectorales y dorsal" />
                <Flag field="flukes" icon="flukes" label="Aleta caudal" hint="Aleta horizontal al final de la cola (delfín, ballena)" />
              </Show>
            </div>
            <Show when={count("tail") > 0}>
              <CountRow
                label="Colas (el número de la tarjeta son sus segmentos)"
                value={Math.max(plan()?.tails ?? 1, 1)}
                min={1}
                max={9}
                onChange={(tails) => update({ tails })}
                disabled={props.disabled}
              />
            </Show>
          </Section>
        </Show>

        <Show when={shape() === "arthropod"}>
          <Section title="Cuerpo">
            <div class={GRID}>
              <Tile
                icon={ARTHROPOD_ICONS.fusedHead}
                label="Cabeza fusionada"
                hint="La cabeza es el frente del tórax: araña, cangrejo, escorpión"
                active={!plan()?.separate_head}
                disabled={props.disabled}
                onClick={() => update({ separate_head: false })}
              />
              <Tile
                icon={ARTHROPOD_ICONS.separateHead}
                label="Cabeza aparte"
                hint="Cabeza con un cuello corto que gira sola: insectos, hormiga, ciempiés"
                active={!!plan()?.separate_head}
                disabled={props.disabled}
                onClick={() => update({ separate_head: true })}
              />
              <Part field="abdomen" icon="abdomen" label="Abdomen" hint="Abdomen detrás del tórax: más segmentos lo doblan más (abeja que pica, langosta)" max={12} />
              <Part field="tail" icon="stinger" label="Cola con aguijón" hint="Cola de escorpión que se arquea sobre el lomo" max={12} />
              <Part field="wings" icon="insectWings" label="Alas" hint="Alas: más segmentos las doblan más" max={5} />
              <Flag field="segmented" icon="segmented" label="Patas en cada segmento" hint="Un par de patas en cada segmento del cuerpo: ciempiés, milpiés" />
            </div>
            <Show when={count("wings") > 0}>
              <CountRow label="Pares de alas" value={Math.max(plan()?.wing_pairs ?? 1, 1)} min={1} max={2} onChange={(wing_pairs) => update({ wing_pairs })} disabled={props.disabled} />
            </Show>
          </Section>
          <Section title="Cabeza y boca">
            <div class={GRID}>
              <Part field="antennae" icon="antennae" label="Antenas" hint="Un par de antenas (langosta: largas, más segmentos)" max={6} />
              <Flag field="fangs" icon="fangs" label="Colmillos" hint="Quelíceros de la araña o mandíbulas de la hormiga: muerden" />
              <Flag field="palps" icon="palps" label="Pedipalpos" hint="Las patitas cortas al frente de la araña" />
              <Flag field="eye_stalks" icon="eyeStalks" label="Ojos con pedúnculo" hint="Ojos sobre tallos: cangrejo, langosta" />
              <Flag field="pincers" icon="pincers" label="Pinzas" hint="Brazo, palma y un dedo móvil que abre y cierra" />
            </div>
          </Section>
          <Section title="Patas">
            <CountRow
              label="Pares de patas"
              value={count("limbs")}
              min={1}
              max={plan()?.segmented ? 24 : 8}
              onChange={(limbs) => update({ limbs })}
              disabled={props.disabled}
            />
            <CountRow label="Segmentos por pata" value={count("limb_segments")} min={2} max={5} onChange={(limb_segments) => update({ limb_segments })} disabled={props.disabled} />
          </Section>
        </Show>

        <Show when={shape() === "radial"}>
          <Section title="Forma">
            <div class={GRID}>
              <For each={RADIAL_POSES}>
                {(r) => (
                  <Tile
                    icon={RADIAL_ICONS[r.id]}
                    label={r.label}
                    hint={r.hint}
                    active={(plan()?.radial_pose ?? "spread") === r.id}
                    disabled={props.disabled}
                    onClick={() => update({ radial_pose: r.id })}
                  />
                )}
              </For>
            </div>
            <CountRow
              label="Manto o columna (0: sin cabeza)"
              value={plan()?.mantle ?? 1}
              min={0}
              max={6}
              onChange={(mantle) => update({ mantle })}
              disabled={props.disabled}
            />
          </Section>
          <Section title="Brazos">
            <div class={GRID}>
              <Part
                field="tentacles"
                icon="tentacles"
                label="Tentáculos de caza"
                hint="Pares de tentáculos mucho más largos que los brazos, entre los del frente: el calamar tiene 1 par para atrapar presas"
                max={2}
              />
              <Flag field="fins" icon="fins" label="Aletas" hint="Aletas del manto (calamar)" />
            </div>
            <CountRow label="Brazos" value={count("limbs")} min={3} max={16} onChange={(limbs) => update({ limbs })} disabled={props.disabled} />
            <CountRow label="Segmentos por brazo" value={count("limb_segments")} min={2} max={10} onChange={(limb_segments) => update({ limb_segments })} disabled={props.disabled} />
          </Section>
        </Show>

        <Show when={shape() === "serpent"}>
          <Section title="Cuerpo">
            <div class={GRID}>
              <Tile icon={PART_ICONS.body} label="Cuerpo" hint="Una cadena sin extremidades" active onClick={() => {}} />
            </div>
            <CountRow label="Segmentos del cuerpo" value={count("tail")} min={4} max={32} onChange={(tail) => update({ tail })} disabled={props.disabled} />
          </Section>
        </Show>

        <Show when={shape() === "tree"}>
          <Section title="Tallo y ramas">
            <div class={GRID}>
              <Tile icon={PART_ICONS.stem} label="Tallo" hint="El eje que sube desde la base" active onClick={() => {}} />
              <Tile icon={PART_ICONS.branches} label="Ramas" hint="Ramas repartidas a lo largo del tallo" active={count("limbs") > 0} onClick={() => update({ limbs: count("limbs") > 0 ? 0 : 4 })} />
            </div>
            <CountRow label="Segmentos del tallo" value={count("tail")} min={2} max={16} onChange={(tail) => update({ tail })} disabled={props.disabled} />
            <CountRow label="Ramas" value={count("limbs")} min={0} max={12} onChange={(limbs) => update({ limbs })} disabled={props.disabled} />
            <CountRow label="Segmentos por rama" value={count("limb_segments")} min={1} max={8} onChange={(limb_segments) => update({ limb_segments })} disabled={props.disabled} />
          </Section>
        </Show>

        <Show when={headed() || shape() === "arthropod" || (count("tail") > 0 && shape() !== "serpent" && shape() !== "tree")}>
          <Section title="Proporciones">
            <div class="space-y-3">
              <Show when={headed() || shape() === "arthropod"}>
                <Ratio field="leg_length" label="Largo de las patas" max={2} />
              </Show>
              <Show when={shape() === "arthropod"}>
                <Ratio field="body_width" label="Ancho del cuerpo" max={3} />
              </Show>
              <Show when={(shape() === "biped" || shape() === "digitigrade") && count("wings") === 0}>
                <Ratio field="arm_length" label="Largo de los brazos" max={2} />
              </Show>
              <Show when={count("tail") > 0}>
                <Ratio field="tail_length" label="Largo de la cola" max={3} />
              </Show>
            </div>
          </Section>
        </Show>
    </div>
  );
};
