# Plan: modelos de IA por empresa y roles (planifica, construye, revisa)

Pedido 2026-11-17: ordenar los modelos por empresa y decidir qué hace cada uno,
para mezclar empresas (p. ej. Opus de Anthropic revisa y DeepSeek construye):
un modelo potente y caro revisa, otros más económicos construyen.

Empresas: las que ya están en la app — Anthropic (la cuenta de Claude Code),
DeepSeek, OpenAI, OpenRouter, Ollama, LM Studio y «otra» (API compatible con
OpenAI). La lista de modelos se le pide a cada API (`GET {base}/models`).

## Fases

### M1 — Empresas y modelos
- Un proveedor guardado es una empresa (dirección + clave) con su lista de
  modelos elegidos (`models`: id, si ve imágenes, precio de entrada y salida
  por millón de tokens). `model` queda como el modelo de siempre.
- `ai_provider_models`: pide los modelos a la API (OpenAI: `data[].id`;
  OpenRouter trae precio y si ve imágenes). Sirve antes de guardar (dirección
  y clave del formulario) o con la clave guardada.
- Configuración ▸ Conector para IA ▸ Modelos de IA: por empresa, «Traer
  modelos», marcar los que se usan, si ven imágenes y su precio.
- Anthropic (Claude Code) aparece como empresa fija con Fable, Opus, Sonnet y Haiku.

### M2 — Roles y combinaciones
- Roles: Planifica (plan y encuesta), Construye (cada paso), Revisa (mira sin
  cambiar, aprueba o pide correcciones). Cada uno con un modelo de cualquier empresa.
- Reglas: correcciones máximas por paso (2), detenerse en cada paso o seguir.
- Combinaciones con nombre («Económica», «Calidad», «Sin internet»); una de un
  solo modelo es el chat de siempre. En el chat se elige la combinación.

### M3 — El ciclo
- Mensaje nuevo → Planifica (plan, encuesta) → Construye el paso → Revisa
  (solo herramientas de mirar) → aprobado: se marca y se detiene (o sigue);
  con correcciones: vuelve a Construye, hasta el máximo; después pregunta al usuario.
- Sesión de Claude Code por rol; el agente por API usa el modelo del rol.

### M4 — El chat con roles
- Cada mensaje dice quién habló («Construye · DeepSeek»); costo por rol y total.

## Avance

- 2026-11-17: plan escrito. M1 hecho (sin commitear): `ModelEntry` y `Provider.models` en
  `mcp/agent.rs`, `ai_provider_models` (prueba `models_tests`), `ModelPicker` en
  `AiProviders.tsx` (probado en Chromium con lista falsa).
- 2026-11-17: M2 hecho (sin commitear): `lib/aiTeams.ts` (combinaciones en `ai.teams`, lo
  elegido en `chat.choice`, migra `chat.engine`/`chat.model`), `AiTeams.tsx` en Configuración,
  «Usar» en el chat. El agente por API acepta el modelo del rol (`Agent::begin`). Por ahora el
  chat usa solo «Construye»: Planifica y Revisa entran en M3. «Paso a paso» quedó en el chat
  (no en la combinación).
