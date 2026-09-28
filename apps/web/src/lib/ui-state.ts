import { createSignal, type Accessor } from "solid-js";

const PREFIX = "pinocchio.ui.";

/** Señal que recuerda su valor entre sesiones (pestaña activa, anchos, paneles abiertos) */
export function createPersisted<T>(key: string, initial: T): [Accessor<T>, (value: T) => void] {
  let start = initial;
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw !== null) start = JSON.parse(raw) as T;
  } catch {
    // Sin almacenamiento (ventana privada, vista previa): vale el valor inicial
  }
  const [get, set] = createSignal<T>(start);
  const save = (value: T) => {
    set(() => value);
    try {
      localStorage.setItem(PREFIX + key, JSON.stringify(value));
    } catch {
      // Idem
    }
  };
  return [get, save];
}

/**
 * Arrastre de un divisor: llama `onMove` con el desplazamiento desde el punto
 * de partida hasta soltar. Mantiene el cursor aunque el puntero salga del divisor.
 */
export function startDrag(e: PointerEvent, cursor: string, onMove: (dx: number, dy: number) => void) {
  e.preventDefault();
  const startX = e.clientX;
  const startY = e.clientY;
  const prevCursor = document.body.style.cursor;
  const prevSelect = document.body.style.userSelect;
  document.body.style.cursor = cursor;
  document.body.style.userSelect = "none";

  const move = (ev: PointerEvent) => onMove(ev.clientX - startX, ev.clientY - startY);
  const up = () => {
    document.body.style.cursor = prevCursor;
    document.body.style.userSelect = prevSelect;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}
