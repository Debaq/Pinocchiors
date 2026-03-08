//! Grafo topológico de esferas mediales

use crate::medial_surface::MedialSphere;
use pinocchio_math::{Real, Vector3};
use pinocchio_spatial::DistanceField;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

/// Grafo topológico de esferas mediales
#[derive(Debug, Clone)]
pub struct SphereGraph {
    /// Esferas del grafo
    spheres: Vec<MedialSphere>,
    /// Aristas: (from, to, weight)
    edges: Vec<(usize, usize, Real)>,
    /// Lista de adyacencia
    adjacency: Vec<Vec<(usize, Real)>>,
}

impl SphereGraph {
    /// Crea un grafo vacío
    pub fn new() -> Self {
        Self {
            spheres: Vec::new(),
            edges: Vec::new(),
            adjacency: Vec::new(),
        }
    }

    /// Construye grafo basado en proximidad adaptativa
    ///
    /// El umbral de conexión se basa en los radios locales y la distancia al borde.
    pub fn from_spheres_proximity(
        spheres: &[MedialSphere],
        field: &DistanceField,
    ) -> Self {
        let n = spheres.len();
        let mut edges = Vec::new();
        let mut adjacency = vec![Vec::new(); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let dist = spheres[i].center.distance(&spheres[j].center);

                // Umbral adaptativo basado en radios
                let avg_radius = (spheres[i].radius + spheres[j].radius) * 0.5;
                let threshold = avg_radius * 3.0;

                if dist < threshold {
                    // Verificar que la conexión está dentro de la malla
                    if is_connection_valid(&spheres[i].center, &spheres[j].center, field) {
                        let weight = dist;
                        edges.push((i, j, weight));
                        adjacency[i].push((j, weight));
                        adjacency[j].push((i, weight));
                    }
                }
            }
        }

        Self {
            spheres: spheres.to_vec(),
            edges,
            adjacency,
        }
    }

    /// Construye grafo conectando k vecinos más cercanos
    pub fn from_spheres_knn(spheres: &[MedialSphere], k: usize) -> Self {
        let n = spheres.len();
        let mut edges = Vec::new();
        let mut adjacency = vec![Vec::new(); n];
        let mut edge_set = std::collections::HashSet::new();

        for i in 0..n {
            // Calcular distancias a todos los demás
            let mut distances: Vec<(usize, Real)> = (0..n)
                .filter(|&j| j != i)
                .map(|j| (j, spheres[i].center.distance(&spheres[j].center)))
                .collect();

            // Ordenar por distancia
            distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

            // Conectar a los k más cercanos
            for (j, dist) in distances.into_iter().take(k) {
                let edge_key = if i < j { (i, j) } else { (j, i) };
                if !edge_set.contains(&edge_key) {
                    edge_set.insert(edge_key);
                    edges.push((i, j, dist));
                    adjacency[i].push((j, dist));
                    adjacency[j].push((i, dist));
                }
            }
        }

        Self {
            spheres: spheres.to_vec(),
            edges,
            adjacency,
        }
    }

    /// Número de esferas en el grafo
    pub fn num_spheres(&self) -> usize {
        self.spheres.len()
    }

    /// Número de aristas
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Obtiene una esfera por índice
    pub fn get_sphere(&self, idx: usize) -> Option<&MedialSphere> {
        self.spheres.get(idx)
    }

    /// Obtiene las esferas
    pub fn spheres(&self) -> &[MedialSphere] {
        &self.spheres
    }

    /// Obtiene las aristas
    pub fn edges(&self) -> &[(usize, usize, Real)] {
        &self.edges
    }

    /// Obtiene los vecinos de una esfera
    pub fn neighbors(&self, idx: usize) -> &[(usize, Real)] {
        if idx < self.adjacency.len() {
            &self.adjacency[idx]
        } else {
            &[]
        }
    }

    /// Encuentra el árbol de expansión mínimo (MST) usando Kruskal
    pub fn minimum_spanning_tree(&self) -> Vec<(usize, usize)> {
        let n = self.spheres.len();
        if n == 0 {
            return Vec::new();
        }

        // Ordenar aristas por peso
        let mut sorted_edges = self.edges.clone();
        sorted_edges.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));

        // Union-Find
        let mut parent: Vec<usize> = (0..n).collect();
        let mut rank = vec![0usize; n];

        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

        fn union(parent: &mut [usize], rank: &mut [usize], x: usize, y: usize) -> bool {
            let px = find(parent, x);
            let py = find(parent, y);

            if px == py {
                return false;
            }

            if rank[px] < rank[py] {
                parent[px] = py;
            } else if rank[px] > rank[py] {
                parent[py] = px;
            } else {
                parent[py] = px;
                rank[px] += 1;
            }

            true
        }

        let mut mst = Vec::new();

        for (from, to, _weight) in sorted_edges {
            if union(&mut parent, &mut rank, from, to) {
                mst.push((from, to));
                if mst.len() == n - 1 {
                    break;
                }
            }
        }

        mst
    }

    /// Encuentra el camino más corto entre dos esferas usando Dijkstra
    pub fn shortest_path(&self, from: usize, to: usize) -> Option<Vec<usize>> {
        if from >= self.spheres.len() || to >= self.spheres.len() {
            return None;
        }

        if from == to {
            return Some(vec![from]);
        }

        let n = self.spheres.len();
        let mut dist = vec![Real::INFINITY; n];
        let mut prev: Vec<Option<usize>> = vec![None; n];
        let mut heap = BinaryHeap::new();

        dist[from] = 0.0;
        heap.push(DijkstraState { cost: 0.0, node: from });

        while let Some(DijkstraState { cost, node }) = heap.pop() {
            if node == to {
                // Reconstruir camino
                let mut path = Vec::new();
                let mut current = Some(to);
                while let Some(c) = current {
                    path.push(c);
                    current = prev[c];
                }
                path.reverse();
                return Some(path);
            }

            if cost > dist[node] {
                continue;
            }

            for &(neighbor, weight) in &self.adjacency[node] {
                let next_cost = cost + weight;
                if next_cost < dist[neighbor] {
                    dist[neighbor] = next_cost;
                    prev[neighbor] = Some(node);
                    heap.push(DijkstraState { cost: next_cost, node: neighbor });
                }
            }
        }

        None
    }

    /// Encuentra la esfera más cercana a una posición
    pub fn find_nearest_sphere(&self, pos: &Vector3) -> Option<usize> {
        self.spheres
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let da = a.1.center.distance(pos);
                let db = b.1.center.distance(pos);
                da.partial_cmp(&db).unwrap_or(Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Verifica si el grafo es conexo
    pub fn is_connected(&self) -> bool {
        if self.spheres.is_empty() {
            return true;
        }

        let mut visited = vec![false; self.spheres.len()];
        let mut stack = vec![0];
        let mut count = 0;

        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            count += 1;

            for &(neighbor, _) in &self.adjacency[node] {
                if !visited[neighbor] {
                    stack.push(neighbor);
                }
            }
        }

        count == self.spheres.len()
    }

    /// Obtiene componentes conexas
    pub fn connected_components(&self) -> Vec<Vec<usize>> {
        let n = self.spheres.len();
        let mut visited = vec![false; n];
        let mut components = Vec::new();

        for start in 0..n {
            if visited[start] {
                continue;
            }

            let mut component = Vec::new();
            let mut stack = vec![start];

            while let Some(node) = stack.pop() {
                if visited[node] {
                    continue;
                }
                visited[node] = true;
                component.push(node);

                for &(neighbor, _) in &self.adjacency[node] {
                    if !visited[neighbor] {
                        stack.push(neighbor);
                    }
                }
            }

            components.push(component);
        }

        components
    }
}

