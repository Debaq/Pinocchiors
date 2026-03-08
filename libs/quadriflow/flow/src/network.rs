//! Flow network construction from mesh dual graph.
//!
//! QuadriFlow formulates singularity minimization as a min-cost flow problem.
//! The network is constructed from the mesh dual graph where:
//! - Nodes correspond to mesh vertices
//! - Arcs correspond to mesh edges (bidirectional)
//! - Supply/demand is based on singularity indices
//! - Costs reflect the difficulty of moving singularities

use pinocchio_mesh::Mesh;
use quadriflow_field::SingularityInfo;
use std::collections::HashMap;

/// A node in the flow network.
#[derive(Debug, Clone)]
pub struct FlowNode {
    /// Node identifier
    pub id: usize,
    /// Supply (positive) or demand (negative)
    /// Multiplied by 4 to work with integers (singularity index is ±1/4)
    pub supply: i32,
    /// Original vertex index in mesh
    pub vertex: usize,
}

/// An arc in the flow network.
#[derive(Debug, Clone)]
pub struct FlowArc {
    /// Source node
    pub from: usize,
    /// Target node
    pub to: usize,
    /// Cost per unit flow (based on edge length and curvature)
    pub cost: i32,
    /// Minimum flow capacity (usually 0)
    pub lower: i32,
    /// Maximum flow capacity
    pub upper: i32,
    /// Original edge index in mesh (if applicable)
    pub edge: Option<usize>,
}

/// Min-cost flow network.
#[derive(Debug, Clone)]
pub struct FlowNetwork {
    /// Nodes with supply/demand
    pub nodes: Vec<FlowNode>,
    /// Arcs with costs and capacities
    pub arcs: Vec<FlowArc>,
    /// Mapping from mesh vertex to node index
    pub vertex_to_node: Vec<usize>,
}

impl FlowNetwork {
    /// Create an empty flow network.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            arcs: Vec::new(),
            vertex_to_node: Vec::new(),
        }
    }

    /// Add a node with given supply (negative for demand).
    pub fn add_node(&mut self, supply: i32, vertex: usize) -> usize {
        let id = self.nodes.len();
        self.nodes.push(FlowNode { id, supply, vertex });
        id
    }

    /// Add an arc between nodes.
    pub fn add_arc(
        &mut self,
        from: usize,
        to: usize,
        cost: i32,
        lower: i32,
        upper: i32,
        edge: Option<usize>,
    ) {
        self.arcs.push(FlowArc {
            from,
            to,
            cost,
            lower,
            upper,
            edge,
        });
    }

    /// Number of nodes.
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Number of arcs.
    pub fn num_arcs(&self) -> usize {
        self.arcs.len()
    }

    /// Total supply in the network (should be 0 for feasible problem).
    pub fn total_supply(&self) -> i32 {
        self.nodes.iter().map(|n| n.supply).sum()
    }

    /// Build a flow network from a mesh and its singularity information.
    ///
    /// # Arguments
    /// * `mesh` - The triangle mesh
    /// * `singularity_info` - Detected singularities and vertex indices
    /// * `cost_scale` - Scaling factor for costs (default 1000)
    ///
    /// # Returns
    /// A flow network ready for solving
    pub fn from_mesh(
        mesh: &Mesh,
        singularity_info: &SingularityInfo,
        cost_scale: i32,
    ) -> Self {
        let mut network = FlowNetwork::new();
        let num_vertices = mesh.num_vertices();

        // Create nodes for each vertex
        // Supply = singularity index * 4 (to work with integers)
        network.vertex_to_node = Vec::with_capacity(num_vertices);

        for v in 0..num_vertices {
            // Convert singularity index to integer supply
            // Index ±0.25 -> supply ±1
            let index = singularity_info.vertex_indices[v];
            let supply = (index * 4.0).round() as i32;

            let node_id = network.add_node(supply, v);
            network.vertex_to_node.push(node_id);
        }

        // Create arcs for each edge (both directions)
        // We iterate over half-edges and only add one arc per undirected edge
        let mut added_edges: HashMap<(usize, usize), bool> = HashMap::new();

        for edge_idx in 0..mesh.edges.len() {
            let edge = &mesh.edges[edge_idx];
            let v1 = edge.vertex;
            let v0 = mesh.get_edge_origin(edge_idx);

            // Canonical edge representation (smaller index first)
            let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };

            if added_edges.contains_key(&key) {
                continue;
            }
            added_edges.insert(key, true);

            // Compute edge cost based on length
            let p0 = mesh.vertices[v0].position;
            let p1 = mesh.vertices[v1].position;
            let length = (p1 - p0).length();

            // Cost is proportional to edge length
            // Shorter edges are cheaper to route flow through
            let cost = ((length * cost_scale as f64) as i32).max(1);

            let n0 = network.vertex_to_node[v0];
            let n1 = network.vertex_to_node[v1];

            // Add arcs in both directions (undirected graph)
            // Capacity is effectively unlimited for singularity flow
            let capacity = 100; // Allow multiple singularities to flow

            network.add_arc(n0, n1, cost, 0, capacity, Some(edge_idx));
            network.add_arc(n1, n0, cost, 0, capacity, Some(edge_idx));
        }

        network
    }

    /// Check if the network is balanced (total supply = 0).
    pub fn is_balanced(&self) -> bool {
        self.total_supply() == 0
    }

    /// Balance the network by adding a super source/sink if needed.
    ///
    /// Returns the indices of added super nodes (if any).
    pub fn balance(&mut self) -> (Option<usize>, Option<usize>) {
        let total = self.total_supply();

        if total == 0 {
            return (None, None);
        }

        let mut super_source = None;
        let mut super_sink = None;

        if total > 0 {
            // Excess supply - add super sink
            let sink_id = self.add_node(-total, usize::MAX);
            super_sink = Some(sink_id);

            // Connect all supply nodes to super sink
            for node in 0..self.nodes.len() - 1 {
                if self.nodes[node].supply > 0 {
                    self.add_arc(node, sink_id, 0, 0, self.nodes[node].supply, None);
                }
            }
        } else {
            // Excess demand - add super source
            let source_id = self.add_node(-total, usize::MAX);
            super_source = Some(source_id);

            // Connect super source to all demand nodes
            for node in 0..self.nodes.len() - 1 {
                if self.nodes[node].supply < 0 {
                    self.add_arc(source_id, node, 0, 0, -self.nodes[node].supply, None);
                }
            }
        }

        (super_source, super_sink)
    }
}

