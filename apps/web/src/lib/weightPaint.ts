// Matemática del pincel de pesos sobre la tabla de influencias del visor:
// por vértice, `k` pares (hueso, peso) que suman 1.

export type PaintMode = "add" | "subtract" | "smooth";

/**
 * Cambia el peso de `bone` en la fila que empieza en `base` y reparte el
 * resto entre las otras influencias, conservando la suma 1. Si el hueso no
 * está entre las influencias, reemplaza a la más débil (si el peso nuevo la
 * supera). Devuelve `true` si la fila cambió.
 *
 * - `add`: acerca el peso a 1 en `amount` (fracción de lo que falta).
 * - `subtract`: lo acerca a 0 en `amount` (fracción de lo que tiene).
 * - `smooth`: lo acerca a `smoothTarget` (el promedio de los vecinos).
 *
 * Si `bone` es la única influencia, lo que pierde va a `fallback` (p. ej. el
 * hueso vecino en la cadena); sin respaldo la fila no cambia.
 */
export function paintRow(
  row: Float32Array,
  base: number,
  k: number,
  bone: number,
  mode: PaintMode,
  amount: number,
  smoothTarget?: number,
  fallback?: number
): boolean {
  let slot = -1;
  let weakest = 0;
  for (let i = 0; i < k; i++) {
    if (row[base + i * 2] === bone && (row[base + i * 2 + 1] > 0 || slot < 0)) slot = i;
    if (row[base + i * 2 + 1] < row[base + weakest * 2 + 1]) weakest = i;
  }
  const current = slot >= 0 ? row[base + slot * 2 + 1] : 0;
  let target = current;
  if (mode === "add") target = current + amount * (1 - current);
  else if (mode === "subtract") target = current - amount * current;
  else if (smoothTarget !== undefined) target = current + amount * (smoothTarget - current);
  target = Math.min(1, Math.max(0, target));
  if (Math.abs(target - current) < 1e-6) return false;

  if (slot < 0) {
    if (target <= row[base + weakest * 2 + 1]) return false;
    slot = weakest;
    row[base + slot * 2] = bone;
    row[base + slot * 2 + 1] = 0;
  }
  let others = 0;
  for (let i = 0; i < k; i++) if (i !== slot) others += row[base + i * 2 + 1];
  if (others <= 1e-9 && target < 1) {
    if (fallback === undefined || fallback === bone || k < 2) return false;
    // Única influencia: lo que pierde va al respaldo, en la ranura libre
    const free = slot === 0 ? 1 : 0;
    row[base + free * 2] = fallback;
    row[base + free * 2 + 1] = 1 - target;
    row[base + slot * 2 + 1] = target;
    return true;
  }
  const scale = others > 1e-9 ? (1 - target) / others : 0;
  for (let i = 0; i < k; i++) {
    if (i === slot) row[base + i * 2 + 1] = target;
    else row[base + i * 2 + 1] *= scale;
  }
  return true;
}

/** Peso de `bone` en la fila que empieza en `base` */
export function boneWeight(row: Float32Array, base: number, k: number, bone: number): number {
  for (let i = 0; i < k; i++) {
    if (row[base + i * 2] === bone) return row[base + i * 2 + 1];
  }
  return 0;
}

/**
 * Vecinos por aristas de cada vértice (formato CSR: los de `v` son
 * `list[offsets[v]] … list[offsets[v + 1] - 1]`). Los vértices en la misma
 * posición (costuras UV) comparten la unión de sus vecinos. Solo arreglos
 * planos: con medio millón de vértices, un `Set` por vértice y claves de texto
 * tardaban segundos y gastaban cientos de megas.
 */
