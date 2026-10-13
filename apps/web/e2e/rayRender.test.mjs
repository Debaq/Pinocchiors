// Pruebas de lib/rayRender.ts (render con GeppettoRay desde la interfaz):
//   node --test apps/web/e2e/rayRender.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { formatEta, hexToLinear, imageFov, raySun, renderRay, renderRayAnimation } from "../src/lib/rayRender.ts";

const lights = { azimuth: 90, elevation: 0, intensity: 2, color: "#ffffff", fill: 0.5, ambient: 0.3, headlight: false, environment: 0.6 };
const view = { position: [0, 0, 5], target: [0, 0, 4], up: [0, 1, 0], fov: 40, viewAspect: 1.5, lights };
const opts = { width: 64, height: 32, background: "viewer", color: "#fff", color2: "#000", lights: null, groundShadow: false, samples: 1, format: "png", quality: 1, hideGrid: true, hideSkeleton: true, hideGizmos: true, hideSelection: true, shadows: true, ambientOcclusion: true };
const close = (a, b, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} vs ${b}`);

test("sRGB a lineal", () => {
  assert.deepEqual(hexToLinear("#000000"), [0, 0, 0]);
  assert.deepEqual(hexToLinear("#ffffff"), [1, 1, 1]);
  close(hexToLinear("#808080")[0], 0.2158605, 1e-5);
});

test("campo de visión: se recorta al centro como el visor", () => {
  assert.equal(imageFov(40, 1.5, 1.2), 40, "más cuadrada que la vista: queda igual");
  // Más ancha: se conserva el ancho, el alto baja
  const f = imageFov(40, 1.5, 3);
  assert.ok(f < 40);
  close(Math.tan((f * Math.PI) / 360) * 3, Math.tan((40 * Math.PI) / 360) * 1.5);
});

test("luces del visor a luces del motor", () => {
  const { suns, ambient } = raySun(lights, [0, 0, -1]);
  assert.equal(suns.length, 2);
  // Acimut 90°, elevación 0 → desde +X; intensidad 2 blanca
  close(suns[0].direction[0], 1);
  close(suns[0].color[0], 2);
  // El relleno viene del lado opuesto
  assert.ok(suns[1].direction[0] < 0);
  assert.ok(ambient[0] > 0 && ambient[0] === ambient[1]);
  const head = raySun({ ...lights, headlight: true }, [0, 0, -1]).suns;
  assert.equal(head.length, 3);
  // Sale de la cámara hacia el modelo: la cámara mira a -Z, la luz viene desde +Z
  assert.deepEqual(head[2].direction.map((v) => v + 0), [0, 0, 1]);
});

test("tiempo restante", () => {
  assert.equal(formatEta(12.4), "12 s");
  assert.equal(formatEta(65), "1:05");
  assert.equal(formatEta(NaN), "");
});

/** invoke falso que imita al comando de Rust: una pasada por consulta */
function fakeInvoke(script) {
  const calls = [];
  let i = 0;
  const invoke = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === "ray_render_start" || cmd === "ray_render_animation_start") return undefined;
    if (cmd === "ray_render_cancel") {
      script.cancelled = true;
      return undefined;
    }
    if (cmd === "ray_render_status") {
      const st = script.cancelled ? { ...script.steps.at(-1), phase: "cancelled" } : script.steps[Math.min(i++, script.steps.length - 1)];
      return { passes: 4, pass: 0, samples: 0, totalSamples: 32, elapsedSecs: 0, etaSecs: 0, version: 0, error: null, ...st };
    }
    if (cmd === "ray_render_image") return new Uint8Array([137, 80, 78, 71]).buffer;
    throw new Error(cmd);
  };
  return { invoke, calls };
}

test("renderRay: manda la petición, sigue el avance y devuelve el PNG", async () => {
  const { invoke, calls } = fakeInvoke({
    steps: [
      { phase: "running", pass: 1, samples: 8, version: 1, etaSecs: 3 },
      { phase: "running", pass: 2, samples: 16, version: 2, etaSecs: 1 },
      { phase: "done", pass: 4, samples: 32, version: 3 },
    ],
  });
  const progress = [];
  const previews = [];
  const image = await renderRay(invoke, view, { ...opts, background: "transparent" }, { samples: 32, denoise: true }, (p) => progress.push(p), undefined, (png) => previews.push(png.length));
  const req = calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.equal(req.width, 64);
  assert.equal(req.samples, 32);
  assert.equal(req.transparent, true);
  assert.equal(req.background, null);
  assert.equal(req.denoise, true);
  assert.deepEqual(req.camera.position, [0, 0, 5]);
  assert.ok(req.camera.vfovDeg < 40, "imagen más ancha que la vista: el alto baja");
  assert.equal(req.suns.length, 2);
  assert.deepEqual(progress.map((p) => p.fraction), [0.25, 0.5, 1]);
  assert.equal(previews.length, 3, "una vista previa por cada imagen nueva");
  assert.deepEqual([image.width, image.height], [64, 32]);
  assert.equal(image.bytes.length, 4);
});

test("renderRay: color de fondo sólido y JPG sin alfa", async () => {
  const mk = () => fakeInvoke({ steps: [{ phase: "done", pass: 4, samples: 32, version: 1 }] });
  let m = mk();
  // El JPG no guarda alfa: "transparente" sale blanco. (El paso a JPG usa el DOM: aquí solo la petición.)
  await renderRay(m.invoke, view, { ...opts, background: "color", color: "#ff0000" }, { samples: 8, denoise: false }).catch(() => {});
  const req = m.calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.equal(req.transparent, false);
  assert.deepEqual(req.background, [1, 0, 0]);
  m = mk();
  await renderRay(m.invoke, view, { ...opts, background: "transparent", format: "jpeg" }, { samples: 8, denoise: false }).catch(() => {});
  const req2 = m.calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.equal(req2.transparent, false);
  assert.deepEqual(req2.background, [1, 1, 1]);
});

test("renderRay: sol con tamaño, tono, suelo con sombra y entorno HDR en la petición", async () => {
  const mk = () => fakeInvoke({ steps: [{ phase: "done", pass: 4, samples: 32, version: 1 }] });
  let m = mk();
  await renderRay(m.invoke, view, { ...opts, groundShadow: false }, { samples: 8, denoise: false }).catch(() => {});
  const req = m.calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.ok(req.sunAngularRadius > 0, "el sol tiene disco");
  assert.equal(req.toneMap, true);
  assert.equal(req.shadowFloor, null);
  assert.equal(req.hdri, null);
  m = mk();
  const hdri = { path: "/cielos/estudio.hdr", intensity: 1.5, rotationDeg: 90 };
  await renderRay(m.invoke, view, { ...opts, groundShadow: true }, { samples: 8, denoise: false, hdri }).catch(() => {});
  const req2 = m.calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.ok(req2.shadowFloor > 0 && req2.shadowFloor <= 1);
  assert.deepEqual(req2.hdri, hdri);
});

test("renderRay: cancelar avisa al motor y lanza AbortError", async () => {
  const script = { steps: [{ phase: "running", pass: 1, samples: 8, version: 1 }] };
  const { invoke, calls } = fakeInvoke(script);
  const controller = new AbortController();
  setTimeout(() => controller.abort(), 20);
  await assert.rejects(renderRay(invoke, view, opts, { samples: 32, denoise: false }, undefined, controller.signal), (e) => e.name === "AbortError");
  assert.equal(calls.filter(([c]) => c === "ray_render_cancel").length, 1);
});

test("renderRay: un error del motor se propaga", async () => {
  const { invoke } = fakeInvoke({ steps: [{ phase: "error", error: "La escena no tiene geometría para renderizar" }] });
  await assert.rejects(renderRay(invoke, view, opts, { samples: 8, denoise: false }), /no tiene geometría/);
});

test("renderRayAnimation: manda el clip y el degradado, sigue los cuadros y devuelve el resultado", async () => {
  const { invoke, calls } = fakeInvoke({
    steps: [
      { phase: "running", pass: 1, samples: 8, version: 1, frame: 1, frames: 4, fraction: 0.125, etaSecs: 9 },
      { phase: "running", pass: 2, samples: 16, version: 2, frame: 3, frames: 4, fraction: 0.6, etaSecs: 2 },
      { phase: "done", pass: 4, samples: 32, version: 3, frame: 4, frames: 4, fraction: 1, skipped: 1, video: "/tmp/x/anim.mp4", note: null },
    ],
  });
  const progress = [];
  const anim = {
    clips: [{ name: "caminar", fps: 24, tracks: [] }],
    clip: 0,
    firstFrame: 0,
    lastFrame: 3,
    fps: 24,
    dir: "/tmp/x",
    name: "anim",
    shutter: 0.5,
    motionSamples: 8,
    resume: true,
    video: true,
    nonDeforming: [],
    useRetopology: false,
  };
  const result = await renderRayAnimation(
    invoke,
    view,
    { ...opts, background: "gradient", color: "#ff0000", color2: "#0000ff", format: "jpeg" },
    { samples: 32, denoise: false },
    anim,
    (p) => progress.push(p),
  );
  const [, args] = calls.find(([c]) => c === "ray_render_animation_start");
  assert.equal(args.request.transparent, true, "el degradado va debajo: el motor entrega alfa");
  assert.deepEqual(args.animation.gradient, { top: [255, 0, 0], bottom: [0, 0, 255] });
  assert.equal(args.animation.clips.length, 1);
  assert.equal(args.animation.motionSamples, 8);
  assert.deepEqual(progress.map((p) => [p.frame, p.frames, p.fraction]), [[1, 4, 0.125], [3, 4, 0.6], [4, 4, 1]]);
  assert.deepEqual(result, { frames: 4, skipped: 1, video: "/tmp/x/anim.mp4", note: null });
  assert.equal(calls.filter(([c]) => c === "ray_render_start").length, 0);
});

test("renderRay: el fondo de estudio llena el cuadro (sin alfa ni suelo con sombra)", async () => {
  const { invoke, calls } = fakeInvoke({ steps: [{ phase: "done", pass: 4, samples: 32, version: 1 }] });
  await renderRay(
    invoke,
    view,
    { ...opts, background: "gradient", groundShadow: true, studio: { color: "#ffffff", gloss: 0.5 } },
    { samples: 8, denoise: false },
  ).catch(() => {});
  const req = calls.find(([c]) => c === "ray_render_start")[1].request;
  assert.deepEqual(req.studio, { color: [1, 1, 1], gloss: 0.5 });
  assert.equal(req.transparent, false);
  assert.equal(req.background, null);
  assert.equal(req.shadowFloor, null);
});
