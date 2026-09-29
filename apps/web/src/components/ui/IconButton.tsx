import { JSX, splitProps } from "solid-js";
import { clsx } from "clsx";

export type IconButtonVariant = "default" | "ghost" | "primary";
export type IconButtonSize = "sm" | "md" | "lg";

export interface IconButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  active?: boolean;
  "aria-label": string;
}

const variantClasses: Record<IconButtonVariant, string> = {
  default: `
    bg-surface/60 border-border text-text-muted
    hover:bg-surface hover:text-text hover:border-border-hover
    active:bg-bg-lighter
  `,
  ghost: `
    bg-transparent border-transparent text-text-muted
    hover:bg-surface/50 hover:text-text
    active:bg-surface
  `,
  primary: `
    bg-accent/20 border-accent/50 text-accent
    hover:bg-accent/30 hover:border-accent
    active:bg-accent/40
  `,
};

const sizeClasses: Record<IconButtonSize, string> = {
  sm: "w-6 h-6 p-1",
  md: "w-8 h-8 p-1.5",
  lg: "w-10 h-10 p-2",
};

export const IconButton = (props: IconButtonProps) => {
  const [local, rest] = splitProps(props, [
    "variant",
    "size",
    "active",
    "class",
    "children",
  ]);

  return (
    <button
      class={clsx(
        // Base
        "inline-flex items-center justify-center",
        "rounded-md border",
        "transition-all duration-100",
        "disabled:opacity-50 disabled:pointer-events-none",
        // Variant
        local.active ? variantClasses.primary : variantClasses[local.variant ?? "default"],
        // Size
        sizeClasses[local.size ?? "md"],
        // Custom
        local.class
      )}
      {...rest}
    >
      {local.children}
    </button>
  );
};