export function meshNeighbors(positions: Float32Array, indices: Uint32Array): { offsets: Uint32Array; list: Uint32Array } {
  const n = positions.length / 3;
  const rep = weldByPosition(positions);

  // Aristas (desde el representante, hacia el vértice tal cual), por fila
  const count = new Uint32Array(n + 1);
  const numTris = Math.floor(indices.length / 3);
  for (let i = 0; i < numTris * 3; i++) count[rep[indices[i]] + 1] += 2;
  for (let r = 0; r < n; r++) count[r + 1] += count[r];
  const raw = new Uint32Array(count[n]);
  const fill = count.slice(0, n);
  for (let t = 0; t < numTris * 3; t += 3) {
    const a = indices[t];
    const b = indices[t + 1];
    const c = indices[t + 2];
    let o = fill[rep[a]];
    raw[o] = b;
    raw[o + 1] = c;
    fill[rep[a]] = o + 2;
    o = fill[rep[b]];
    raw[o] = a;
    raw[o + 1] = c;
    fill[rep[b]] = o + 2;
    o = fill[rep[c]];
    raw[o] = a;
    raw[o + 1] = b;
    fill[rep[c]] = o + 2;
  }

  // Sin repetidos: cada fila ordenada y compactada en su lugar
  const rowLength = new Uint32Array(n);
  for (let r = 0; r < n; r++) {
    const start = count[r];
    const end = count[r + 1];
    if (end - start < 2) {
      rowLength[r] = end - start;
      continue;
    }
    raw.subarray(start, end).sort();
    let w = start + 1;
    for (let i = start + 1; i < end; i++) if (raw[i] !== raw[w - 1]) raw[w++] = raw[i];
    rowLength[r] = w - start;
  }

  // Cada vértice copia la fila de su representante
  const offsets = new Uint32Array(n + 1);
  for (let v = 0; v < n; v++) offsets[v + 1] = offsets[v] + rowLength[rep[v]];
  const list = new Uint32Array(offsets[n]);
  for (let v = 0; v < n; v++) {
    const r = rep[v];
    list.set(raw.subarray(count[r], count[r] + rowLength[r]), offsets[v]);
  }
  return { offsets, list };
}

/** Por vértice, el primero que está exactamente en su misma posición */
function weldByPosition(positions: Float32Array): Uint32Array {
  const n = positions.length / 3;
  const bits = new Uint32Array(positions.buffer, positions.byteOffset, positions.length);
  const key = (i: number) => {
    const b = bits[i];
    return b === 0x80000000 ? 0 : b; // -0 y 0 son la misma posición
  };
  let size = 1;
  while (size < n * 2) size <<= 1;
  const mask = size - 1;
  const table = new Int32Array(size).fill(-1);
  const rep = new Uint32Array(n);
  for (let v = 0; v < n; v++) {
    const x = key(3 * v);
    const y = key(3 * v + 1);
    const z = key(3 * v + 2);
    let h = (Math.imul(x, 0x9e3779b1) ^ Math.imul(y, 0x85ebca77) ^ Math.imul(z, 0xc2b2ae3d)) >>> 0;
    h = (h ^ (h >>> 15)) & mask;
    for (;;) {
      const u = table[h];
      if (u < 0) {
        table[h] = v;
        rep[v] = v;
        break;
      }
      if (key(3 * u) === x && key(3 * u + 1) === y && key(3 * u + 2) === z) {
        rep[v] = u;
        break;
      }
      h = (h + 1) & mask;
    }
  }
  return rep;
}

/**
 * Rejilla de vértices para el pincel: cada pincelada mira solo las celdas que
 * toca su esfera, no la malla entera. Se arma una vez por malla.
 */
export class VertexGrid {
  private readonly min: [number, number, number];
  private readonly cell: number;
  private readonly dims: [number, number, number];
  /** Por celda, dónde empiezan sus vértices en `items` (CSR) */
  private readonly start: Uint32Array;
  private readonly items: Uint32Array;
  private readonly positions: Float32Array;

  constructor(positions: Float32Array) {
    this.positions = positions;
    const n = positions.length / 3;
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (let v = 0; v < n; v++) {
      for (let k = 0; k < 3; k++) {
        const x = positions[3 * v + k];
        if (x < lo[k]) lo[k] = x;
        if (x > hi[k]) hi[k] = x;
      }
    }
    if (n === 0) {
      lo.fill(0);
      hi.fill(0);
    }
    const diag = Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    // Unas 40 celdas por lado con 500 k vértices (pocas decenas por celda sobre la superficie)
    this.cell = Math.max(diag / Math.max(2, 0.5 * Math.cbrt(n)), 1e-9);
    this.min = [lo[0], lo[1], lo[2]];
    this.dims = [0, 1, 2].map((k) => Math.floor((hi[k] - lo[k]) / this.cell) + 1) as [number, number, number];
    const cells = this.dims[0] * this.dims[1] * this.dims[2];
    const cellOf = new Uint32Array(n);
    this.start = new Uint32Array(cells + 1);
    for (let v = 0; v < n; v++) {
      const c = this.cellIndex(this.coord(positions[3 * v], 0), this.coord(positions[3 * v + 1], 1), this.coord(positions[3 * v + 2], 2));
      cellOf[v] = c;
      this.start[c + 1]++;
    }
    for (let c = 0; c < cells; c++) this.start[c + 1] += this.start[c];
    const fill = this.start.slice(0, cells);
    this.items = new Uint32Array(n);
    for (let v = 0; v < n; v++) this.items[fill[cellOf[v]]++] = v;
  }

