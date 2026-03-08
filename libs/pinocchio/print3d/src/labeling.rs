//! Sistema de etiquetado y tracking de piezas subdivididas

use pinocchio_math::Real;
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Una pieza etiquetada resultado de la subdivisión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledPiece {
    /// Malla de la pieza
    #[serde(skip)]
    pub mesh: Mesh,

    /// Identificador único de la pieza
    pub id: usize,

    /// Etiqueta legible (ej: "1A", "2B")
    pub label: String,

    /// Posición original en el modelo completo [x, y, z]
    pub original_position: [Real; 3],

    /// IDs de piezas vecinas (comparten cara de corte)
    pub neighbors: Vec<usize>,

    /// Índices de caras que son resultado del corte
    pub cut_faces: Vec<usize>,
}

impl Default for LabeledPiece {
    fn default() -> Self {
        Self {
            mesh: Mesh::new(),
            id: 0,
            label: String::new(),
            original_position: [0.0, 0.0, 0.0],
            neighbors: Vec::new(),
            cut_faces: Vec::new(),
        }
    }
}

/// Información de ensamblaje para un conjunto de piezas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyInfo {
    /// Número total de piezas
    pub piece_count: usize,

    /// Mapa de vecindad: pieza_id -> [vecino_ids]
    pub adjacency: HashMap<usize, Vec<usize>>,

    /// Orden sugerido de ensamblaje (IDs de piezas)
    pub assembly_order: Vec<usize>,

    /// Dimensiones del modelo original [x, y, z]
    pub original_dimensions: [Real; 3],

    /// Volumen total del modelo
    pub total_volume: Real,
}

impl AssemblyInfo {
    /// Crea información de ensamblaje desde piezas etiquetadas
    pub fn from_pieces(pieces: &[LabeledPiece], original_dimensions: [Real; 3]) -> Self {
        let mut adjacency = HashMap::new();

        for piece in pieces {
            adjacency.insert(piece.id, piece.neighbors.clone());
        }

        // Orden de ensamblaje: por ahora simplemente por ID
        // TODO: Implementar orden topológico óptimo
        let assembly_order: Vec<usize> = pieces.iter().map(|p| p.id).collect();

        Self {
            piece_count: pieces.len(),
            adjacency,
            assembly_order,
            original_dimensions,
            total_volume: 0.0, // TODO: Calcular
        }
    }

    /// Exporta la información a JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Importa información desde JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Detecta qué piezas son vecinas basándose en proximidad de caras de corte
///
/// Dos piezas son vecinas si comparten una cara de corte (dentro de tolerancia)
pub fn find_neighbors(pieces: &mut [LabeledPiece], _tolerance: Real) {
    // TODO: Implementar detección de vecinos
    //
    // Algoritmo:
    // 1. Para cada pieza, obtener las caras de corte
    // 2. Para cada par de piezas, verificar si alguna cara de corte coincide
    // 3. Dos caras coinciden si sus vértices están dentro de tolerancia
    // 4. Marcar como vecinas si comparten cara

    // Por ahora, marcamos como vecinas las piezas consecutivas
    for i in 0..pieces.len() {
        let mut neighbors = Vec::new();
        if i > 0 {
            neighbors.push(i - 1);
        }
        if i + 1 < pieces.len() {
            neighbors.push(i + 1);
        }
        pieces[i].neighbors = neighbors;
    }
}

/// Información de una conexión entre dos piezas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceConnection {
    /// ID de la primera pieza
    pub piece_a: usize,
    /// ID de la segunda pieza
    pub piece_b: usize,
    /// Área de la superficie de contacto (mm²)
    pub contact_area: Real,
    /// Normal promedio de la superficie de contacto [x, y, z]
    pub contact_normal: [Real; 3],
    /// Centro de la superficie de contacto [x, y, z]
    pub contact_center: [Real; 3],
}

