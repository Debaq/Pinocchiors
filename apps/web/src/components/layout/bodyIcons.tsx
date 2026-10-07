import { For, type Component, type JSX } from "solid-js";

/**
 * Pictogramas del constructor de cuerpos: esqueletos de palitos con sus
 * articulaciones. El cuerpo de referencia va tenue y la parte de la que se
 * habla resaltada, para ubicarla de un vistazo. Vista lateral mirando a la
 * derecha salvo donde se indica.
 */

type Point = [number, number];

/** Huesos en cadena con un punto en cada articulación */
const Chain: Component<{ points: Point[]; joints?: boolean }> = (props) => (
  <>
    <polyline points={props.points.map((p) => p.join(",")).join(" ")} fill="none" />
    {props.joints !== false && (
      <For each={props.points}>{([x, y]) => <circle cx={x} cy={y} r="1.6" fill="currentColor" stroke="none" />}</For>
    )}
  </>
);

/** Varias cadenas */
const Chains: Component<{ chains: Point[][]; joints?: boolean }> = (props) => (
  <For each={props.chains}>{(points) => <Chain points={points} joints={props.joints} />}</For>
);

const Frame: Component<{ children: JSX.Element; size?: number }> = (props) => (
  <svg
    viewBox="0 0 48 48"
    width={props.size ?? 40}
    height={props.size ?? 40}
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    {props.children}
  </svg>
);

/** Referencia tenue */
const Faint: Component<{ children: JSX.Element }> = (props) => (
  <g opacity="0.35" stroke-width="1.6">
    {props.children}
  </g>
);

/** Parte resaltada */
const Accent: Component<{ children: JSX.Element }> = (props) => <g class="text-accent">{props.children}</g>;

const Ground = () => <line x1="3" y1="44" x2="45" y2="44" opacity="0.3" stroke-width="1" stroke-dasharray="2 2" />;

// ─── Cuerpos de referencia ──────────────────────────────────────────────────

const QUAD_BODY: Point[][] = [
  [[11, 22], [21, 21], [31, 22]],
  [[31, 22], [31, 32], [31, 42]],
  [[11, 22], [11, 32], [11, 42]],
];
const QUAD_NECK: Point[] = [[31, 22], [36, 14], [43, 15]];
const QUAD_TAIL: Point[] = [[11, 22], [6, 28]];

const QuadBody: Component<{ neck?: boolean; tail?: boolean }> = (props) => (
  <Chains chains={[...QUAD_BODY, ...(props.neck !== false ? [QUAD_NECK] : []), ...(props.tail !== false ? [QUAD_TAIL] : [])]} />
);

const HUMAN: Point[][] = [
  [[24, 26], [24, 18], [24, 12]],
  [[24, 14], [17, 21], [14, 28]],
  [[24, 14], [31, 21], [34, 28]],
  [[24, 26], [20, 35], [20, 44]],
  [[24, 26], [28, 35], [28, 44]],
];

const HumanBody: Component<{ arms?: boolean }> = (props) => (
  <>
    <circle cx="24" cy="8" r="3.2" />
    <Chains chains={props.arms === false ? [HUMAN[0], HUMAN[3], HUMAN[4]] : HUMAN} />
  </>
);

// ─── Formas base ────────────────────────────────────────────────────────────

export const SHAPE_ICONS: Record<string, Component> = {
  biped: () => (
    <Frame>
      <HumanBody />
    </Frame>
  ),
  digitigrade: () => (
    <Frame>
      <Chains
        chains={[
          [[4, 30], [12, 24], [20, 20], [30, 18]],
          [[30, 18], [35, 11], [43, 11]],
          [[30, 18], [34, 23], [37, 22]],
          [[20, 20], [23, 30], [18, 37], [23, 44]],
        ]}
      />
    </Frame>
  ),
  quadruped: () => (
    <Frame>
      <QuadBody />
    </Frame>
  ),
  radial: () => (
    <Frame>
      <ellipse cx="24" cy="13" rx="8" ry="8" />
      <Chains
        joints={false}
        chains={[
          [[18, 20], [12, 30], [5, 33]],
          [[21, 21], [18, 33], [13, 42]],
          [[24, 21], [24, 34], [26, 44]],
          [[27, 21], [31, 33], [36, 42]],
          [[30, 20], [36, 29], [43, 32]],
        ]}
      />
    </Frame>
  ),
  fish: () => (
    <Frame>
      <Chain points={[[6, 24], [16, 24], [28, 24], [40, 24]]} />
      <Chains
        joints={false}
        chains={[
          [[6, 24], [3, 16]],
          [[6, 24], [3, 32]],
          [[24, 24], [22, 14]],
          [[32, 26], [28, 32]],
        ]}
      />
    </Frame>
  ),
  arthropod: () => (
    <Frame>
      {/* Desde arriba */}
      <ellipse cx="24" cy="24" rx="5" ry="9" />
      <Chains
        chains={[
          [[20, 19], [12, 14], [7, 18]],
          [[20, 24], [11, 23], [6, 28]],
          [[20, 29], [12, 33], [8, 39]],
          [[28, 19], [36, 14], [41, 18]],
          [[28, 24], [37, 23], [42, 28]],
          [[28, 29], [36, 33], [40, 39]],
        ]}
      />
    </Frame>
  ),
  serpent: () => (
    <Frame>
      <Chain points={[[40, 8], [32, 10], [26, 16], [30, 24], [24, 31], [14, 30], [9, 37], [14, 43]]} />
    </Frame>
  ),
  tree: () => (
    <Frame>
      <Chains
        chains={[
          [[24, 44], [24, 32], [24, 20], [24, 8]],
          [[24, 32], [15, 25], [10, 18]],
          [[24, 26], [33, 19], [38, 12]],
          [[24, 18], [17, 12]],
        ]}
      />
    </Frame>
  ),
};

