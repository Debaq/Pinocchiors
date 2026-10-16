# Entornos HDR incluidos

Imágenes de 1K (1024×512, formato Radiance `.hdr`) de [Poly Haven](https://polyhaven.com/hdris),
licencia CC0 (dominio público). Se embeben en el binario (`src/hdri.rs`) y el render con
GeppettoRay las ofrece como entornos predeterminados. Se referencian con el prefijo
`builtin:` más el id (por ejemplo `builtin:studio`). Para agregar uno, ponga el `.hdr` aquí y
añádalo a la lista `BUILTIN` de `src/hdri.rs`.

| id | archivo | autor/origen |
|---|---|---|
| studio | studio_small_03 | Poly Haven |
| loft | photo_studio_loft_hall | Poly Haven |
| sky | kloofendal_48d_partly_cloudy_puresky | Poly Haven |
| overcast | overcast_soil_puresky | Poly Haven |
| sunset | venice_sunset | Poly Haven |
| warehouse | empty_warehouse_01 | Poly Haven |
