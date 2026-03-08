import { Component } from "solid-js";
import { clsx } from "clsx";
import { Button } from "../ui";
import * as Icons from "../icons";

export interface WelcomeScreenProps {
  onImport?: () => void;
}

export const WelcomeScreen: Component<WelcomeScreenProps> = (props) => {
  return (
    <div class="absolute inset-0 flex items-center justify-center z-10 pointer-events-auto p-8">
      <div
        class={clsx(
          "flex flex-col items-center gap-6 p-12",
          "rounded-2xl border-2 border-dashed border-border",
          "bg-bg-darker/80 backdrop-blur-sm",
          "max-w-md w-full"
        )}
      >
        {/* Icon */}
        <div
          class={clsx(
            "w-20 h-20 rounded-2xl",
            "bg-accent/10 border border-accent/30",
            "flex items-center justify-center",
            "text-accent"
          )}
        >
          <Icons.FilePlus size={40} />
        </div>

        {/* Text */}
        <div class="text-center space-y-2">
          <h2 class="text-lg font-semibold text-text">
            Arrastra un modelo 3D aqui
          </h2>
          <p class="text-xs text-text-muted leading-relaxed">
            o usa el boton para importar desde el sistema de archivos
          </p>
        </div>

        {/* Button */}
        <Button
          onClick={props.onImport}
          variant="primary"
          size="lg"
          icon={<Icons.FolderOpen size={18} />}
        >
          Importar Modelo
        </Button>

        {/* Supported formats */}
        <p class="text-[10px] text-text-dim text-center">
          Formatos soportados: GLB, GLTF, OBJ, STL, FBX
        </p>
      </div>
    </div>
  );
};
