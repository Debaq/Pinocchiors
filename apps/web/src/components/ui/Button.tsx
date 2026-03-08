import { JSX, ParentComponent, splitProps } from "solid-js";
import { clsx } from "clsx";

export type ButtonVariant = "default" | "primary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: JSX.Element;
  iconRight?: JSX.Element;
  loading?: boolean;
  fullWidth?: boolean;
}

const variantClasses: Record<ButtonVariant, string> = {
  default: `
    bg-current/80 border-border text-text
    hover:bg-current hover:border-border-hover
    active:bg-bg-lighter
  `,
  primary: `
    bg-accent border-accent text-bg
    hover:bg-accent-hover hover:border-accent-hover
    active:bg-accent-dim
  `,
  ghost: `
    bg-transparent border-transparent text-text-muted
    hover:bg-current/50 hover:text-text
    active:bg-current
  `,
  danger: `
    bg-red/20 border-red/50 text-red
    hover:bg-red/30 hover:border-red
    active:bg-red/40
  `,
};

const sizeClasses: Record<ButtonSize, string> = {
  sm: "h-7 px-3 text-xs gap-1.5",
  md: "h-9 px-5 text-sm gap-2",
  lg: "h-11 px-6 text-sm gap-2.5",
};

export const Button: ParentComponent<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, [
    "variant",
    "size",
    "icon",
    "iconRight",
    "loading",
    "fullWidth",
    "class",
    "children",
    "disabled",
  ]);

  return (
    <button
      class={clsx(
        // Base
        "inline-flex items-center justify-center",
        "font-medium rounded-md border",
        "transition-all duration-100",
        "disabled:opacity-50 disabled:pointer-events-none",
        // Variant
        variantClasses[local.variant ?? "default"],
        // Size
        sizeClasses[local.size ?? "md"],
        // Full width
        local.fullWidth && "w-full",
        // Custom
        local.class
      )}
      disabled={local.disabled || local.loading}
      {...rest}
    >
      {local.loading ? (
        <span class="animate-spin">
          <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none">
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="4"
            />
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
            />
          </svg>
        </span>
      ) : (
        local.icon && <span class="shrink-0">{local.icon}</span>
      )}
      {local.children}
      {local.iconRight && <span class="shrink-0">{local.iconRight}</span>}
    </button>
  );
};
