//! Algoritmos de caminos mínimos

use crate::PtGraph;
use petgraph::algo::dijkstra;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use pinocchio_math::Real;
use std::collections::HashMap;

/// Resultado de Dijkstra desde una fuente
pub struct ShortestPather {
    /// Distancias desde la fuente
    distances: HashMap<NodeIndex, Real>,
    /// Predecesores para reconstruir caminos
    predecessors: HashMap<NodeIndex, NodeIndex>,
    /// Nodo fuente
    source: NodeIndex,
}

impl ShortestPather {
    /// Calcula los caminos mínimos desde un nodo fuente
    pub fn from_source<N: Clone>(graph: &PtGraph<N, Real>, source_id: usize) -> Option<Self> {
        let source = graph.get_node_index(source_id)?;

        let result = dijkstra(
            graph.inner(),
            source,
            None,
            |e| *e.weight(),
        );

        // Convertir el resultado
        let distances: HashMap<NodeIndex, Real> = result.into_iter().collect();

        // TODO: Reconstruir predecesores (requiere modificar dijkstra)
        let predecessors = HashMap::new();

        Some(Self {
            distances,
            predecessors,
            source,
        })
    }

    /// Obtiene la distancia a un nodo destino
    pub fn distance_to(&self, target: NodeIndex) -> Option<Real> {
        self.distances.get(&target).copied()
    }

    /// Obtiene la distancia a un nodo por ID externo
    pub fn distance_to_id<N: Clone>(&self, graph: &PtGraph<N, Real>, target_id: usize) -> Option<Real> {
        let target = graph.get_node_index(target_id)?;
        self.distance_to(target)
    }

    /// Reconstruye el camino a un nodo destino
    pub fn path_to(&self, target: NodeIndex) -> Vec<NodeIndex> {
        let mut path = Vec::new();
        let mut current = target;

        while current != self.source {
            path.push(current);
            if let Some(&pred) = self.predecessors.get(&current) {
                current = pred;
            } else {
                // No hay camino
                return Vec::new();
            }
        }
        path.push(self.source);
        path.reverse();
        path
    }

    /// Verifica si hay un camino al destino
    pub fn has_path_to(&self, target: NodeIndex) -> bool {
        self.distances.contains_key(&target)
    }
}

/// Calcula todos los pares de caminos mínimos
pub struct AllShortestPather {
    /// Matriz de distancias (indexada por IDs externos)
    distances: Vec<Vec<Real>>,
    /// Número de nodos
    n: usize,
}

impl AllShortestPather {
    /// Calcula todos los pares de caminos mínimos usando Floyd-Warshall
    pub fn new<N: Clone>(graph: &PtGraph<N, Real>) -> Self {
        let n = graph.node_count();
        let mut distances = vec![vec![Real::INFINITY; n]; n];

        // Inicializar diagonal
        for i in 0..n {
            distances[i][i] = 0.0;
        }

        // Inicializar con aristas existentes
        for edge in graph.inner().edge_references() {
            let (i, j) = (edge.source().index(), edge.target().index());
            let w = *edge.weight();
            distances[i][j] = w;
            distances[j][i] = w; // Grafo no dirigido
        }

        // Floyd-Warshall
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    let through_k = distances[i][k] + distances[k][j];
                    if through_k < distances[i][j] {
                        distances[i][j] = through_k;
                    }
                }
            }
        }

        Self { distances, n }
    }

    /// Obtiene la distancia entre dos nodos (por índice interno)
    pub fn distance(&self, from: usize, to: usize) -> Real {
        if from < self.n && to < self.n {
            self.distances[from][to]
        } else {
            Real::INFINITY
        }
    }

    /// Verifica si dos nodos están conectados
    pub fn connected(&self, from: usize, to: usize) -> bool {
        self.distance(from, to).is_finite()
    }

    /// Número de nodos
    pub fn node_count(&self) -> usize {
        self.n
    }

    /// Obtiene el diámetro del grafo (máxima distancia mínima)
    pub fn diameter(&self) -> Real {
        let mut max_dist = 0.0_f64;
        for i in 0..self.n {
            for j in 0..self.n {
                let d = self.distances[i][j];
                if d.is_finite() && d > max_dist {
                    max_dist = d;
                }
            }
        }
        max_dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_graph() -> PtGraph<i32, Real> {
        let mut graph = PtGraph::new();

        // Grafo simple: 0 -- 1 -- 2
        //                    |
        //                    3
        graph.add_node(0, 0);
        graph.add_node(1, 1);
        graph.add_node(2, 2);
        graph.add_node(3, 3);

        graph.add_edge(0, 1, 1.0);
        graph.add_edge(1, 2, 2.0);
        graph.add_edge(1, 3, 1.5);

        graph
    }

    #[test]
    fn test_shortest_path_single_source() {
        let graph = make_test_graph();
        let sp = ShortestPather::from_source(&graph, 0).unwrap();

        assert_eq!(sp.distance_to_id(&graph, 0), Some(0.0));
        assert_eq!(sp.distance_to_id(&graph, 1), Some(1.0));
        assert_eq!(sp.distance_to_id(&graph, 2), Some(3.0)); // 0->1->2
        assert_eq!(sp.distance_to_id(&graph, 3), Some(2.5)); // 0->1->3
    }

    #[test]
    fn test_all_pairs_shortest_path() {
        let graph = make_test_graph();
        let apsp = AllShortestPather::new(&graph);

        assert_eq!(apsp.distance(0, 0), 0.0);
        assert_eq!(apsp.distance(0, 1), 1.0);
        assert_eq!(apsp.distance(0, 2), 3.0);
        assert_eq!(apsp.distance(2, 3), 3.5); // 2->1->3

        assert!(apsp.connected(0, 3));
        assert_eq!(apsp.node_count(), 4);
    }

    #[test]
    fn test_diameter() {
        let graph = make_test_graph();
        let apsp = AllShortestPather::new(&graph);

        // Máxima distancia: 0 -> 2 = 3.0 o 2 -> 3 = 3.5
        assert!((apsp.diameter() - 3.5).abs() < 1e-10);
    }
}
