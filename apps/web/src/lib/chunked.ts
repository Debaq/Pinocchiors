import { createEffect, createSignal, onCleanup, untrack, type Accessor } from "solid-js";

/**
 * Cálculo cuadro a cuadro que no congela la interfaz: espera `delay` ms sin
 * cambios en `source`, y recorre los cuadros en tandas de `budget` ms
 * cediendo el hilo entre una y otra. Si `source` cambia a mitad, se descarta
 * lo hecho y se empieza de nuevo; mientras tanto se conserva el último
 * resultado completo. `source` devuelve `undefined` cuando no hay nada que calcular.
 */
export function createChunked<S, A, T>(options: {
  source: () => S | undefined;
  frames: (s: S) => [number, number];
  start: (s: S) => A;
  step: (s: S, acc: A, frame: number) => void;
  finish: (acc: A) => T;
  empty: T;
  delay?: number;
  budget?: number;
}): Accessor<T> {
  const [result, setResult] = createSignal<T>(options.empty);
  const { delay = 120, budget = 8 } = options;
  createEffect(() => {
    const s = options.source();
    if (s === undefined) {
      setResult(() => options.empty);
      return;
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    onCleanup(() => clearTimeout(timer));
    untrack(() => {
      const [from, to] = options.frames(s);
      const acc = options.start(s);
      let f = from;
      const run = () => {
        const t0 = performance.now();
        while (f <= to && performance.now() - t0 < budget) options.step(s, acc, f++);
        if (f <= to) timer = setTimeout(run, 0);
        else setResult(() => options.finish(acc));
      };
      timer = setTimeout(run, delay);
    });
  });
  return result;
}