// ─── Patas ──────────────────────────────────────────────────────────────────

export const FEET_ICONS: Record<string, Component> = {
  simple: () => (
    <Frame>
      <Ground />
      <Chain points={[[24, 5], [24, 24], [24, 43]]} />
    </Frame>
  ),
  plantigrade: () => (
    <Frame>
      <Ground />
      <Chain points={[[21, 5], [24, 22], [21, 38], [33, 42]]} />
    </Frame>
  ),
  digitigrade: () => (
    <Frame>
      <Ground />
      <Chain points={[[19, 5], [27, 19], [17, 31], [24, 43]]} />
    </Frame>
  ),
  unguligrade: () => (
    <Frame>
      <Ground />
      <Chain points={[[19, 4], [27, 15], [18, 25], [20, 37], [24, 43]]} />
      <path d="M21 43 h6" stroke-width="3" />
    </Frame>
  ),
};

// ─── Postura (de frente) ────────────────────────────────────────────────────

export const POSTURE_ICONS: Record<string, Component> = {
  under: () => (
    <Frame>
      <Ground />
      <ellipse cx="24" cy="16" rx="9" ry="6" />
      <Chain points={[[18, 21], [18, 32], [18, 43]]} />
      <Chain points={[[30, 21], [30, 32], [30, 43]]} />
    </Frame>
  ),
  sprawl: () => (
    <Frame>
      <Ground />
      <ellipse cx="24" cy="31" rx="7" ry="5" />
      <Chain points={[[18, 31], [8, 29], [5, 43]]} />
      <Chain points={[[30, 31], [40, 29], [43, 43]]} />
    </Frame>
  ),
};

// ─── Cuello ─────────────────────────────────────────────────────────────────

const BODY_NO_NECK = () => (
  <Faint>
    <QuadBody neck={false} />
  </Faint>
);

export const NECK_ICONS: Record<string, Component> = {
  rising: () => (
    <Frame>
      <BODY_NO_NECK />
      <Accent>
        <Chain points={[[31, 22], [34, 16], [37, 11], [44, 12]]} />
      </Accent>
    </Frame>
  ),
  swan: () => (
    <Frame>
      <BODY_NO_NECK />
      <Accent>
        <Chain points={[[31, 22], [35, 28], [39, 22], [40, 12], [46, 13]]} />
      </Accent>
    </Frame>
  ),
  level: () => (
    <Frame>
      <BODY_NO_NECK />
      <Accent>
        <Chain points={[[31, 22], [36, 21], [40, 21], [46, 22]]} />
      </Accent>
    </Frame>
  ),
  upright: () => (
    <Frame>
      <BODY_NO_NECK />
      <Accent>
        <Chain points={[[31, 22], [32, 15], [33, 8], [33, 4], [40, 5]]} />
      </Accent>
    </Frame>
  ),
};

// ─── Partes ─────────────────────────────────────────────────────────────────

/** Cuadrúpedo tenue con una parte resaltada */
const OnQuad: Component<{ children: JSX.Element; neck?: boolean; tail?: boolean }> = (props) => (
  <Frame>
    <Faint>
      <QuadBody neck={props.neck} tail={props.tail} />
    </Faint>
    <Accent>{props.children}</Accent>
  </Frame>
);

