// Pincel de selección de la nube de puntos: rejilla de celdas en pantalla con
// la proyección del trazo, para que cada tramo mire solo los puntos que caen
// cerca y no la nube entera (millones de puntos) (PLAN_RENDIMIENTO R9).

export class ScreenGrid {
  private readonly cell: number;
  private readonly cols: number;
  private readonly rows: number;
  /** Por celda, dónde empiezan sus puntos en `items` (CSR) */
  private readonly start: Uint32Array;
  private readonly items: Uint32Array;

  /** `screen`: x, y en px del lienzo por punto (NaN = no se ve: no entra) */
  constructor(screen: Float32Array, width: number, height: number, cell = 32) {
    this.cell = cell;
    this.cols = Math.max(1, Math.ceil(width / cell));
    this.rows = Math.max(1, Math.ceil(height / cell));
    const n = screen.length / 2;
    const cellOf = new Int32Array(n);
    this.start = new Uint32Array(this.cols * this.rows + 1);
    let kept = 0;
    for (let i = 0; i < n; i++) {
      const x = screen[i * 2];
      const y = screen[i * 2 + 1];
      if (!(x === x && y === y)) {
        cellOf[i] = -1;
        continue;
      }
      // Fuera del lienzo: a la celda del borde (la prueba exacta la hace quien pregunta)
      const c = this.col(x) + this.cols * this.row(y);
      cellOf[i] = c;
      this.start[c + 1]++;
      kept++;
    }
    for (let c = 0; c < this.cols * this.rows; c++) this.start[c + 1] += this.start[c];
    const fill = this.start.slice(0, this.cols * this.rows);
    this.items = new Uint32Array(kept);
    for (let i = 0; i < n; i++) if (cellOf[i] >= 0) this.items[fill[cellOf[i]]++] = i;
  }

  private col(x: number): number {
    const c = Math.floor(x / this.cell);
    return c < 0 ? 0 : c >= this.cols ? this.cols - 1 : c;
  }

  private row(y: number): number {
    const r = Math.floor(y / this.cell);
    return r < 0 ? 0 : r >= this.rows ? this.rows - 1 : r;
  }

  /** Los puntos de las celdas que tocan el rectángulo [x0, x1] × [y0, y1] (puede traer algunos de fuera) */
  forEachNear(x0: number, x1: number, y0: number, y1: number, visit: (i: number) => void): void {
    const [c0, c1, r0, r1] = [this.col(x0), this.col(x1), this.row(y0), this.row(y1)];
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const cell = c + this.cols * r;
        for (let j = this.start[cell]; j < this.start[cell + 1]; j++) visit(this.items[j]);
      }
    }
  }
}
