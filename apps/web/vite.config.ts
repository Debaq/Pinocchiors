import { defineConfig, type Plugin } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";

// Versión con el mismo formato que los tags de release:
// pre.<YYYY>.<MM>.<DD>.<sha7>, fecha del commit en UTC. Se ve en el splash
function appVersion(): string {
  try {
    const git = (args: string) => execSync(`git ${args}`, { encoding: "utf8", env: { ...process.env, TZ: "UTC0" } }).trim();
    const date = git("show -s --format=%cd --date=format-local:%Y.%m.%d HEAD");
    const sha = git("rev-parse --short=7 HEAD");
    return `pre.${date}.${sha}`;
  } catch {
    return process.env.npm_package_version ?? "";
  }
}
process.env.VITE_APP_VERSION ??= appVersion();

// El logo va dentro de splash.html: aparece en el primer cuadro sin esperar
// otra petición (en dev compite con todos los módulos de la ventana principal)
const inlineSplashLogo: Plugin = {
  name: "inline-splash-logo",
  transformIndexHtml: {
    order: "pre",
    handler: (html, ctx) =>
      ctx.filename.endsWith("splash.html")
        ? html.replace("<!-- logo -->", readFileSync(new URL("./src/assets/logo.svg", import.meta.url), "utf8"))
        : html,
  },
};

export default defineConfig({
  plugins: [solid(), tailwindcss(), inlineSplashLogo],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    // App de escritorio: los chunks se cargan del disco, no por red
    chunkSizeWarningLimit: 2500,
    // splash.html: ventana chica que se ve mientras carga la principal
    rollupOptions: {
      input: { main: "index.html", splash: "splash.html" },
      output: {
        // Librerías pesadas aparte: cachean mejor y el chunk principal baja de 500 kB
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("/three/")) return "three";
          if (id.includes("/solid-js/")) return "solid";
          return "vendor";
        },
      },
    },
  },
});
