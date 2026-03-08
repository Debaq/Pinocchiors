//! Flow solvers for min-cost network flow.
//!
//! Implements the Successive Shortest Paths (SSP) algorithm for solving
//! minimum-cost flow problems. This is simpler than Network Simplex but
//! still produces optimal solutions.

use crate::network::FlowNetwork;
use crate::FlowError;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

/// Result of solving a flow network.
#[derive(Debug, Clone)]
pub struct FlowResult {
    /// Flow value on each arc (same order as network.arcs)
    pub flows: Vec<i32>,
    /// Total cost of the flow
    pub total_cost: i64,
    /// Residual supplies after flow (should all be 0 if feasible)
    pub residual_supply: Vec<i32>,
}

impl FlowResult {
    /// Check if the flow satisfies all supply/demand constraints.
    pub fn is_feasible(&self) -> bool {
        self.residual_supply.iter().all(|&s| s == 0)
    }

    /// Get the flow on a specific arc.
    pub fn flow(&self, arc_idx: usize) -> i32 {
        self.flows.get(arc_idx).copied().unwrap_or(0)
    }
}

/// Trait for min-cost flow solvers.
pub trait FlowSolver {
    /// Solve the min-cost flow problem.
    fn solve(&self, network: &FlowNetwork) -> Result<FlowResult, FlowError>;
}

/// Successive Shortest Paths algorithm.
///
/// This algorithm repeatedly finds shortest paths from supply nodes to
/// demand nodes and pushes flow along them until all supply is exhausted.
///
/// Time complexity: O(n * m * log(n) * U) where U is max supply
#[derive(Debug, Clone, Default)]
pub struct SuccessiveShortestPaths {
    /// Maximum iterations before giving up
    pub max_iterations: usize,
}

impl SuccessiveShortestPaths {
    /// Create a new SSP solver with default settings.
    pub fn new() -> Self {
        Self {
            max_iterations: 100_000,
        }
    }

    /// Create with custom max iterations.
    pub fn with_max_iterations(max_iterations: usize) -> Self {
        Self { max_iterations }
    }
}

/// State for Dijkstra's algorithm with potential adjustments.
#[derive(Debug, Clone, Eq, PartialEq)]
struct DijkstraState {
    cost: i64,
    node: usize,
}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other.cost.cmp(&self.cost)
            .then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FlowSolver for SuccessiveShortestPaths {
    fn solve(&self, network: &FlowNetwork) -> Result<FlowResult, FlowError> {
        let n = network.num_nodes();
        let m = network.num_arcs();

        if n == 0 {
            return Ok(FlowResult {
                flows: Vec::new(),
                total_cost: 0,
                residual_supply: Vec::new(),
            });
        }

        // Check if network is balanced
        if !network.is_balanced() {
            return Err(FlowError::Infeasible);
        }

        // Initialize flows to 0
        let mut flows = vec![0i32; m];

        // Residual capacities (forward and backward)
        // For arc i: residual_cap[i] = upper - flow, residual_cap[m + i] = flow - lower
        let mut residual_cap: Vec<i32> = Vec::with_capacity(2 * m);
        for arc in &network.arcs {
            residual_cap.push(arc.upper - arc.lower);
        }
        for _arc in &network.arcs {
            residual_cap.push(0); // Initially no backward capacity
        }

        // Build adjacency list for residual graph
        // Each arc has a forward (original) and backward (reverse) version
        let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for (i, arc) in network.arcs.iter().enumerate() {
            adj[arc.from].push((arc.to, i));         // Forward arc
            adj[arc.to].push((arc.from, m + i));     // Backward arc
        }

        // Track remaining supply/demand
        let mut supply: Vec<i32> = network.nodes.iter().map(|n| n.supply).collect();

        // Node potentials for reduced costs (Johnson's algorithm style)
        let mut potential: Vec<i64> = vec![0; n];

        let mut total_cost: i64 = 0;
        let mut iterations = 0;

        // Main loop: while there's supply to push
        while iterations < self.max_iterations {
            // Find a source (positive supply) and sink (negative supply)
            let source = supply.iter().position(|&s| s > 0);
            let sink = supply.iter().position(|&s| s < 0);

            let (source, sink) = match (source, sink) {
                (Some(s), Some(t)) => (s, t),
                _ => break, // No more flow to push
            };

            // Find shortest path from source to sink in residual graph
            // Using Dijkstra with reduced costs
            let path = self.find_shortest_path(
                source,
                sink,
                &adj,
                &residual_cap,
                &network.arcs,
                &potential,
                n,
                m,
            );

            let (path_arcs, path_cost, dist) = match path {
                Some(p) => p,
                None => {
                    // No path exists - infeasible
                    return Err(FlowError::Infeasible);
                }
            };

            // Update potentials
            for i in 0..n {
                if dist[i] < i64::MAX / 2 {
                    potential[i] += dist[i];
                }
            }

            // Find bottleneck capacity along path
            let mut bottleneck = supply[source].min(-supply[sink]);
            for &arc_idx in &path_arcs {
                bottleneck = bottleneck.min(residual_cap[arc_idx]);
            }

            if bottleneck <= 0 {
                iterations += 1;
                continue;
            }

            // Push flow along path
            for &arc_idx in &path_arcs {
                if arc_idx < m {
                    // Forward arc
                    flows[arc_idx] += bottleneck;
                    residual_cap[arc_idx] -= bottleneck;
                    residual_cap[m + arc_idx] += bottleneck;
                } else {
                    // Backward arc
                    let orig_idx = arc_idx - m;
                    flows[orig_idx] -= bottleneck;
                    residual_cap[orig_idx] += bottleneck;
                    residual_cap[arc_idx] -= bottleneck;
                }
            }

            // Update supplies
            supply[source] -= bottleneck;
            supply[sink] += bottleneck;
            total_cost += path_cost * bottleneck as i64;

            iterations += 1;
        }

        if iterations >= self.max_iterations {
            return Err(FlowError::SolverError("Max iterations exceeded".into()));
        }

        Ok(FlowResult {
            flows,
            total_cost,
            residual_supply: supply,
        })
    }
}