impl Default for SphereGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Estado para el algoritmo de Dijkstra
#[derive(Clone, Copy)]
struct DijkstraState {
    cost: Real,
    node: usize,
}

impl PartialEq for DijkstraState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}

impl Eq for DijkstraState {}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Invertir para min-heap
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Verifica si una conexión entre dos puntos está dentro de la malla
fn is_connection_valid(from: &Vector3, to: &Vector3, field: &DistanceField) -> bool {
    // Muestrear puntos a lo largo de la línea
    let num_samples = 5;
    for i in 1..num_samples {
        let t = i as Real / num_samples as Real;
        let pos = *from + (*to - *from) * t;
        let dist = field.sample(&pos);

        // Si algún punto está fuera de la malla, la conexión no es válida
        if dist <= 0.0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_spheres() -> Vec<MedialSphere> {
        vec![
            MedialSphere::new(Vector3::new(0.0, 0.0, 0.0), 0.5),
            MedialSphere::new(Vector3::new(1.0, 0.0, 0.0), 0.5),
            MedialSphere::new(Vector3::new(2.0, 0.0, 0.0), 0.5),
            MedialSphere::new(Vector3::new(1.0, 1.0, 0.0), 0.5),
        ]
    }

    #[test]
    fn test_sphere_graph_knn() {
        let spheres = make_test_spheres();
        let graph = SphereGraph::from_spheres_knn(&spheres, 2);

        assert_eq!(graph.num_spheres(), 4);
        assert!(graph.num_edges() > 0);
    }

    #[test]
    fn test_shortest_path() {
        let spheres = make_test_spheres();
        let graph = SphereGraph::from_spheres_knn(&spheres, 2);

        let path = graph.shortest_path(0, 2);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path[0], 0);
        assert_eq!(*path.last().unwrap(), 2);
    }

    #[test]
    fn test_mst() {
        let spheres = make_test_spheres();
        let graph = SphereGraph::from_spheres_knn(&spheres, 3);

        let mst = graph.minimum_spanning_tree();
        // MST debe tener n-1 aristas para n nodos
        assert_eq!(mst.len(), 3);
    }

    #[test]
    fn test_connected() {
        let spheres = make_test_spheres();
        let graph = SphereGraph::from_spheres_knn(&spheres, 2);

        assert!(graph.is_connected());
    }

    #[test]
    fn test_find_nearest() {
        let spheres = make_test_spheres();
        let graph = SphereGraph::from_spheres_knn(&spheres, 2);

        let nearest = graph.find_nearest_sphere(&Vector3::new(0.1, 0.0, 0.0));
        assert_eq!(nearest, Some(0));

        let nearest2 = graph.find_nearest_sphere(&Vector3::new(1.9, 0.0, 0.0));
        assert_eq!(nearest2, Some(2));
    }
}
