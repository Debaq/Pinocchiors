/**
 * Dibujo del gcode laminado en el visor: cada extrusión como una cinta del
 * ancho de la línea (LineSegments2 en unidades del mundo), de color por tipo,
 * con los viajes aparte y la cama debajo. Coordenadas: la impresora usa Z
 * arriba en mm; la escena Y arriba, así que (x, y, z) → (x, z, −y).
 */
import * as THREE from "three";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import { thickLineMaterial, thickLineSegments, type ThickLineMaterial } from "./gpuRenderer";
import { FEATURES, type GcodePreview, type GcodeViewOptions } from "./gcodePreview";
import { buildPrintBed } from "./printBed";

export class GcodeView {
  readonly group = new THREE.Group();
  private data: GcodePreview;
  private hiddenKey = "";
  private lines: THREE.Mesh | null = null;
  /** LineMaterial, o su versión de nodos con la aceleración GPU (lib/gpuRenderer.ts) */
  private material: ThickLineMaterial;
  /** Primer segmento (ya filtrado) de cada capa, y el total al final */
  private layerStart: number[] = [];
  private travelLines: THREE.LineSegments;
  private bed: THREE.Group;

  constructor(data: GcodePreview, resolution: THREE.Vector2) {
    this.data = data;
    const widths = Array.from(data.widths).sort((a, b) => a - b);
    const width = widths.length ? widths[Math.floor(widths.length / 2)] : 0.42;
    this.material = thickLineMaterial({ vertexColors: true, worldUnits: true, linewidth: width, resolution }, LineMaterial);
    this.travelLines = this.buildTravels();
    this.bed = this.buildBed();
    this.group.add(this.bed, this.travelLines);
  }

  /** Esfera que encuadra la cama y lo impreso */
  bounds(): THREE.Sphere {
    const [bx, by] = this.data.bed;
    const top = Math.max(this.data.meta.max[2], 10);
    const box = new THREE.Box3(new THREE.Vector3(0, 0, -by), new THREE.Vector3(bx, top, 0));
    return box.getBoundingSphere(new THREE.Sphere());
  }

  setResolution(w: number, h: number): void {
    // El material de nodos (aceleración GPU) toma el tamaño solo
    this.material.resolution?.set(w, h);
  }

  update(options: GcodeViewOptions): void {
    const key = [...options.hidden].sort((a, b) => a - b).join(",");
    if (!this.lines || key !== this.hiddenKey) this.buildLines(new Set(options.hidden), key);
    const layers = this.data.meta.layer_z.length;
    const upto = Math.min(Math.max(options.layer, 0), Math.max(layers - 1, 0));
    // Las capas hasta `upto` incluida
    const geometry = this.lines!.geometry as LineSegmentsGeometry;
    geometry.instanceCount = this.layerStart[upto + 1] ?? this.layerStart[this.layerStart.length - 1] ?? 0;
    this.travelLines.visible = options.travels;
    const travelEnd = this.data.meta.travel_start[upto + 1] ?? this.data.meta.travel_segments;
    this.travelLines.geometry.setDrawRange(0, travelEnd * 2);
  }

  dispose(): void {
    this.group.traverse((o) => {
      if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments || o instanceof THREE.LineLoop || o instanceof LineSegments2) {
        o.geometry.dispose();
        const m = o.material as THREE.Material | THREE.Material[];
        (Array.isArray(m) ? m : [m]).forEach((x) => x.dispose());
      }
    });
  }

  private buildLines(hidden: Set<number>, key: string): void {
    if (this.lines) {
      this.group.remove(this.lines);
      this.lines.geometry.dispose();
    }
    const { positions, features, meta } = this.data;
    const n = meta.segments;
    let kept = 0;
    for (let i = 0; i < n; i++) if (!hidden.has(features[i])) kept++;
    const pos = new Float32Array(kept * 6);
    const col = new Float32Array(kept * 6);
    const palette = FEATURES.map((f) => new THREE.Color(f.color));
    this.layerStart = [];
    let layer = 0;
    let j = 0;
    for (let i = 0; i < n; i++) {
      while (layer < meta.layer_start.length && meta.layer_start[layer] <= i) {
        this.layerStart.push(j);
        layer++;
      }
      const f = features[i];
      if (hidden.has(f)) continue;
      const s = i * 6;
      const [x0, y0, z0, x1, y1, z1] = [positions[s], positions[s + 1], positions[s + 2], positions[s + 3], positions[s + 4], positions[s + 5]];
      pos.set([x0, z0, -y0, x1, z1, -y1], j * 6);
      // Un poco de sombra según la dirección: las capas se distinguen sin luces
      const dx = x1 - x0;
      const dy = y1 - y0;
      const len = Math.hypot(dx, dy) || 1;
      const shade = 0.62 + 0.38 * Math.abs((dx * 0.35 + dy * 0.94) / len);
      const c = palette[f] ?? palette[palette.length - 1];
      col.set([c.r * shade, c.g * shade, c.b * shade, c.r * shade, c.g * shade, c.b * shade], j * 6);
      j++;
    }
    while (this.layerStart.length < meta.layer_start.length) this.layerStart.push(j);
    this.layerStart.push(j);
    const geometry = new LineSegmentsGeometry();
    geometry.setPositions(pos);
    geometry.setColors(col);
    this.lines = thickLineSegments(geometry, this.material, LineSegments2);
    this.lines.frustumCulled = false;
    this.group.add(this.lines);
    this.hiddenKey = key;
  }

  private buildTravels(): THREE.LineSegments {
    const t = this.data.travels;
    const pos = new Float32Array(t.length);
    for (let i = 0; i < t.length; i += 3) {
      pos[i] = t[i];
      pos[i + 1] = t[i + 2];
      pos[i + 2] = -t[i + 1];
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    const lines = new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color: 0x4da6ff, transparent: true, opacity: 0.35 }));
    lines.frustumCulled = false;
    lines.visible = false;
    return lines;
  }

  private buildBed(): THREE.Group {
    const [bx, by] = this.data.bed;
    return buildPrintBed(bx, by);
  }
}