impl SuccessiveShortestPaths {
    /// Find shortest path using Dijkstra with reduced costs.
    fn find_shortest_path(
        &self,
        source: usize,
        sink: usize,
        adj: &[Vec<(usize, usize)>],
        residual_cap: &[i32],
        arcs: &[crate::network::FlowArc],
        potential: &[i64],
        n: usize,
        m: usize,
    ) -> Option<(Vec<usize>, i64, Vec<i64>)> {
        let mut dist = vec![i64::MAX / 2; n];
        let mut parent: Vec<Option<(usize, usize)>> = vec![None; n]; // (prev_node, arc_idx)
        let mut heap = BinaryHeap::new();

        dist[source] = 0;
        heap.push(DijkstraState { cost: 0, node: source });

        while let Some(DijkstraState { cost, node }) = heap.pop() {
            if cost > dist[node] {
                continue;
            }

            if node == sink {
                break;
            }

            for &(next, arc_idx) in &adj[node] {
                if residual_cap[arc_idx] <= 0 {
                    continue;
                }

                // Calculate reduced cost
                let arc_cost = if arc_idx < m {
                    arcs[arc_idx].cost as i64
                } else {
                    -(arcs[arc_idx - m].cost as i64)
                };

                let reduced_cost = arc_cost + potential[node] - potential[next];
                let new_dist = dist[node] + reduced_cost;

                if new_dist < dist[next] {
                    dist[next] = new_dist;
                    parent[next] = Some((node, arc_idx));
                    heap.push(DijkstraState { cost: new_dist, node: next });
                }
            }
        }

        if parent[sink].is_none() && source != sink {
            return None;
        }

        // Reconstruct path
        let mut path_arcs = Vec::new();
        let mut current = sink;
        let mut path_cost: i64 = 0;

        while current != source {
            if let Some((prev, arc_idx)) = parent[current] {
                path_arcs.push(arc_idx);
                let arc_cost = if arc_idx < m {
                    arcs[arc_idx].cost as i64
                } else {
                    -(arcs[arc_idx - m].cost as i64)
                };
                path_cost += arc_cost;
                current = prev;
            } else {
                return None;
            }
        }

        path_arcs.reverse();
        Some((path_arcs, path_cost, dist))
    }
}

