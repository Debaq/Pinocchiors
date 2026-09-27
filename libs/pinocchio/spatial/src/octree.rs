//! Octree para búsquedas espaciales

use pinocchio_math::{Real, Rect, Vector3};

/// Nodo de un Octree
#[derive(Debug)]
#[allow(dead_code)]
enum OctreeNode<T> {
    /// Nodo hoja con datos
    Leaf {
        bounds: Rect,
        items: Vec<(Vector3, T)>,
    },
    /// Nodo interno con 8 hijos
    Internal {
        bounds: Rect,
        children: Box<[Option<OctreeNode<T>>; 8]>,
    },
}

/// Octree para búsquedas espaciales eficientes
#[derive(Debug)]
pub struct Octree<T> {
    root: Option<OctreeNode<T>>,
    _max_depth: usize,
    _max_items_per_leaf: usize,
}

impl<T: Clone> Octree<T> {
    /// Crea un nuevo Octree vacío
    pub fn new(max_depth: usize, max_items_per_leaf: usize) -> Self {
        Self {
            root: None,
            _max_depth: max_depth,
            _max_items_per_leaf: max_items_per_leaf,
        }
    }

    /// Crea un Octree con límites predefinidos
    pub fn with_bounds(bounds: Rect, max_depth: usize, max_items_per_leaf: usize) -> Self {
        Self {
            root: Some(OctreeNode::Leaf {
                bounds,
                items: Vec::new(),
            }),
            _max_depth: max_depth,
            _max_items_per_leaf: max_items_per_leaf,
        }
    }

    /// Inserta un elemento en el Octree
    pub fn insert(&mut self, position: Vector3, item: T) {
        if self.root.is_none() {
            self.root = Some(OctreeNode::Leaf {
                bounds: Rect::from_point(position).expand(1.0),
                items: vec![(position, item)],
            });
            return;
        }

        // Expandir bounds si es necesario
        let bounds = self.bounds().unwrap();
        if !bounds.contains_point(&position) {
            let new_bounds = bounds.expanded_to_point(position).expand(0.1);
            self.rebuild_with_bounds(new_bounds);
        }

        // Insertar en el nodo raíz
        if let Some(ref mut node) = self.root {
            Self::insert_into_node(node, position, item);
        }
    }

    fn insert_into_node(node: &mut OctreeNode<T>, position: Vector3, item: T) {
        match node {
            OctreeNode::Leaf { items, .. } => {
                items.push((position, item));
            }
            OctreeNode::Internal { children, bounds } => {
                let octant = Self::get_octant(bounds, &position);
                if let Some(ref mut child) = children[octant] {
                    Self::insert_into_node(child, position, item);
                } else {
                    let child_bounds = Self::get_child_bounds(bounds, octant);
                    children[octant] = Some(OctreeNode::Leaf {
                        bounds: child_bounds,
                        items: vec![(position, item)],
                    });
                }
            }
        }
    }

    /// Obtiene el octante (0-7) para una posición
    fn get_octant(bounds: &Rect, position: &Vector3) -> usize {
        let center = bounds.center();
        let mut octant = 0;
        if position.x() >= center.x() { octant |= 1; }
        if position.y() >= center.y() { octant |= 2; }
        if position.z() >= center.z() { octant |= 4; }
        octant
    }

    /// Calcula los límites de un hijo dado el octante
    fn get_child_bounds(bounds: &Rect, octant: usize) -> Rect {
        let center = bounds.center();
        let min = bounds.min;
        let max = bounds.max;

        let new_min = Vector3::new(
            if octant & 1 == 0 { min.x() } else { center.x() },
            if octant & 2 == 0 { min.y() } else { center.y() },
            if octant & 4 == 0 { min.z() } else { center.z() },
        );
        let new_max = Vector3::new(
            if octant & 1 == 0 { center.x() } else { max.x() },
            if octant & 2 == 0 { center.y() } else { max.y() },
            if octant & 4 == 0 { center.z() } else { max.z() },
        );

        Rect::new(new_min, new_max)
    }

    /// Reconstruye el Octree con nuevos límites
    fn rebuild_with_bounds(&mut self, new_bounds: Rect) {
        let items = self.collect_all();
        self.root = Some(OctreeNode::Leaf {
            bounds: new_bounds,
            items,
        });
    }

    /// Recolecta todos los elementos
    fn collect_all(&self) -> Vec<(Vector3, T)> {
        let mut items = Vec::new();
        Self::collect_from_node(&self.root, &mut items);
        items
    }

