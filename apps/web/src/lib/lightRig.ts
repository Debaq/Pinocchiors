import * as THREE from "three";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

/** Luces del visor (panel Luces); las mismas en todos los espacios */
export interface LightSettings {
  /** Dirección de la luz principal (grados): acimut alrededor del modelo y elevación */
  azimuth: number;
  elevation: number;
  intensity: number;
  color: string;
  /** Luz de relleno (del lado opuesto) y ambiente */
  fill: number;
  ambient: number;
  /** Luz que sale desde la cámara */
  headlight: boolean;
  /** Reflejos de entorno (iluminación por imagen) */
  environment: number;
}

export const defaultLights: LightSettings = {
  azimuth: 45,
  elevation: 45,
  intensity: 2,
  color: "#ffffff",
  fill: 0.6,
  ambient: 0.3,
  headlight: false,
  environment: 0.6,
};

/**
 * Las luces de los dos visores (el principal y el de Diseñar): principal,
 * relleno del lado opuesto, ambiente, luz de cámara y entorno para los
 * reflejos PBR. Se ubican alrededor del modelo con `place`.
 */
export class LightRig {
  readonly key = new THREE.DirectionalLight(0xffffff, 1);
  private fillLight = new THREE.DirectionalLight(0x8be9fd, 1);
  private ambientLight = new THREE.HemisphereLight(0xffffff, 0x44475a, 1);
  private headLight = new THREE.DirectionalLight(0xffffff, 0);
  private environmentMap: THREE.Texture;
  private settings: LightSettings = { ...defaultLights };
  private center = new THREE.Vector3();
  private radius = 5;

  constructor(
    private scene: THREE.Scene,
    camera: THREE.Camera,
    renderer: THREE.WebGLRenderer,
  ) {
    scene.add(this.ambientLight);
    scene.add(this.key, this.key.target);
    scene.add(this.fillLight, this.fillLight.target);
    // Luz de cámara: cuelga de la cámara y apunta hacia adelante
    camera.add(this.headLight, this.headLight.target);
    this.headLight.target.position.set(0, 0, -1);
    if (!camera.parent) scene.add(camera);
    const pmrem = new THREE.PMREMGenerator(renderer);
    this.environmentMap = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
    pmrem.dispose();
    this.apply();
  }

  get(): LightSettings {
    return { ...this.settings };
  }

  set(settings: LightSettings) {
    this.settings = { ...settings };
    this.apply();
  }

  /** Alrededor de qué y a qué distancia van la principal y el relleno */
  place(center: THREE.Vector3, radius: number) {
    this.center.copy(center);
    this.radius = Math.max(radius, 1e-3);
    this.apply();
  }

  private apply() {
    const s = this.settings;
    const direction = (azimuth: number, elevation: number) => {
      const a = THREE.MathUtils.degToRad(azimuth);
      const e = THREE.MathUtils.degToRad(elevation);
      return new THREE.Vector3(Math.cos(e) * Math.sin(a), Math.sin(e), Math.cos(e) * Math.cos(a));
    };
    this.key.position.copy(this.center).addScaledVector(direction(s.azimuth, s.elevation), 2 * this.radius);
    this.key.target.position.copy(this.center);
    this.key.color.set(s.color);
    this.key.intensity = s.intensity;
    this.fillLight.position.copy(this.center).addScaledVector(direction(s.azimuth + 180, 20), 2 * this.radius);
    this.fillLight.target.position.copy(this.center);
    this.fillLight.intensity = s.fill;
    this.ambientLight.intensity = s.ambient;
    this.headLight.intensity = s.headlight ? 0.6 * s.intensity : 0;
    this.scene.environment = s.environment > 0 ? this.environmentMap : null;
    this.scene.environmentIntensity = s.environment;
  }

  dispose() {
    this.environmentMap.dispose();
  }
}

/** El mismo renderer en los dos visores: suavizado, densidad de píxeles y tono */
export function configureRenderer(renderer: THREE.WebGLRenderer) {
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;
}
