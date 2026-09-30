import { Component, For, Show } from "solid-js";
import * as THREE from "three";
import { NumberInput } from "../ui";
import { PlacementPanel, type PlacementPanelProps } from "./PlacementPanel";

/** Nodo del archivo elegido en el Outliner: su transformación local */
export interface ObjectTabNode {
  name: string;
  hint: string;
  translation: [number, number, number];
  rotation: [number, number, number, number];
  scale: [number, number, number];
  hidden: boolean;
}

export interface NodeTransform {
  translation: [number, number, number];
  rotation: [number, number, number, number];
  scale: [number, number, number];
}

export interface ObjectTabProps {
  meshInfo?: { vertices: number; faces: number; format: string };
  node?: ObjectTabNode;
  onToggleNode?: () => void;
  onDeselectNode?: () => void;
  /** Cambio de la transformación local del nodo elegido */
  onNodeTransform?: (transform: NodeTransform) => void;
  /** Transformar el modelo: piso, frente, girar, espejar, escalar, mover, origen */
  placement?: PlacementPanelProps;
}

/** Pestaña Objeto: el modelo y sus transformaciones, disponible en cualquier espacio */
export const ObjectTab: Component<ObjectTabProps> = (props) => (
  <Show
    when={props.meshInfo}
    fallback={<p class="text-xs text-text-muted">Importa un modelo (Archivo → Importar modelo) para transformarlo.</p>}
  >
    {(info) => (
      <div class="space-y-4">
        <div class="flex justify-between gap-2 text-xs px-3 py-2 rounded-md bg-surface/30 border border-border">
          <span class="text-text font-mono uppercase">{info().format}</span>
          <span class="text-text-muted">
            <span class="text-text font-mono">{info().vertices.toLocaleString()}</span> vértices ·{" "}
            <span class="text-text font-mono">{info().faces.toLocaleString()}</span> caras
          </span>
        </div>
        <Show when={props.node}>
          {(node) => {
            const euler = () => {
              const [x, y, z, w] = node().rotation;
              const e = new THREE.Euler().setFromQuaternion(new THREE.Quaternion(x, y, z, w), "XYZ");
              return [e.x, e.y, e.z].map((r) => THREE.MathUtils.radToDeg(r));
            };
            const current = (): NodeTransform => ({ translation: node().translation, rotation: node().rotation, scale: node().scale });
            const setRotation = (degrees: number[]) => {
              const r = degrees.map((d) => THREE.MathUtils.degToRad(d));
              const q = new THREE.Quaternion().setFromEuler(new THREE.Euler(r[0], r[1], r[2], "XYZ"));
              props.onNodeTransform?.({ ...current(), rotation: [q.x, q.y, q.z, q.w] });
            };
            const row = (label: string, values: number[], step: number, suffix: string, onChange: (next: number[]) => void) => (
              <div class="space-y-1">
                <span class="text-text-muted">{label}</span>
                <div class="grid grid-cols-3 gap-1">
                  <For each={values}>
                    {(v, i) => (
                      <NumberInput
                        value={Math.round(v * 1000) / 1000}
                        step={step}
                        suffix={suffix}
                        onChange={(x) => onChange(values.map((old, k) => (k === i() ? x : old)))}
                      />
                    )}
                  </For>
                </div>
              </div>
            );
            return (
              <div class="space-y-2 text-xs px-3 py-2 rounded-md bg-surface/30 border border-border">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-text font-medium truncate">{node().name}</span>
                  <span class="text-text-dim truncate">{node().hint}</span>
                </div>
                {row("Posición", node().translation, 0.01, "", (t) => props.onNodeTransform?.({ ...current(), translation: t as NodeTransform["translation"] }))}
                {row("Giro (XYZ)", euler(), 1, "°", setRotation)}
                {row("Escala", node().scale, 0.01, "", (s) => props.onNodeTransform?.({ ...current(), scale: s as NodeTransform["scale"] }))}
                <p class="text-[11px] text-text-dim leading-relaxed">
                  Transformación local del archivo (respecto de su padre). Cambiarla rehace la malla: el rig y la
                  retopología se descartan. Para mover el modelo entero usa las herramientas de abajo.
                </p>
                <div class="flex gap-3">
                  <button class="text-accent hover:underline" onClick={() => props.onToggleNode?.()}>
                    {node().hidden ? "Mostrar" : "Ocultar"}
                  </button>
                  <button class="text-text-muted hover:text-text" onClick={() => props.onDeselectNode?.()}>
                    Quitar selección
                  </button>
                </div>
              </div>
            );
          }}
        </Show>
        <Show when={props.placement}>{(placement) => <PlacementPanel {...placement()} />}</Show>
      </div>
    )}
  </Show>
);
