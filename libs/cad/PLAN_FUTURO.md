# Más adelante

Áreas grandes de Onshape que no conviene empezar hasta tener lo de [PLANES.md](PLANES.md).
Cada una es un proyecto en sí; aquí queda qué es, qué necesita y por dónde empezar.

## Chapa metálica (*sheet metal*)

- **Qué es**: piezas de chapa con espesor constante: pestañas, dobleces con radio y factor K,
  alivios en las esquinas, y el **desarrollo** (la chapa plana para cortar con láser).
- **Necesita**: piezas ([PLAN_PIEZAS.md](PLAN_PIEZAS.md)), extrusión delgada y engrosar
  ([PLAN_CONSTRUCCION.md](PLAN_CONSTRUCCION.md)), DXF ([PLAN_PLANOS_2D.md](PLAN_PLANOS_2D.md))
  para exportar el desarrollo.
- **Por dónde empezar**: modelo propio de chapa (grafo de caras planas unidas por dobleces
  cilíndricos) en vez de intentar desplegar un sólido cualquiera. Primera versión: convertir un
  sólido de paredes delgadas en chapa + desarrollo + DXF; después pestaña en arista y doblez
  por línea.
- **Valor para el laboratorio**: corte láser de cajas y soportes.

## Superficies

- **Qué es**: modelar con superficies abiertas: extruir/revolucionar/barrer curvas sin cerrar,
  rellenar huecos (*fill*), recortar, extender, unir superficies y engrosarlas a sólido.
- **Necesita**: operaciones que devuelvan caras o conchas (`Shell`) en vez de sólidos; el modelo
  hoy asume sólido en `Tagged`. Agregar un tipo de cuerpo "superficie" a las piezas.
- **Por dónde empezar**: relleno de superficie (`BRepFill_Filling`, en TKFill — no enlazado) y
  engrosar; útil también para cerrar agujeros de escaneos convertidos a CAD.

## Versiones con nombre

- **Qué es**: el historial de versiones de Onshape: marcar "v1 enviada a imprimir", volver a
  ver una versión vieja, comparar y ramas.
- **Necesita**: el proyecto `.pinocchio` hoy guarda un estado. Guardar instantáneas del
  documento CAD (son JSON chicos; la geometría se recalcula) con nombre y fecha.
- **Por dónde empezar**: lista de versiones con nombre dentro del proyecto, "abrir versión" en
  solo lectura y "restaurar". Comparar dos versiones = mapa de diferencias de volumen (booleana
  entre ambos sólidos). Las ramas y la fusión, mucho después.

## Configuraciones

- **Qué es**: una tabla de variantes de la misma pieza (M3, M4, M5 con sus medidas; con o sin
  agujero) que cambian parámetros y supresión de operaciones.
- **Necesita**: parámetros con nombre (ya existen, `Document.parameters`) y supresión por
  operación (ya existe). Falta la tabla.
- *Hecho el 2026-10-08*: `Document.configurations: Vec<Configuration { name, values: {parámetro →
  expresión}, suppressed }>` y `active_configuration`; `Document::resolve` aplica la activa antes
  de las fórmulas (lo de base no cambia). Panel: sección Configuraciones (Base + variantes, valores
  vacíos = los de base, operaciones apagadas); Exportar: "Todas las configuraciones" saca un
  archivo por variante (nombre-variante.ext) y vuelve a la elegida.

## Otros

- **FeatureScript propio**: operaciones definidas por el usuario. Alternativa razonable:
  "operación compuesta" que guarda un trozo de historial con parámetros de entrada y se reutiliza.
- **Colaboración en tiempo real**: fuera de alcance para una app de escritorio; las versiones
  con nombre cubren buena parte de la necesidad.
- **Renderizado**: la app ya tiene materiales y luces para mallas; exportar el sólido a la
  escena de la app ("Usar como modelo") ya existe.

## Orden sugerido dentro de esta lista

1. Configuraciones (barato, reutiliza parámetros y supresión).
2. Versiones con nombre.
3. Chapa metálica (alto valor para láser).
4. Superficies.
