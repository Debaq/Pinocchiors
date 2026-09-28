import { Select as KobalteSelect } from "@kobalte/core/select";
import { Show, createMemo } from "solid-js";
import { clsx } from "clsx";

export interface SelectOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

export interface SelectProps {
  options: SelectOption[];
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  class?: string;
  label?: string;
}

export const Select = (props: SelectProps) => {
  // Kobalte compara la opción elegida por referencia: si `options` llega como
  // un arreglo nuevo en cada lectura, el valor no está en la lista, Kobalte lo
  // toma como un cambio y avisa con onChange, que vuelve a pedir el valor… en
  // bucle. Se lee una vez por cambio y solo se avisa si el valor cambió
  const options = createMemo(() => props.options);
  const selected = createMemo(() => options().find((o) => o.value === props.value));

  return (
    <KobalteSelect
      options={options()}
      optionValue="value"
      optionTextValue="label"
      optionDisabled="disabled"
      value={selected()}
      onChange={(option) => option && option.value !== props.value && props.onChange?.(option.value)}
      placeholder={props.placeholder ?? "Seleccionar..."}
      disabled={props.disabled}
      itemComponent={(itemProps) => (
        <KobalteSelect.Item
          item={itemProps.item}
          class={clsx(
            "flex items-center justify-between",
            "px-3 py-2 text-sm",
            "cursor-pointer outline-none",
            "text-text-muted",
            "hover:bg-current/50 hover:text-text",
            "data-[highlighted]:bg-current/50 data-[highlighted]:text-text",
            "data-[selected]:text-accent data-[selected]:bg-accent/10",
            "data-[disabled]:opacity-50 data-[disabled]:pointer-events-none"
          )}
        >
          <KobalteSelect.ItemLabel>{itemProps.item.rawValue.label}</KobalteSelect.ItemLabel>
          <KobalteSelect.ItemIndicator>
            <svg class="w-4 h-4 text-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12" />
            </svg>
          </KobalteSelect.ItemIndicator>
        </KobalteSelect.Item>
      )}
    >
      <Show when={props.label}>
        <KobalteSelect.Label class="block text-xs text-text-muted mb-1.5">
          {props.label}
        </KobalteSelect.Label>
      </Show>

      <KobalteSelect.Trigger
        class={clsx(
          "w-full flex items-center justify-between",
          "h-8 px-3 rounded-md",
          "bg-bg-lighter border border-border",
          "text-sm text-text",
          "transition-colors duration-100",
          "hover:border-border-hover",
          "focus:border-accent focus:outline-none",
          "disabled:opacity-50 disabled:pointer-events-none",
          props.class
        )}
      >
        <KobalteSelect.Value<SelectOption>>
          {(state) => state.selectedOption().label}
        </KobalteSelect.Value>
        <KobalteSelect.Icon>
          <svg class="w-4 h-4 text-text-muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M6 9l6 6 6-6" />
          </svg>
        </KobalteSelect.Icon>
      </KobalteSelect.Trigger>

      <KobalteSelect.Portal>
        <KobalteSelect.Content
          class={clsx(
            "bg-bg-lighter border border-border rounded-md",
            "shadow-lg overflow-hidden",
            "animate-slide-up",
            "z-50"
          )}
        >
          <KobalteSelect.Listbox class="max-h-60 overflow-auto py-1" />
        </KobalteSelect.Content>
      </KobalteSelect.Portal>
    </KobalteSelect>
  );
};
