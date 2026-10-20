// Arnés temporal: monta la línea de tiempo con un clip de estrés y mide montaje y edición
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { insertKeys } from "../src/lib/animation";
import { defaultRotationCodecs } from "../src/lib/curves";

const params = new URLSearchParams(location.search);
const BONES = Number(params.get("bones") ?? 120);
const KEYS = Number(params.get("keys") ?? 300);
const mod = await import("../src/components/layout/Timeline");
const Timeline = mod.Timeline;

let seed = 1;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
const names = Array.from({ length: BONES }, (_, i) => `b${i}`);
const clip0 = {
  id: "c", name: "stress", fps: 24, start: 0, end: KEYS * 2,
  tracks: names.map((bone) => ({
    bone,
    rotation: Array.from({ length: KEYS }, (_, i) => ({ frame: i * 2, value: [rnd() * 0.1, 0, 0, 1], interpolation: i % 2 ? "bezier" : "linear" })),
    translation: [],
  })),
} as any;
const rows = names.map((bone, joint) => ({ joint, bone, depth: joint % 5 }));
const [clip, setClip] = createSignal(clip0);
const noop = () => {};
const props: any = {
  get clip() { return clip(); }, frame: 10, playing: false, rows, selectedJoint: 0, selection: new Set(), autoKey: true,
  interpolation: "linear", onFrame: noop, onTogglePlay: noop, onRangeChange: noop, onSelectJoint: noop, onSelection: noop,
  onMoveKeys: noop, onDeleteKeys: noop, onInsertKey: noop, onAutoKey: noop, onInterpolation: noop, mode: "keys", onMode: noop,
  onTool: noop, onlySelection: false, onOnlySelection: noop, onMarker: noop,
  onion: { enabled: false, before: 1, after: 1, step: 1, mesh: false }, onOnion: noop, paths: false, onPaths: noop,
  curves: { rows: [], rotation: defaultRotationCodecs, selection: new Set(), onSelection: noop, onEdit: noop },
  mixer: { mixer: { layers: [] }, clips: [], view: [0, 10], onSelectLayer: noop, onChange: noop, onAddLayer: noop, onBake: noop, onEnabled: noop },
};
const t0 = performance.now();
render(() => <Timeline {...props} />, document.getElementById("root")!);
const mount = performance.now() - t0;
const edits: number[] = [];
for (let i = 0; i < 3; i++) {
  const t1 = performance.now();
  setClip(insertKeys(clip(), 100 + i * 2 + 1, names.map((bone) => ({ bone, rotation: [0, 0, 0, 1] as any })), "linear"));
  edits.push(performance.now() - t1);
}
(window as any).__result = { old: !!params.get("old"), mount, edits, nodes: document.querySelectorAll("*").length };