export const PART_ICONS: Record<string, Component> = {
  ears: () => (
    <OnQuad>
      <Chain points={[[36, 14], [34, 8], [31, 6]]} />
    </OnQuad>
  ),
  horns: () => (
    <OnQuad>
      <Chain points={[[37, 13], [36, 7], [40, 3]]} />
    </OnQuad>
  ),
  tusks: () => (
    <OnQuad>
      <Chain points={[[40, 17], [44, 21], [46, 19]]} />
    </OnQuad>
  ),
  trunk: () => (
    <OnQuad>
      <Chain points={[[43, 15], [46, 22], [45, 29], [42, 35]]} />
    </OnQuad>
  ),
  jaw: () => (
    <OnQuad>
      <Chain points={[[36, 15], [43, 19]]} />
    </OnQuad>
  ),
  neck: () => (
    <OnQuad neck={false}>
      <Chain points={[[31, 22], [33, 18], [36, 14], [43, 15]]} />
    </OnQuad>
  ),
  tail: () => (
    <OnQuad tail={false}>
      <Chain points={[[11, 22], [6, 25], [3, 31], [4, 37]]} />
    </OnQuad>
  ),
  hump1: () => (
    <OnQuad>
      <path d="M14 21 Q21 9 28 21" />
      <Chain points={[[21, 21], [21, 13]]} />
    </OnQuad>
  ),
  hump2: () => (
    <OnQuad>
      <path d="M11 21 Q15 11 20 21 M22 21 Q27 11 31 21" />
      <Chain points={[[15, 21], [15, 14]]} />
      <Chain points={[[27, 21], [27, 14]]} />
    </OnQuad>
  ),
  wings: () => (
    <OnQuad>
      <Chain points={[[26, 21], [21, 11], [12, 5]]} />
      <path d="M12 5 L17 17 M21 11 L22 20" opacity="0.6" stroke-width="1.4" />
    </OnQuad>
  ),
  legLength: () => (
    <OnQuad>
      <Chain points={[[31, 22], [31, 32], [31, 42]]} />
      <Chain points={[[11, 22], [11, 32], [11, 42]]} />
    </OnQuad>
  ),
  armLength: () => (
    <Frame>
      <Faint>
        <HumanBody arms={false} />
      </Faint>
      <Accent>
        <Chain points={[[24, 14], [15, 24], [10, 34]]} />
        <Chain points={[[24, 14], [33, 24], [38, 34]]} />
      </Accent>
    </Frame>
  ),
  bipedTail: () => (
    <Frame>
      <Faint>
        <HumanBody />
      </Faint>
      <Accent>
        <Chain points={[[24, 26], [32, 30], [39, 26], [41, 18], [37, 15]]} />
      </Accent>
    </Frame>
  ),
  stinger: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="30" rx="9" ry="5" />
        <Chains chains={[[[20, 33], [14, 40]], [[28, 33], [34, 40]]]} />
      </Faint>
      <Accent>
        {/* De costado: la cola del escorpión se arquea por arriba */}
        <Chain points={[[16, 28], [9, 20], [11, 11], [19, 7], [24, 11]]} />
      </Accent>
    </Frame>
  ),
  insectWings: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="26" rx="4" ry="12" />
      </Faint>
      <Accent>
        {/* Desde arriba */}
        <Chain points={[[21, 20], [12, 12], [5, 16]]} />
        <Chain points={[[27, 20], [36, 12], [43, 16]]} />
        <path d="M5 16 Q10 24 21 22 M43 16 Q38 24 27 22" opacity="0.6" stroke-width="1.4" />
      </Accent>
    </Frame>
  ),
  fishTail: () => (
    <Frame>
      <Faint>
        <Chain points={[[44, 24], [34, 24]]} />
      </Faint>
      <Accent>
        <Chain points={[[34, 24], [24, 24], [14, 24], [6, 24]]} />
      </Accent>
    </Frame>
  ),
  fins: () => (
    <Frame>
      <Faint>
        <Chain points={[[6, 24], [16, 24], [28, 24], [40, 24]]} />
      </Faint>
      <Accent>
        <Chain points={[[24, 22], [21, 12]]} />
        <Chain points={[[32, 26], [27, 33]]} />
      </Accent>
    </Frame>
  ),
  flukes: () => (
    <Frame>
      <Faint>
        <Chain points={[[12, 24], [24, 24], [36, 24], [44, 24]]} />
      </Faint>
      <Accent>
        {/* Desde arriba: la aleta es horizontal */}
        <Chain points={[[12, 24], [5, 15]]} />
        <Chain points={[[12, 24], [5, 33]]} />
      </Accent>
    </Frame>
  ),
  pincers: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="28" rx="5" ry="9" />
        <Chains chains={[[[20, 28], [11, 27], [6, 32]], [[28, 28], [37, 27], [42, 32]]]} />
      </Faint>
      <Accent>
        <Chain points={[[21, 21], [14, 15], [16, 6]]} />
        <Chain points={[[27, 21], [34, 15], [32, 6]]} />
        <path d="M16 6 l-4 2 M32 6 l4 2" />
      </Accent>
    </Frame>
  ),
  antennae: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="30" rx="5" ry="9" />
      </Faint>
      <Accent>
        <Chain points={[[22, 21], [18, 13], [12, 6]]} />
        <Chain points={[[26, 21], [30, 13], [36, 6]]} />
      </Accent>
    </Frame>
  ),
  limbs: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="24" rx="5" ry="9" />
      </Faint>
      <Accent>
        <Chains
          chains={[
            [[20, 19], [12, 14], [7, 18]],
            [[20, 24], [11, 23], [6, 28]],
            [[28, 19], [36, 14], [41, 18]],
            [[28, 24], [37, 23], [42, 28]],
          ]}
        />
      </Accent>
    </Frame>
  ),
  arms: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="13" rx="8" ry="8" />
      </Faint>
      <Accent>
        <Chains
          chains={[
            [[18, 20], [12, 30], [5, 33]],
            [[24, 21], [24, 34], [26, 44]],
            [[30, 20], [36, 29], [43, 32]],
          ]}
        />
      </Accent>
    </Frame>
  ),
  tentacles: () => (
    <Frame>
      <Faint>
        <ellipse cx="24" cy="10" rx="7" ry="7" />
        <Chains joints={false} chains={[[[20, 16], [16, 28]], [[28, 16], [32, 28]]]} />
      </Faint>
      <Accent>
        <Chain points={[[22, 17], [20, 30], [16, 40], [10, 45]]} />
        <Chain points={[[26, 17], [28, 30], [32, 40], [38, 45]]} />
      </Accent>
    </Frame>
  ),
  stem: () => (
    <Frame>
      <Faint>
        <Chains chains={[[[24, 32], [15, 25], [10, 18]], [[24, 22], [33, 15], [38, 8]]]} />
      </Faint>
      <Accent>
        <Chain points={[[24, 44], [24, 32], [24, 22], [24, 8]]} />
      </Accent>
    </Frame>
  ),
  branches: () => (
    <Frame>
      <Faint>
        <Chain points={[[24, 44], [24, 32], [24, 22], [24, 8]]} />
      </Faint>
      <Accent>
        <Chains chains={[[[24, 32], [15, 25], [10, 18]], [[24, 22], [33, 15], [38, 8]]]} />
      </Accent>
    </Frame>
  ),
  body: () => (
    <Frame>
      <Accent>
        <Chain points={[[40, 8], [32, 10], [26, 16], [30, 24], [24, 31], [14, 30], [9, 37], [14, 43]]} />
      </Accent>
    </Frame>
  ),
};

