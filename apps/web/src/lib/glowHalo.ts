import * as THREE from "three";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";

/** Capa donde se dibuja solo lo que brilla */
const GLOW_LAYER = 8;

/**
 * Halo de los materiales con glow: la malla se dibuja aparte solo con su
 * emisión (negro donde no brilla), se le aplica bloom y el resultado se suma
 * a lo ya dibujado. Lo demás de la escena (guías, esqueleto) no brilla.
 */
export class GlowHalo {
  private composer: EffectComposer;
  private bloom: UnrealBloomPass;
  private quadScene = new THREE.Scene();
  private quadCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  private quad: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  private renderPass: RenderPass;
  private clear = new THREE.Color();

  constructor(
    private renderer: THREE.WebGLRenderer,
    width: number,
    height: number
  ) {
    this.renderPass = new RenderPass(new THREE.Scene(), new THREE.PerspectiveCamera());
    this.bloom = new UnrealBloomPass(new THREE.Vector2(width, height), 0.3, 0.2, 0);
    this.composer = new EffectComposer(renderer);
    this.composer.renderToScreen = false;
    this.composer.addPass(this.renderPass);
    this.composer.addPass(this.bloom);
    this.composer.setSize(width, height);
    const material = new THREE.MeshBasicMaterial({
      blending: THREE.AdditiveBlending,
      transparent: true,
      depthTest: false,
      depthWrite: false,
      toneMapped: false,
    });
    this.quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), material);
    this.quad.frustumCulled = false;
    this.quadScene.add(this.quad);
  }

  setSize(width: number, height: number): void {
    this.composer.setSize(width, height);
  }

  /** Dibuja el halo de `mesh` sobre el cuadro ya dibujado; `strength` va de 0 a 1 */
  render(scene: THREE.Scene, camera: THREE.Camera, mesh: THREE.Mesh, glow: THREE.Material | THREE.Material[], strength: number): void {
    const renderer = this.renderer;
    const background = scene.background;
    const material = mesh.material;
    const mask = camera.layers.mask;
    renderer.getClearColor(this.clear);
    const clearAlpha = renderer.getClearAlpha();
    const autoClear = renderer.autoClear;

    scene.background = null;
    mesh.material = glow;
    mesh.layers.enable(GLOW_LAYER);
    camera.layers.set(GLOW_LAYER);
    renderer.setClearColor(0x000000, 1);
    this.renderPass.scene = scene;
    this.renderPass.camera = camera;
    this.bloom.strength = 0.25 + 0.35 * Math.min(1, strength);
    try {
      this.composer.render();
    } finally {
      scene.background = background;
      mesh.material = material;
      mesh.layers.disable(GLOW_LAYER);
      camera.layers.mask = mask;
      renderer.setClearColor(this.clear, clearAlpha);
    }

    this.quad.material.map = this.composer.readBuffer.texture;
    renderer.setRenderTarget(null);
    renderer.autoClear = false;
    renderer.render(this.quadScene, this.quadCamera);
    renderer.autoClear = autoClear;
  }

  dispose(): void {
    this.composer.dispose();
    this.bloom.dispose();
    this.quad.geometry.dispose();
    this.quad.material.dispose();
  }
}