/// Calcula las conexiones entre piezas
pub fn calculate_connections(pieces: &[LabeledPiece]) -> Vec<PieceConnection> {
    let mut connections = Vec::new();

    for piece in pieces {
        for &neighbor_id in &piece.neighbors {
            // Evitar duplicados (solo añadir si piece.id < neighbor_id)
            if piece.id < neighbor_id {
                // TODO: Calcular área y posición real del contacto
                connections.push(PieceConnection {
                    piece_a: piece.id,
                    piece_b: neighbor_id,
                    contact_area: 0.0,
                    contact_normal: [0.0, 0.0, 0.0],
                    contact_center: [0.0, 0.0, 0.0],
                });
            }
        }
    }

    connections
}

/// Genera un informe de ensamblaje legible
pub fn generate_assembly_report(pieces: &[LabeledPiece], info: &AssemblyInfo) -> String {
    let mut report = String::new();

    report.push_str("=== INFORME DE ENSAMBLAJE ===\n\n");
    report.push_str(&format!("Número de piezas: {}\n", info.piece_count));
    report.push_str(&format!(
        "Dimensiones originales: {:.1} x {:.1} x {:.1} mm\n\n",
        info.original_dimensions[0], info.original_dimensions[1], info.original_dimensions[2]
    ));

    report.push_str("LISTA DE PIEZAS:\n");
    for piece in pieces {
        report.push_str(&format!(
            "  {} (ID: {}) - Vecinas: {:?}\n",
            piece.label, piece.id, piece.neighbors
        ));
    }

    report.push_str("\nORDEN DE ENSAMBLAJE SUGERIDO:\n");
    for (step, &id) in info.assembly_order.iter().enumerate() {
        if let Some(piece) = pieces.iter().find(|p| p.id == id) {
            report.push_str(&format!("  {}. Pieza {}\n", step + 1, piece.label));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labeled_piece_default() {
        let piece = LabeledPiece::default();
        assert_eq!(piece.id, 0);
        assert!(piece.label.is_empty());
        assert!(piece.neighbors.is_empty());
    }

    #[test]
    fn test_assembly_info_json_roundtrip() {
        let info = AssemblyInfo {
            piece_count: 4,
            adjacency: [(0, vec![1]), (1, vec![0, 2]), (2, vec![1, 3]), (3, vec![2])]
                .into_iter()
                .collect(),
            assembly_order: vec![0, 1, 2, 3],
            original_dimensions: [100.0, 100.0, 200.0],
            total_volume: 2_000_000.0,
        };

        let json = info.to_json().unwrap();
        let parsed = AssemblyInfo::from_json(&json).unwrap();

        assert_eq!(parsed.piece_count, 4);
        assert_eq!(parsed.assembly_order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_find_neighbors() {
        let mut pieces: Vec<LabeledPiece> = (0..3)
            .map(|i| LabeledPiece {
                id: i,
                ..Default::default()
            })
            .collect();

        find_neighbors(&mut pieces, 0.1);

        assert_eq!(pieces[0].neighbors, vec![1]);
        assert_eq!(pieces[1].neighbors, vec![0, 2]);
        assert_eq!(pieces[2].neighbors, vec![1]);
    }

    #[test]
    fn test_calculate_connections() {
        let pieces = vec![
            LabeledPiece {
                id: 0,
                neighbors: vec![1],
                ..Default::default()
            },
            LabeledPiece {
                id: 1,
                neighbors: vec![0, 2],
                ..Default::default()
            },
            LabeledPiece {
                id: 2,
                neighbors: vec![1],
                ..Default::default()
            },
        ];

        let connections = calculate_connections(&pieces);

        // Solo 2 conexiones únicas: 0-1 y 1-2
        assert_eq!(connections.len(), 2);
        assert_eq!(connections[0].piece_a, 0);
        assert_eq!(connections[0].piece_b, 1);
        assert_eq!(connections[1].piece_a, 1);
        assert_eq!(connections[1].piece_b, 2);
    }
}
