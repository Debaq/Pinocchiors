//! Entornos HDR que trae la app (Poly Haven, CC0, 1K). Van dentro del binario: el sistema
//! solo los carga a memoria si se usan.

/// Prefijo del "camino" de un entorno incluido: `builtin:studio`
pub const PREFIX: &str = "builtin:";

const BUILTIN: &[(&str, &[u8])] = &[
    ("studio", include_bytes!("../assets/hdri/studio_small_03.hdr")),
    ("loft", include_bytes!("../assets/hdri/photo_studio_loft_hall.hdr")),
    ("sky", include_bytes!("../assets/hdri/kloofendal_48d_partly_cloudy_puresky.hdr")),
    ("overcast", include_bytes!("../assets/hdri/overcast_soil_puresky.hdr")),
    ("sunset", include_bytes!("../assets/hdri/venice_sunset.hdr")),
    ("warehouse", include_bytes!("../assets/hdri/empty_warehouse_01.hdr")),
];

/// Bytes del entorno incluido con ese id (`studio`, `loft`, `sky`, `overcast`, `sunset`, `warehouse`)
pub fn builtin(id: &str) -> Option<&'static [u8]> {
    BUILTIN.iter().find(|(name, _)| *name == id).map(|(_, bytes)| *bytes)
}

/// Lee el entorno de `path`: un id incluido (`builtin:studio`) o un archivo del disco
pub fn load(path: &str) -> Result<std::borrow::Cow<'static, [u8]>, String> {
    use std::borrow::Cow;
    match path.strip_prefix(PREFIX) {
        Some(id) => builtin(id).map(Cow::Borrowed).ok_or_else(|| format!("No existe el entorno incluido «{id}»")),
        None => std::fs::read(path).map(Cow::Owned).map_err(|e| format!("No se pudo leer la imagen de entorno: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_incluidos_se_leen_como_entorno() {
        for (id, bytes) in BUILTIN {
            let map = geppettoray::EnvMap::from_bytes(bytes, 1.0, 0.0);
            assert!(map.is_some(), "{id} no es un HDR válido");
            assert!(bytes.starts_with(b"#?RADIANCE"), "{id}");
        }
    }

    #[test]
    fn carga_por_id_y_por_ruta() {
        assert!(load("builtin:studio").is_ok());
        assert!(load("builtin:no_existe").unwrap_err().contains("no_existe"));
        assert!(load("/no/existe.hdr").is_err());
    }
}
