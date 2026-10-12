import * as THREE from "three";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";

/** Lo que se ve de un material en la bolita (colores lineales) */
export interface PreviewMaterial {
  color: [number, number, number];
  opacity: number;
  metallic: number;
  roughness: number;
  emissive: [number, number, number];
  flat: boolean;
  transmission?: number;
  ior?: number;
  clearcoat?: number;
  clearcoatRoughness?: number;
}

interface Stage {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  sphere: THREE.Mesh;
}

let stage: Stage | null = null;
const cache = new Map<string, string>();

/** Un solo contexto WebGL para todas las bolitas (la galería tiene decenas) */
function setup(): Stage {
  const canvas = document.createElement("canvas");
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(1);
  const scene = new THREE.Scene();
  scene.environmentIntensity = 0.6;
  scene.environment = new THREE.PMREMGenerator(renderer).fromScene(new RoomEnvironment(), 0.04).texture;

  // Fondo de tablero: así se nota lo translúcido
  const check = document.createElement("canvas");
  check.width = check.height = 16;
  const c = check.getContext("2d")!;
  c.fillStyle = "#8a8a90";
  c.fillRect(0, 0, 16, 16);
  c.fillStyle = "#5c5c62";
  c.fillRect(0, 0, 8, 8);
  c.fillRect(8, 8, 8, 8);
  const texture = new THREE.CanvasTexture(check);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.magFilter = THREE.NearestFilter;
  texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
  texture.repeat.set(3, 3);
  const backdrop = new THREE.Mesh(new THREE.PlaneGeometry(6, 6), new THREE.MeshBasicMaterial({ map: texture }));
  backdrop.position.z = -1.2;
  scene.add(backdrop);

  const light = new THREE.DirectionalLight(0xffffff, 1);
  light.position.set(2, 3, 4);
  scene.add(light, new THREE.AmbientLight(0xffffff, 0.05));

  const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 20);
  camera.position.set(0, 0, 4);
  const sphere = new THREE.Mesh(new THREE.SphereGeometry(1, 48, 32));
  scene.add(sphere);
  return { renderer, scene, camera, sphere };
}

/** Imagen (PNG en data URL) de un material sobre una esfera */
export function renderMaterialPreview(m: PreviewMaterial, size = 64): string {
  const key = `${size}|${m.color}|${m.opacity}|${m.metallic}|${m.roughness}|${m.emissive}|${m.flat}|${m.transmission}|${m.ior}|${m.clearcoat}|${m.clearcoatRoughness}`;
  const cached = cache.get(key);
  if (cached) return cached;
  stage ??= setup();
  const { renderer, scene, camera, sphere } = stage;
  renderer.setSize(size, size, false);
  const old = sphere.material as THREE.Material | undefined;
  old?.dispose();
  const translucent = m.opacity < 1;
  const color = new THREE.Color().setRGB(...m.color);
  sphere.material = m.flat
    ? new THREE.MeshBasicMaterial({ color, transparent: translucent, opacity: m.opacity })
    : new THREE.MeshPhysicalMaterial({
        color,
        metalness: m.metallic,
        roughness: m.roughness,
        emissive: new THREE.Color().setRGB(...m.emissive),
        transparent: translucent && !m.transmission,
        opacity: m.opacity,
        transmission: m.transmission ?? 0,
        ior: m.ior ?? 1.5,
        thickness: 0.8,
        clearcoat: m.clearcoat ?? 0,
        clearcoatRoughness: m.clearcoatRoughness ?? 0,
      });
  renderer.render(scene, camera);
  const url = renderer.domElement.toDataURL("image/png");
  if (cache.size > 400) cache.clear();
  cache.set(key, url);
  return url;
}
