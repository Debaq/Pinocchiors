//! Documento: lista ordenada de operaciones y su edición.

use serde::{Deserialize, Serialize};

use crate::eval::Evaluation;
use crate::feature::{Feature, FeatureId, FeatureKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModelError {
    #[error("la operación {0:?} no existe")]
    NoFeature(FeatureId),
    #[error("otras operaciones dependen de esta: {0:?}")]
    HasDependents(Vec<FeatureId>),
    #[error("la operación quedaría antes de algo que necesita: {0:?}")]
    BreaksOrder(Vec<FeatureId>),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub features: Vec<Feature>,
    /// Recalcular solo las primeras N operaciones (barra de retroceso).
    #[serde(default)]
    pub rollback: Option<usize>,
    #[serde(default)]
    pub next_id: u32,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    fn fresh_id(&mut self) -> FeatureId {
        let max = self.features.iter().map(|f| f.id.0 + 1).max().unwrap_or(0);
        self.next_id = self.next_id.max(max);
        let id = FeatureId(self.next_id);
        self.next_id += 1;
        id
    }

    fn default_name(&self, kind: &FeatureKind) -> String {
        let label = kind.label();
        let n = self.features.iter().filter(|f| f.kind.label() == label).count() + 1;
        format!("{label} {n}")
    }

    /// Agrega al final (o en la posición de retroceso, si hay una activa).
    pub fn add(&mut self, kind: FeatureKind) -> FeatureId {
        let at = self.rollback.unwrap_or(self.features.len()).min(self.features.len());
        let id = self.insert(at, kind);
        if let Some(r) = self.rollback.as_mut() {
            *r += 1;
        }
        id
    }

    pub fn insert(&mut self, index: usize, kind: FeatureKind) -> FeatureId {
        let id = self.fresh_id();
        let name = self.default_name(&kind);
        self.features.insert(index.min(self.features.len()), Feature { id, name, suppressed: false, kind });
        id
    }

    pub fn index_of(&self, id: FeatureId) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }

    pub fn get(&self, id: FeatureId) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    pub fn get_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
        self.features.iter_mut().find(|f| f.id == id)
    }

    /// Operaciones que dependen directamente de `id`.
    pub fn dependents(&self, id: FeatureId) -> Vec<FeatureId> {
        self.features.iter().filter(|f| f.kind.dependencies().contains(&id)).map(|f| f.id).collect()
    }

    pub fn remove(&mut self, id: FeatureId) -> Result<Feature, ModelError> {
        let i = self.index_of(id).ok_or(ModelError::NoFeature(id))?;
        let deps = self.dependents(id);
        if !deps.is_empty() {
            return Err(ModelError::HasDependents(deps));
        }
        if let Some(r) = self.rollback.as_mut()
            && i < *r
        {
            *r -= 1;
        }
        Ok(self.features.remove(i))
    }

    /// Mueve la operación a `index` si el orden de dependencias lo permite.
    pub fn move_feature(&mut self, id: FeatureId, index: usize) -> Result<(), ModelError> {
        let from = self.index_of(id).ok_or(ModelError::NoFeature(id))?;
        let mut order = self.features.clone();
        let f = order.remove(from);
        let to = index.min(order.len());
        order.insert(to, f);
        let pos = |x: FeatureId| order.iter().position(|f| f.id == x);
        let mut broken = Vec::new();
        for (i, f) in order.iter().enumerate() {
            for d in f.kind.dependencies() {
                if pos(d).is_none_or(|p| p > i) {
                    broken.push(d);
                }
            }
        }
        if !broken.is_empty() {
            return Err(ModelError::BreaksOrder(broken));
        }
        self.features = order;
        Ok(())
    }

    pub fn set_suppressed(&mut self, id: FeatureId, suppressed: bool) -> Result<(), ModelError> {
        self.get_mut(id).ok_or(ModelError::NoFeature(id))?.suppressed = suppressed;
        Ok(())
    }

    /// Recalcula todo el árbol.
    pub fn evaluate(&self) -> Evaluation {
        crate::eval::evaluate(self)
    }
}
