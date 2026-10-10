// Rueda del mouse sobre un campo numérico: sube o baja el valor un paso.
// Vale para <input type="number"> y para los de texto marcados con
// `data-wheel-number` (cuando el texto es un número simple, no una fórmula).
// Paso: `data-wheel-step`, si no el `step` del campo, si no 1. Shift ×10, Ctrl ×0,1.
// Se avisa con "input" y "change", como si se hubiera escrito.

const NUMBER = /^\s*[-+]?(\d+([.,]\d*)?|[.,]\d+)\s*$/;
/** Si la rueda venía desplazando un panel, pasar por encima de un campo no lo cambia */
const SCROLL_GRACE_MS = 400;

const decimals = (x: number) => {
  const s = String(x);
  const e = s.match(/e-(\d+)$/);
  if (e) return Number(e[1]);
  return s.includes(".") ? s.split(".")[1].length : 0;
};

function wheelTarget(el: EventTarget | null): HTMLInputElement | undefined {
  if (!(el instanceof HTMLInputElement) || el.disabled || el.readOnly) return undefined;
  if (el.type === "number") return el;
  if (el.type === "text" && el.dataset.wheelNumber !== undefined && NUMBER.test(el.value)) return el;
  return undefined;
}

export function installWheelNumbers() {
  let lastScroll = 0;
  document.addEventListener(
    "wheel",
    (e) => {
      const input = wheelTarget(e.target);
      if (!input) {
        lastScroll = e.timeStamp;
        return;
      }
      if (e.timeStamp - lastScroll < SCROLL_GRACE_MS) {
        lastScroll = e.timeStamp;
        return;
      }
      const delta = e.deltaY || e.deltaX;
      if (!delta) return;
      e.preventDefault();
      e.stopPropagation();
      const attr = input.dataset.wheelStep ?? (input.type === "number" ? input.step : "");
      const base = Number(attr) > 0 ? Number(attr) : 1;
      const step = base * (e.shiftKey ? 10 : e.ctrlKey ? 0.1 : 1);
      const current = Number(input.value.replace(",", ".")) || 0;
      let next = current + (delta < 0 ? step : -step);
      if (input.min !== "" && Number.isFinite(Number(input.min))) next = Math.max(next, Number(input.min));
      if (input.max !== "" && Number.isFinite(Number(input.max))) next = Math.min(next, Number(input.max));
      // Sin restos de coma flotante (0,1 + 0,2 = 0,30000000000000004)
      next = Number(next.toFixed(Math.min(10, Math.max(decimals(step), decimals(current)))));
      if (next === current) return;
      input.value = String(next);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    },
    { passive: false, capture: true },
  );
}