  private coord(x: number, k: number): number {
    const i = Math.floor((x - this.min[k]) / this.cell);
    return i < 0 ? 0 : i >= this.dims[k] ? this.dims[k] - 1 : i;
  }

  private cellIndex(ix: number, iy: number, iz: number): number {
    return ix + this.dims[0] * (iy + this.dims[1] * iz);
  }

  /**
   * El vértice más cercano al punto (a menos de `maxRadius`), o -1. Recorre
   * anillos de celdas alrededor del punto hasta que ninguno más lejano pueda
   * tener uno más cerca
   */
  nearest(cx: number, cy: number, cz: number, maxRadius = Infinity): number {
    const p = this.positions;
    const [ix, iy, iz] = [this.coord(cx, 0), this.coord(cy, 1), this.coord(cz, 2)];
    const maxRing = Math.max(...this.dims);
    let best = -1;
    let bestD = maxRadius * maxRadius;
    for (let ring = 0; ring <= maxRing; ring++) {
      // Lo más cerca que puede estar un vértice de este anillo
      const reach = Math.max(0, ring - 1) * this.cell;
      if (reach * reach > bestD) break;
      for (let z = iz - ring; z <= iz + ring; z++) {
        if (z < 0 || z >= this.dims[2]) continue;
        for (let y = iy - ring; y <= iy + ring; y++) {
          if (y < 0 || y >= this.dims[1]) continue;
          const shellYZ = Math.abs(z - iz) === ring || Math.abs(y - iy) === ring;
          for (let x = ix - ring; x <= ix + ring; x += shellYZ ? 1 : 2 * ring || 1) {
            if (x < 0 || x >= this.dims[0]) continue;
            const c = this.cellIndex(x, y, z);
            for (let j = this.start[c]; j < this.start[c + 1]; j++) {
              const v = this.items[j];
              const dx = p[3 * v] - cx;
              const dy = p[3 * v + 1] - cy;
              const dz = p[3 * v + 2] - cz;
              const d = dx * dx + dy * dy + dz * dz;
              if (d < bestD || (d === bestD && v < best)) {
                bestD = d;
                best = v;
              }
            }
          }
        }
      }
    }
    return best;
  }

  /** Vértices a menos de `radius` del centro, de menor a mayor, con su distancia al cuadrado */
  within(cx: number, cy: number, cz: number, radius: number): { vertices: Uint32Array; d2: Float32Array } {
    const r2 = radius * radius;
    const found: number[] = [];
    const p = this.positions;
    const [x0, x1] = [this.coord(cx - radius, 0), this.coord(cx + radius, 0)];
    const [y0, y1] = [this.coord(cy - radius, 1), this.coord(cy + radius, 1)];
    const [z0, z1] = [this.coord(cz - radius, 2), this.coord(cz + radius, 2)];
    for (let iz = z0; iz <= z1; iz++) {
      for (let iy = y0; iy <= y1; iy++) {
        for (let ix = x0; ix <= x1; ix++) {
          const c = this.cellIndex(ix, iy, iz);
          for (let j = this.start[c]; j < this.start[c + 1]; j++) {
            const v = this.items[j];
            const dx = p[3 * v] - cx;
            const dy = p[3 * v + 1] - cy;
            const dz = p[3 * v + 2] - cz;
            if (dx * dx + dy * dy + dz * dz < r2) found.push(v);
          }
        }
      }
    }
    // En orden, como recorriendo la malla entera (el espejo pinta en ese orden)
    const vertices = Uint32Array.from(found).sort();
    const d2 = new Float32Array(vertices.length);
    vertices.forEach((v, i) => {
      const dx = p[3 * v] - cx;
      const dy = p[3 * v + 1] - cy;
      const dz = p[3 * v + 2] - cz;
      d2[i] = dx * dx + dy * dy + dz * dz;
    });
    return { vertices, d2 };
  }
}