/// Network simplex algorithm (exact).
///
/// This is a more sophisticated algorithm that can be faster for large networks.
/// Currently delegates to SSP.
#[derive(Debug, Clone, Default)]
pub struct NetworkSimplex;

impl FlowSolver for NetworkSimplex {
    fn solve(&self, network: &FlowNetwork) -> Result<FlowResult, FlowError> {
        // For now, use SSP as fallback
        // A full Network Simplex implementation would be more efficient
        // but SSP is correct and sufficient for moderate-sized meshes
        SuccessiveShortestPaths::new().solve(network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::FlowNetwork;

    fn make_simple_network() -> FlowNetwork {
        // Simple network:
        //   (0) --1--> (1) --1--> (2)
        // Supply: 0=+1, 2=-1
        let mut network = FlowNetwork::new();
        network.add_node(1, 0);   // source with supply 1
        network.add_node(0, 1);   // transit
        network.add_node(-1, 2);  // sink with demand 1

        network.add_arc(0, 1, 1, 0, 10, None);
        network.add_arc(1, 2, 1, 0, 10, None);

        network
    }

    fn make_parallel_paths_network() -> FlowNetwork {
        // Network with two parallel paths:
        //       ---(1)---
        //      /   c=1   \
        //   (0)           (3)
        //      \   c=2   /
        //       ---(2)---
        // Supply: 0=+1, 3=-1
        let mut network = FlowNetwork::new();
        network.add_node(1, 0);
        network.add_node(0, 1);
        network.add_node(0, 2);
        network.add_node(-1, 3);

        network.add_arc(0, 1, 1, 0, 10, None); // cheap path
        network.add_arc(1, 3, 1, 0, 10, None);
        network.add_arc(0, 2, 2, 0, 10, None); // expensive path
        network.add_arc(2, 3, 2, 0, 10, None);

        network
    }

    #[test]
    fn test_ssp_simple_network() {
        let network = make_simple_network();
        let solver = SuccessiveShortestPaths::new();
        let result = solver.solve(&network).unwrap();

        assert!(result.is_feasible());
        assert_eq!(result.total_cost, 2); // Cost 1 + 1
        assert_eq!(result.flows[0], 1);   // Flow on first arc
        assert_eq!(result.flows[1], 1);   // Flow on second arc
    }

    #[test]
    fn test_ssp_chooses_cheaper_path() {
        let network = make_parallel_paths_network();
        let solver = SuccessiveShortestPaths::new();
        let result = solver.solve(&network).unwrap();

        assert!(result.is_feasible());
        assert_eq!(result.total_cost, 2); // Should use cheap path (1+1=2, not 2+2=4)
        assert_eq!(result.flows[0], 1);   // Flow on cheap path
        assert_eq!(result.flows[2], 0);   // No flow on expensive path
    }

    #[test]
    fn test_ssp_infeasible() {
        let mut network = FlowNetwork::new();
        network.add_node(1, 0);  // supply
        network.add_node(0, 1);  // no demand!

        // No connection to demand node
        let solver = SuccessiveShortestPaths::new();
        let result = solver.solve(&network);

        assert!(result.is_err());
    }

    #[test]
    fn test_ssp_empty_network() {
        let network = FlowNetwork::new();
        let solver = SuccessiveShortestPaths::new();
        let result = solver.solve(&network).unwrap();

        assert!(result.is_feasible());
        assert_eq!(result.total_cost, 0);
    }

    #[test]
    fn test_ssp_balanced_no_flow_needed() {
        let mut network = FlowNetwork::new();
        network.add_node(0, 0);
        network.add_node(0, 1);
        network.add_arc(0, 1, 10, 0, 10, None);

        let solver = SuccessiveShortestPaths::new();
        let result = solver.solve(&network).unwrap();

        assert!(result.is_feasible());
        assert_eq!(result.total_cost, 0);
        assert_eq!(result.flows[0], 0);
    }

    #[test]
    fn test_network_simplex_delegates_to_ssp() {
        let network = make_simple_network();
        let solver = NetworkSimplex;
        let result = solver.solve(&network).unwrap();

        assert!(result.is_feasible());
        assert_eq!(result.total_cost, 2);
    }
}
