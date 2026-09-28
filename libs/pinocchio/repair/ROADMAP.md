# pinocchio-repair

Análisis y reparación de mallas trianguladas. Reescrito el 2026-09-27: la
versión anterior borraba triángulos alargados válidos (abría cientos de
agujeros en STL de CAD sanos), dependía de los `twin` del half-edge (que se
rompen justo en las mallas a reparar) y rellenaba proyectando a un plano.

## Principios

- **Nada abre agujeros ni borra geometría válida.** Degeneradas se colapsan o
  se absorben en sus vecinas; lo non-manifold se separa duplicando vértices.
- **Sopa indexada** (`TriMesh`) + topología de aristas por ordenamiento
  (`topology::EdgeTopology`): admite aristas con cualquier número de caras y
  orientaciones mezcladas. La `Mesh` half-edge solo se construye al final.
- **Tolerancias relativas** a la diagonal de la caja envolvente.
- **Idempotente**: reparar una malla ya reparada no cambia nada.

## Pipeline (`repair_trimesh`)

1. Caras inválidas (índices fuera de rango/repetidos, NaN).
2. Costura (`weld::stitch_map`): funde vértices coincidentes **solo a lo largo
   de aristas de borde coincidentes**. Cierra costuras de STL/glTF sin fundir
   cuerpos que solo se tocan (si no, el análisis volvería a juntar lo que la
   reparación separó).
3. Degeneradas (altura ≤ tol): aguja → colapso de la arista corta; gorra →
   partir las vecinas por la arista larga (unión en T); astilla colgante → borrar.
4. Caras repetidas: se cancelan por orientación (pared interna entre dos
   cuerpos pegados desaparece).
5. `manifold::orient_and_split`: orientación por BFS (piezas cerradas hacia
   afuera por volumen, abiertas por mayoría), emparejamiento radial en aristas
   non-manifold (cada cara con su vecina hacia el interior, solo pares
   mutuos) y separación por abanicos. Verifica que ninguna arista quede con >2
   caras; si pasa, anula esos pares y reintenta. `zip_slits` recose rendijas
   de ancho cero que dejan los contactos coplanares.
6. Piezas sueltas (opcional, área < ratio · mayor).
7. Relleno (Liepa 2003): DP min-max diedro + área en 3D, prohibiendo aristas
   existentes; refinamiento por centroides + flips de Delaunay; fairing
   bi-laplaciano (mínimos cuadrados, Cholesky en envolvente + RCM). No rellena
   láminas (parche > 60 % del área de la pieza) ni rendijas (área ≈ 0).
8. Orientación hacia afuera por cáscara, con anidamiento por número de
   vueltas (todas las muestras dentro ⇒ anidada; solapadas no cuentan).

## Banco

```text
cargo run --release -p pinocchio-repair --example repair_report -- modelo.stl --brief [--punch N] [--obj out.obj]
```

`--punch N` abre N agujeros para probar el relleno. Con los 73 modelos
cargables de ~/Descargas (2026-09-27): los sanos quedan idénticos, 69 salen
sanos y el volumen se conserva. Los otros 4: cuerpos solapados (ver
pendientes) y una lámina suelta. Con agujeros abiertos a propósito se
recupera >99.5 % del volumen original.

## Pendiente

- **Cuerpos solapados / auto-intersecciones**: se detectan pero no se
  resuelven. Requiere unión booleana o reconstrucción volumétrica
  (`quadriflow-core::rebuild`). Donde se apoyan de forma coplanar quedan
  unas pocas aristas non-manifold (grupos de ≥3 bordes coincidentes).
- **Uniones en T** entre piezas (vértice sobre la arista de otra sin
  degenerada que lo delate): la costura no las cierra.
- **Atributos**: la app reconstruye la escena desde la malla reparada y
  pierde UVs/materiales. Habría que propagar el mapa de vértices.
- Rendimiento: 4.2 M caras ≈ 14 s (126 k ≈ 0.4 s). Paralelizar (feature `parallel`) e
  índices `u32` si hace falta.
