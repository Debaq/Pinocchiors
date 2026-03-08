//! Pinocchio Graph - Algoritmos de grafos
//!
//! Este crate proporciona:
//! - PtGraph: wrapper sobre petgraph
//! - ShortestPather: Dijkstra single-source
//! - AllShortestPather: all-pairs shortest paths

mod pt_graph;
mod shortest_path;

pub use pt_graph::PtGraph;
pub use shortest_path::{ShortestPather, AllShortestPather};

pub use petgraph;
