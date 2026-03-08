//! Wrapper sobre petgraph para grafos ponderados

use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use pinocchio_math::Real;
use std::collections::HashMap;

/// Grafo ponderado no dirigido
pub struct PtGraph<N, E> {
    graph: UnGraph<N, E>,
    node_map: HashMap<usize, NodeIndex>,
}

impl<N: Clone, E: Clone> PtGraph<N, E> {
    /// Crea un grafo vacío
    pub fn new() -> Self {
        Self {
            graph: UnGraph::new_undirected(),
            node_map: HashMap::new(),
        }
    }

    /// Añade un nodo con un ID externo
    pub fn add_node(&mut self, id: usize, data: N) -> NodeIndex {
        let idx = self.graph.add_node(data);
        self.node_map.insert(id, idx);
        idx
    }

    /// Obtiene el índice interno de un nodo por su ID externo
    pub fn get_node_index(&self, id: usize) -> Option<NodeIndex> {
        self.node_map.get(&id).copied()
    }

    /// Añade una arista entre dos nodos (por ID externo)
    pub fn add_edge(&mut self, from: usize, to: usize, weight: E) -> bool {
        if let (Some(&from_idx), Some(&to_idx)) = (
            self.node_map.get(&from),
            self.node_map.get(&to),
        ) {
            self.graph.add_edge(from_idx, to_idx, weight);
            true
        } else {
            false
        }
    }

    /// Número de nodos
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Número de aristas
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Obtiene los datos de un nodo
    pub fn get_node(&self, id: usize) -> Option<&N> {
        self.node_map.get(&id)
            .and_then(|&idx| self.graph.node_weight(idx))
    }

    /// Obtiene los vecinos de un nodo
    pub fn neighbors(&self, id: usize) -> Vec<usize> {
        if let Some(&idx) = self.node_map.get(&id) {
            self.graph
                .neighbors(idx)
                .filter_map(|neighbor_idx| {
                    self.node_map
                        .iter()
                        .find(|&(_, v)| *v == neighbor_idx)
                        .map(|(&k, _)| k)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Acceso al grafo interno de petgraph
    pub fn inner(&self) -> &UnGraph<N, E> {
        &self.graph
    }

    /// Acceso mutable al grafo interno
    pub fn inner_mut(&mut self) -> &mut UnGraph<N, E> {
        &mut self.graph
    }
}

impl<N: Clone, E: Clone> Default for PtGraph<N, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: Clone> PtGraph<N, Real> {
    /// Obtiene el peso de una arista entre dos nodos
    pub fn get_edge_weight(&self, from: usize, to: usize) -> Option<Real> {
        if let (Some(&from_idx), Some(&to_idx)) = (
            self.node_map.get(&from),
            self.node_map.get(&to),
        ) {
            self.graph
                .edges(from_idx)
                .find(|e| e.target() == to_idx)
                .map(|e| *e.weight())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_creation() {
        let mut graph: PtGraph<&str, f64> = PtGraph::new();

        graph.add_node(0, "A");
        graph.add_node(1, "B");
        graph.add_node(2, "C");

        graph.add_edge(0, 1, 1.0);
        graph.add_edge(1, 2, 2.0);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_neighbors() {
        let mut graph: PtGraph<i32, f64> = PtGraph::new();

        graph.add_node(0, 0);
        graph.add_node(1, 1);
        graph.add_node(2, 2);

        graph.add_edge(0, 1, 1.0);
        graph.add_edge(0, 2, 1.0);

        let neighbors = graph.neighbors(0);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&1));
        assert!(neighbors.contains(&2));
    }

    #[test]
    fn test_edge_weight() {
        let mut graph: PtGraph<i32, f64> = PtGraph::new();

        graph.add_node(0, 0);
        graph.add_node(1, 1);
        graph.add_edge(0, 1, 3.14);

        assert_eq!(graph.get_edge_weight(0, 1), Some(3.14));
        assert_eq!(graph.get_edge_weight(1, 0), Some(3.14)); // No dirigido
        assert_eq!(graph.get_edge_weight(0, 2), None);
    }
}