    fn collect_from_node(node: &Option<OctreeNode<T>>, items: &mut Vec<(Vector3, T)>) {
        if let Some(n) = node {
            match n {
                OctreeNode::Leaf { items: leaf_items, .. } => {
                    items.extend(leaf_items.iter().cloned());
                }
                OctreeNode::Internal { children, .. } => {
                    for child in children.iter() {
                        Self::collect_from_node(child, items);
                    }
                }
            }
        }
    }

    /// Obtiene los límites del Octree
    pub fn bounds(&self) -> Option<Rect> {
        self.root.as_ref().map(|n| match n {
            OctreeNode::Leaf { bounds, .. } => *bounds,
            OctreeNode::Internal { bounds, .. } => *bounds,
        })
    }

    /// Busca elementos dentro de un radio
    pub fn query_radius(&self, center: &Vector3, radius: Real) -> Vec<&T> {
        let mut results = Vec::new();
        let radius_sq = radius * radius;
        Self::query_radius_from_node(&self.root, center, radius_sq, &mut results);
        results
    }

    fn query_radius_from_node<'a>(
        node: &'a Option<OctreeNode<T>>,
        center: &Vector3,
        radius_sq: Real,
        results: &mut Vec<&'a T>,
    ) {
        if let Some(n) = node {
            match n {
                OctreeNode::Leaf { bounds, items } => {
                    if bounds.distance_squared_to_point(center) <= radius_sq {
                        for (pos, item) in items {
                            if pos.distance_squared(center) <= radius_sq {
                                results.push(item);
                            }
                        }
                    }
                }
                OctreeNode::Internal { bounds, children } => {
                    if bounds.distance_squared_to_point(center) <= radius_sq {
                        for child in children.iter() {
                            Self::query_radius_from_node(child, center, radius_sq, results);
                        }
                    }
                }
            }
        }
    }

    /// Busca el vecino más cercano
    pub fn nearest_neighbor(&self, query: &Vector3) -> Option<(&Vector3, &T)> {
        let mut best: Option<(&Vector3, &T, Real)> = None;
        Self::nearest_from_node(&self.root, query, &mut best);
        best.map(|(pos, item, _)| (pos, item))
    }

    fn nearest_from_node<'a>(
        node: &'a Option<OctreeNode<T>>,
        query: &Vector3,
        best: &mut Option<(&'a Vector3, &'a T, Real)>,
    ) {
        if let Some(n) = node {
            let bounds = match n {
                OctreeNode::Leaf { bounds, .. } => bounds,
                OctreeNode::Internal { bounds, .. } => bounds,
            };

            // Podar si el nodo no puede contener un punto más cercano
            if let Some((_, _, best_dist)) = best
                && bounds.distance_squared_to_point(query) > *best_dist {
                    return;
                }

            match n {
                OctreeNode::Leaf { items, .. } => {
                    for (pos, item) in items {
                        let dist = pos.distance_squared(query);
                        if best.is_none() || dist < best.unwrap().2 {
                            *best = Some((pos, item, dist));
                        }
                    }
                }
                OctreeNode::Internal { children, .. } => {
                    for child in children.iter() {
                        Self::nearest_from_node(child, query, best);
                    }
                }
            }
        }
    }

    /// Número total de elementos
    pub fn len(&self) -> usize {
        self.collect_all().len()
    }

    /// Verifica si está vacío
    pub fn is_empty(&self) -> bool {
        self.root.is_none() || self.len() == 0
    }
}

impl<T: Clone> Default for Octree<T> {
    fn default() -> Self {
        Self::new(10, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_query() {
        let mut octree = Octree::new(10, 16);

        octree.insert(Vector3::new(0.0, 0.0, 0.0), 1);
        octree.insert(Vector3::new(1.0, 0.0, 0.0), 2);
        octree.insert(Vector3::new(0.0, 1.0, 0.0), 3);

        assert_eq!(octree.len(), 3);

        let results = octree.query_radius(&Vector3::zero(), 0.5);
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0], 1);
    }

    #[test]
    fn test_nearest_neighbor() {
        let mut octree = Octree::new(10, 16);

        octree.insert(Vector3::new(0.0, 0.0, 0.0), "origin");
        octree.insert(Vector3::new(1.0, 0.0, 0.0), "x");
        octree.insert(Vector3::new(0.0, 1.0, 0.0), "y");

        let query = Vector3::new(0.1, 0.1, 0.0);
        let (_, item) = octree.nearest_neighbor(&query).unwrap();
        assert_eq!(*item, "origin");
    }
}
