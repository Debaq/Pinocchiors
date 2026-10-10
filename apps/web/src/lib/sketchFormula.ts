// Curvas por ecuación del sketch: fórmulas con x (explícita) o t (paramétrica)
// y los parámetros del documento. Funciones puras: se prueban con node
// (e2e/sketchFormula.test.mjs).
//
// Fórmulas: + − * / ^, paréntesis, números con punto decimal (la coma separa
// argumentos), `pi`, `e`
// y sin cos tan asin acos atan sqrt abs exp ln log (base 10) min max pow
// floor ceil round sign. Los ángulos van en radianes.

import type { P2 } from "./cad.ts";

type Node = { k: "num"; v: number } | { k: "var"; name: string } | { k: "neg"; a: Node } | { k: "bin"; op: string; a: Node; b: Node } | { k: "call"; f: string; args: Node[] };

const FUNCS: Record<string, (...a: number[]) => number> = {
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  atan2: Math.atan2,
  sqrt: Math.sqrt,
  abs: Math.abs,
  exp: Math.exp,
  ln: Math.log,
  log: Math.log10,
  min: Math.min,
  max: Math.max,
  pow: Math.pow,
  floor: Math.floor,
  ceil: Math.ceil,
  round: Math.round,
  sign: Math.sign,
};

const CONSTS: Record<string, number> = { pi: Math.PI, e: Math.E };

function tokens(text: string): string[] {
  const out: string[] = [];
  const re = /\s*(\d+(?:\.\d*)?(?:e[-+]?\d+)?|\.\d+|[A-Za-z_][A-Za-z_0-9]*|\*\*|[-+*/^(),])/y;
  let i = 0;
  const t = text.trim();
  while (i < t.length) {
    re.lastIndex = i;
    const m = re.exec(t);
    if (!m) throw new Error(`no entiendo «${t.slice(i).trim().slice(0, 12)}»`);
    out.push(m[1] === "**" ? "^" : m[1]);
    i = re.lastIndex;
    while (i < t.length && /\s/.test(t[i])) i++;
  }
  return out;
}

/** Arma el árbol de una fórmula (error con mensaje si no se entiende) */
export function parseFormula(text: string): Node {
  const tk = tokens(text);
  let i = 0;
  const peek = () => tk[i];
  const take = (want?: string) => {
    const t = tk[i++];
    if (want && t !== want) throw new Error(`falta «${want}»`);
    if (t === undefined) throw new Error("la fórmula está incompleta");
    return t;
  };
  // sum := prod (('+'|'-') prod)*
  const sum = (): Node => {
    let a = prod();
    while (peek() === "+" || peek() === "-") a = { k: "bin", op: take(), a, b: prod() };
    return a;
  };
  // prod := unary (('*'|'/') unary | implicit factor)*
  const prod = (): Node => {
    let a = unary();
    for (;;) {
      if (peek() === "*" || peek() === "/") a = { k: "bin", op: take(), a, b: unary() };
      // Multiplicación implícita: 2x, 3(x+1), 2pi
      else if (peek() !== undefined && /^[A-Za-z_(]/.test(peek()!) && a.k === "num") a = { k: "bin", op: "*", a, b: unary() };
      else return a;
    }
  };
  const unary = (): Node => {
    if (peek() === "-") {
      take();
      return { k: "neg", a: unary() };
    }
    if (peek() === "+") {
      take();
      return unary();
    }
    return power();
  };
  // power := atom ('^' unary)?   (a la derecha: 2^3^2 = 2^9)
  const power = (): Node => {
    const a = atom();
    if (peek() === "^") {
      take();
      return { k: "bin", op: "^", a, b: unary() };
    }
    return a;
  };
  const atom = (): Node => {
    const t = take();
    if (t === "(") {
      const a = sum();
      take(")");
      return a;
    }
    if (/^[\d.]/.test(t)) return { k: "num", v: Number(t) };
    if (/^[A-Za-z_]/.test(t)) {
      if (peek() === "(") {
        if (!FUNCS[t]) throw new Error(`no conozco la función «${t}»`);
        take("(");
        const args = [sum()];
        while (peek() === ",") {
          take();
          args.push(sum());
        }
        take(")");
        return { k: "call", f: t, args };
      }
      return { k: "var", name: t };
    }
    throw new Error(`no esperaba «${t}»`);
  };
  const root = sum();
  if (i < tk.length) throw new Error(`sobra «${tk[i]}»`);
  return root;
}

/** Valor de una fórmula con esas variables (error si falta alguna) */
export function evalFormula(n: Node, vars: Record<string, number>): number {
  switch (n.k) {
    case "num":
      return n.v;
    case "var": {
      if (n.name in vars) return vars[n.name];
      if (n.name in CONSTS) return CONSTS[n.name];
      throw new Error(`no conozco «${n.name}»`);
    }
    case "neg":
      return -evalFormula(n.a, vars);
    case "bin": {
      const [a, b] = [evalFormula(n.a, vars), evalFormula(n.b, vars)];
      return n.op === "+" ? a + b : n.op === "-" ? a - b : n.op === "*" ? a * b : n.op === "/" ? a / b : Math.pow(a, b);
    }
    case "call":
      return FUNCS[n.f](...n.args.map((a) => evalFormula(a, vars)));
  }
}

export interface CurveSpec {
  kind: "explicit" | "parametric";
  /** Explícita: y(x). Paramétrica: x(t) */
  fx: string;
  /** Paramétrica: y(t) */
  fy?: string;
  from: number;
  to: number;
  /** Cantidad de puntos de paso */
  samples: number;
}

/**
 * Puntos de una curva por ecuación, con los parámetros del documento como
 * variables. Devuelve los puntos o un mensaje. Las muestras que no dan un
 * número (raíz de negativo, división por cero) cortan la curva: se avisa.
 */
export function sampleCurve(spec: CurveSpec, params: Record<string, number> = {}): P2[] | string {
  const n = Math.max(2, Math.min(500, Math.round(spec.samples)));
  if (!(Number.isFinite(spec.from) && Number.isFinite(spec.to)) || spec.from === spec.to) return "El intervalo tiene que tener largo";
  let fx: Node, fy: Node | undefined;
  try {
    fx = parseFormula(spec.fx);
    if (spec.kind === "parametric") fy = parseFormula(spec.fy ?? "");
  } catch (e) {
    return `Fórmula: ${(e as Error).message}`;
  }
  const out: P2[] = [];
  const v = spec.kind === "explicit" ? "x" : "t";
  try {
    for (let i = 0; i < n; i++) {
      const u = spec.from + ((spec.to - spec.from) * i) / (n - 1);
      const vars = { ...params, [v]: u };
      const p: P2 = spec.kind === "explicit" ? [u, evalFormula(fx, vars)] : [evalFormula(fx, vars), evalFormula(fy!, vars)];
      if (!Number.isFinite(p[0]) || !Number.isFinite(p[1])) return `La fórmula no da un número en ${v} = ${+u.toFixed(6)}`;
      out.push(p);
    }
  } catch (e) {
    return `Fórmula: ${(e as Error).message}`;
  }
  return out;
}