impl Default for FlowNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_tetrahedron() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 0.866, 0.0),
            Vector3::new(0.5, 0.289, 0.816),
        ];
        let faces = vec![[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];
        Mesh::from_triangles(&vertices, &faces)
    }

    fn make_mock_singularity_info(num_vertices: usize) -> SingularityInfo {
        SingularityInfo {
            singularities: Vec::new(),
            vertex_indices: vec![0.0; num_vertices],
        }
    }

    #[test]
    fn test_network_from_mesh() {
        let mesh = make_tetrahedron();
        let sing_info = make_mock_singularity_info(mesh.num_vertices());
        let network = FlowNetwork::from_mesh(&mesh, &sing_info, 1000);

        // 4 vertices -> 4 nodes
        assert_eq!(network.num_nodes(), 4);

        // 6 edges * 2 directions = 12 arcs
        assert_eq!(network.num_arcs(), 12);

        // With all zero singularities, total supply should be 0
        assert_eq!(network.total_supply(), 0);
        assert!(network.is_balanced());
    }

    #[test]
    fn test_network_with_singularities() {
        let mesh = make_tetrahedron();
        let mut sing_info = make_mock_singularity_info(mesh.num_vertices());

        // Add a positive singularity at vertex 0 and negative at vertex 1
        sing_info.vertex_indices[0] = 0.25;  // +1/4 -> supply +1
        sing_info.vertex_indices[1] = -0.25; // -1/4 -> supply -1

        let network = FlowNetwork::from_mesh(&mesh, &sing_info, 1000);

        assert_eq!(network.nodes[0].supply, 1);
        assert_eq!(network.nodes[1].supply, -1);
        assert_eq!(network.total_supply(), 0);
    }

    #[test]
    fn test_network_balance() {
        let mesh = make_tetrahedron();
        let mut sing_info = make_mock_singularity_info(mesh.num_vertices());

        // Unbalanced: only positive singularity
        sing_info.vertex_indices[0] = 0.25;

        let mut network = FlowNetwork::from_mesh(&mesh, &sing_info, 1000);
        assert!(!network.is_balanced());

        let (source, sink) = network.balance();
        assert!(source.is_none());
        assert!(sink.is_some());
        assert!(network.is_balanced());
    }

    #[test]
    fn test_arc_costs_proportional_to_length() {
        let mesh = make_tetrahedron();
        let sing_info = make_mock_singularity_info(mesh.num_vertices());
        let network = FlowNetwork::from_mesh(&mesh, &sing_info, 1000);

        // All arcs should have positive costs
        for arc in &network.arcs {
            assert!(arc.cost > 0);
        }
    }
}
