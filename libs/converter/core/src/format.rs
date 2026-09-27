use std::path::Path;

/// Todos los formatos disponibles.
const ALL_FORMATS: [Format; 5] = [
    Format::Gltf,
    Format::Stl,
    Format::Obj,
    Format::Usda,
    Format::Usdz,
];

/// Formatos 3D soportados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Gltf,
    Usda,
    Usdz,
    Stl,
    Obj,
}

impl Format {
    /// Detecta el formato a partir de la extensión del archivo.
    pub fn from_extension(path: impl AsRef<Path>) -> Option<Self> {
        let ext = path
            .as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())?;

        match ext.as_str() {
            "glb" | "gltf" => Some(Format::Gltf),
            "usda" => Some(Format::Usda),
            "usdz" => Some(Format::Usdz),
            "stl" => Some(Format::Stl),
            "obj" => Some(Format::Obj),
            _ => None,
        }
    }

    /// Nombre legible del formato.
    pub fn name(self) -> &'static str {
        match self {
            Format::Gltf => "glTF/GLB",
            Format::Usda => "USDA",
            Format::Usdz => "USDZ",
            Format::Stl => "STL",
            Format::Obj => "OBJ",
        }
    }

    /// Extensiones de archivo reconocidas para este formato.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Gltf => &["glb", "gltf"],
            Format::Usda => &["usda"],
            Format::Usdz => &["usdz"],
            Format::Stl => &["stl"],
            Format::Obj => &["obj"],
        }
    }

    /// Descripción breve del formato.
    pub fn description(self) -> &'static str {
        match self {
            Format::Gltf => "GL Transmission Format",
            Format::Usda => "Universal Scene Description (texto)",
            Format::Usdz => "Universal Scene Description (paquete AR)",
            Format::Stl => "Stereolithography",
            Format::Obj => "Wavefront OBJ",
        }
    }

    /// Indica si el formato soporta importación.
    pub fn can_import(self) -> bool {
        matches!(self, Format::Gltf | Format::Stl | Format::Obj)
    }

    /// Indica si el formato soporta exportación.
    pub fn can_export(self) -> bool {
        matches!(self, Format::Gltf | Format::Usda | Format::Usdz | Format::Stl | Format::Obj)
    }

    /// Indica si el formato soporta operaciones en memoria (bytes).
    pub fn can_import_bytes(self) -> bool {
        matches!(self, Format::Gltf | Format::Stl)
    }

    /// Indica si el formato soporta exportación a bytes en memoria.
    pub fn can_export_bytes(self) -> bool {
        matches!(self, Format::Gltf | Format::Usda | Format::Usdz | Format::Stl)
    }

    /// Todos los formatos disponibles.
    pub fn all() -> &'static [Format] {
        &ALL_FORMATS
    }

    /// Formatos que soportan importación.
    pub fn importable() -> impl Iterator<Item = Format> {
        ALL_FORMATS.into_iter().filter(|f| f.can_import())
    }

    /// Formatos que soportan exportación.
    pub fn exportable() -> impl Iterator<Item = Format> {
        ALL_FORMATS.into_iter().filter(|f| f.can_export())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_formats() {
        assert_eq!(Format::from_extension("model.glb"), Some(Format::Gltf));
        assert_eq!(Format::from_extension("model.gltf"), Some(Format::Gltf));
        assert_eq!(Format::from_extension("scene.usda"), Some(Format::Usda));
        assert_eq!(Format::from_extension("scene.usdz"), Some(Format::Usdz));
        assert_eq!(Format::from_extension("mesh.stl"), Some(Format::Stl));
        assert_eq!(Format::from_extension("mesh.obj"), Some(Format::Obj));
        assert_eq!(Format::from_extension("unknown.fbx"), None);
        assert_eq!(Format::from_extension("no_extension"), None);
    }

    #[test]
    fn format_extensions() {
        assert_eq!(Format::Gltf.extensions(), &["glb", "gltf"]);
        assert_eq!(Format::Stl.extensions(), &["stl"]);
        assert_eq!(Format::Usdz.extensions(), &["usdz"]);
    }

    #[test]
    fn format_enumeration() {
        let importable: Vec<Format> = Format::importable().collect();
        assert_eq!(importable, vec![Format::Gltf, Format::Stl, Format::Obj]);

        let exportable: Vec<Format> = Format::exportable().collect();
        assert_eq!(exportable, vec![Format::Gltf, Format::Stl, Format::Obj, Format::Usda, Format::Usdz]);

        assert_eq!(Format::all().len(), 5);
    }
}
