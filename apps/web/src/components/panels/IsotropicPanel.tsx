import { Component, Show } from "solid-js";
import { Button, Checkbox, NumberInput, Slider } from "../ui";
import { RemeshActions, type RemeshActionsProps } from "./RemeshActions";

/** La malla del modelo, para proponer parámetros (del backend, en mm) */
export interface RemeshInfo {
  extent_mm: number;
  mean_edge_mm: number;
  area_mm2: number;
  triangles: number;
  /** Isótropo la necesita manifold */
  manifold: boolean;
  /** Textura, colores de vértice o varios materiales */
  has_skin: boolean;
  /** Rig con pesos */
  has_rig: boolean;
}

/** Opciones de Isótropo (Remallar) */
export interface IsotropicConfig {
  /** Lado de los triángulos en mm (`null`: automático, ver `autoEdge`) */
  edgeMm: number | null;
  keepSharp: boolean;
  /** Ángulo entre caras desde el que una arista es viva, en grados */
  sharpAngle: number;
  iterations: number;
  /** Triángulos más chicos en las partes delgadas */
  thinFeatures: boolean;
}

export const DEFAULT_ISOTROPIC: IsotropicConfig = {
  edgeMm: null,
  keepSharp: true,
  sharpAngle: 45,
  iterations: 5,
  thinFeatures: false,
};

/** Triángulos de más: el backend no los hace */
export const MAX_ISOTROPIC_TRIANGLES = 6_000_000;

/** Lado automático: el largo medio de las aristas, sin pasar de 1/50 del
 * tamaño (una pieza CAD tiene pocos triángulos enormes) */
export const autoEdge = (info?: RemeshInfo) => (info ? Math.min(info.mean_edge_mm, info.extent_mm / 50) : 1);

/** Lado que se usa: el elegido o el automático */
export const isotropicEdge = (c: IsotropicConfig, info?: RemeshInfo) => c.edgeMm ?? autoEdge(info);

/** Triángulos equiláteros de lado `edge` que caben en el área */
export const estimateTriangles = (areaMm2: number, edgeMm: number) => Math.round(areaMm2 / ((Math.sqrt(3) / 4) * edgeMm * edgeMm));

export const isotropicParams = (c: IsotropicConfig, info?: RemeshInfo) => ({
  mode: "isotropic" as const,
  edge_mm: isotropicEdge(c, info),
  sharp_angle: c.keepSharp ? c.sharpAngle : null,
  iterations: Math.round(c.iterations),
  thin_features: c.thinFeatures,
});

/** Milímetros con pocas cifras */
export const formatMm = (v: number) => `${v.toLocaleString("es", { maximumSignificantDigits: 3 })} mm`;

export interface IsotropicPanelProps extends RemeshActionsProps {
  config: IsotropicConfig;
  onChange: (config: IsotropicConfig) => void;
  info?: RemeshInfo;
  /** Pasar a Vóxeles (cuando la malla no es manifold) */
  onUseVoxels: () => void;
}

export const IsotropicPanel: Component<IsotropicPanelProps> = (props) => {
  const update = (change: Partial<IsotropicConfig>) => props.onChange({ ...props.config, ...change });
  const edge = () => isotropicEdge(props.config, props.info);
  const estimate = () => (props.info ? estimateTriangles(props.info.area_mm2, edge()) : 0);
  const tooDense = () => estimate() > MAX_ISOTROPIC_TRIANGLES;
  const broken = () => props.info && !props.info.manifold;

  return (
    <div data-isotropic class="space-y-4">
      <Show when={broken()}>
        <div data-isotropic-broken class="rounded-md border border-warning/40 bg-warning/10 p-2 text-xs text-text-muted space-y-2">
          <p>
            La malla tiene aristas con más de dos caras o caras dadas vuelta (piezas unidas sin fundir, por ejemplo). Isótropo
            necesita una superficie limpia: repararla antes o rehacerla con Vóxeles, que también puede dejar los triángulos
            parejos.
          </p>
          <Button size="sm" variant="default" onClick={() => props.onUseVoxels()}>
            Usar Vóxeles
          </Button>
        </div>
      </Show>

      <div class="space-y-1">
        <div class="flex items-center justify-between gap-2">
          <span class="text-xs text-text-muted">Lado de los triángulos</span>
          <div class="flex items-center gap-1">
            <NumberInput
              class="w-24"
              value={Number(edge().toPrecision(3))}
              onChange={(v) => v > 0 && update({ edgeMm: v })}
              min={0}
              step={Number((edge() / 10).toPrecision(1))}
              suffix="mm"
            />
            <Button size="sm" variant={props.config.edgeMm == null ? "default" : "ghost"} onClick={() => update({ edgeMm: null })} title="El largo medio de las aristas de ahora, como mucho 1/50 del tamaño">
              Auto
            </Button>
          </div>
        </div>
        <Show when={props.info}>
          {(info) => (
            <p class="text-xs text-text-muted">
              ≈ <span data-isotropic-estimate class={tooDense() ? "text-error" : "text-text"}>{estimate().toLocaleString("es")}</span> triángulos
              (ahora {info().triangles.toLocaleString("es")}; arista media {formatMm(info().mean_edge_mm)})
              <Show when={tooDense()}>
                <span class="text-error"> — demasiados, usar un lado mayor</span>
              </Show>
            </p>
          )}
        </Show>
      </div>

      <div class="space-y-2">
        <Checkbox label="Conservar las aristas vivas" checked={props.config.keepSharp} onChange={(keepSharp) => update({ keepSharp })} />
        <Show when={props.config.keepSharp}>
          <Slider
            label="Desde"
            value={props.config.sharpAngle}
            onChange={(sharpAngle) => update({ sharpAngle })}
            min={10}
            max={120}
            step={5}
            formatValue={(v) => `${v}° entre caras`}
          />
        </Show>
      </div>
      <Slider
        label="Pasadas"
        value={props.config.iterations}
        onChange={(iterations) => update({ iterations })}
        min={1}
        max={15}
        step={1}
        title="Partir, colapsar, voltear y relajar; más pasadas, triángulos más parejos"
      />
      <div class="space-y-1">
        <Checkbox
          label="Más fino en las partes delgadas"
          checked={props.config.thinFeatures}
          onChange={(thinFeatures) => update({ thinFeatures })}
        />
        <p class="text-xs text-text-muted leading-relaxed">
          Paredes, puntas y ranuras más angostas que un triángulo reciben triángulos más chicos, con la misma cantidad total.
        </p>
      </div>

      <p class="text-xs text-text-dim leading-relaxed">
        Es una malla nueva, sin UV: al aplicar se elige si trasladar la textura (se hornea sobre un mapa nuevo) y los pesos.
        En mallas grandes tarda: unos 20 s cada medio millón de triángulos.
      </p>

      <RemeshActions {...props} canExecute={props.canExecute && !broken() && !tooDense()} />
    </div>
  );
};