// ─── Extremidades (para quitarlas) ──────────────────────────────────────────

/** Bit de cada extremidad en `BodyPlan.missing` */
export const LIMB_BITS = [1, 2, 4, 8] as const;

// Bípedo de frente: el lado izquierdo del personaje queda a la derecha del dibujo
const BIPED_LIMBS: Record<number, Point[]> = {
  1: [[24, 14], [31, 21], [34, 28]],
  2: [[24, 14], [17, 21], [14, 28]],
  4: [[24, 26], [28, 35], [28, 44]],
  8: [[24, 26], [20, 35], [20, 44]],
};
// Cuadrúpedo desde arriba, la cabeza hacia arriba: su izquierda queda a la izquierda
const QUAD_LIMBS: Record<number, Point[]> = {
  1: [[19, 17], [11, 15], [8, 10]],
  2: [[29, 17], [37, 15], [40, 10]],
  4: [[19, 31], [11, 33], [8, 38]],
  8: [[29, 31], [37, 33], [40, 38]],
};

const limbIcon = (limbs: Record<number, Point[]>, bit: number, body: () => JSX.Element): Component => () => (
  <Frame>
    <Faint>
      {body()}
      <Chains chains={LIMB_BITS.filter((b) => b !== bit).map((b) => limbs[b])} />
    </Faint>
    <Accent>
      <Chain points={limbs[bit]} />
    </Accent>
  </Frame>
);

const bipedTrunk = () => (
  <>
    <circle cx="24" cy="8" r="3.2" />
    <Chain points={[[24, 26], [24, 18], [24, 12]]} />
  </>
);
const quadTrunk = () => (
  <>
    <ellipse cx="24" cy="24" rx="5.5" ry="11" />
    <ellipse cx="24" cy="8" rx="3" ry="3.5" />
    <path d="M24 35 Q23 41 26 46" />
  </>
);

export const LIMB_ICONS: Record<"biped" | "quadruped", Record<number, Component>> = {
  biped: Object.fromEntries(LIMB_BITS.map((b) => [b, limbIcon(BIPED_LIMBS, b, bipedTrunk)])),
  quadruped: Object.fromEntries(LIMB_BITS.map((b) => [b, limbIcon(QUAD_LIMBS, b, quadTrunk)])),
};
