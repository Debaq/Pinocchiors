//! Documento: lista ordenada de operaciones y su edición.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::eval::Evaluation;
use crate::expr::{Expr, valid_name};
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

/// Parámetro con nombre: `ancho = 40`, `alto = ancho / 2`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub expr: String,
}

/// Valor calculado de un parámetro o de un campo vinculado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedValue {
    /// Nombre del parámetro o ruta del campo
    pub key: String,
    pub value: Option<f64>,
    pub error: Option<String>,
}

/// Documento con las fórmulas aplicadas.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub document: Document,
    pub parameters: Vec<ResolvedValue>,
    pub bindings: Vec<ResolvedValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub features: Vec<Feature>,
    /// Recalcular solo las primeras N operaciones (barra de retroceso).
    #[serde(default)]
    pub rollback: Option<usize>,
    #[serde(default)]
    pub next_id: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// Campos calculados por fórmula: ruta → expresión. La ruta empieza con el
    /// id de la operación y sigue el JSON de la operación, p. ej.
    /// `3.kind.extent.distance` o `0.kind.sketch.constraints.4.value`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, String>,
    /// Material del sólido (para la masa); no cambia la geometría.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<Material>,
    /// Carpetas del árbol: solo presentación, no cambian el recálculo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<Folder>,
    /// Nombre, color y visibilidad de las piezas (no cambian la geometría).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartProps>,
    /// Ensamble de las piezas (instancias y relaciones); no cambia el diseño.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<crate::assembly::Assembly>,
    /// Ajustes y cotas del plano 2D (los maneja la interfaz; no cambian el diseño).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing: Option<serde_json::Value>,
}

/// Lo que el usuario le cambió a una pieza.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartProps {
    pub part: crate::feature::PartId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Color como "#rrggbb".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Material propio (si no, el del diseño).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<Material>,
}

/// Carpeta del árbol: las operaciones desde `first` hasta `last` en el orden
/// actual (así reordenar no la rompe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub first: FeatureId,
    pub last: FeatureId,
    #[serde(default)]
    pub collapsed: bool,
}

/// Material con su densidad en kg/m³.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub density: f64,
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
        self.features.insert(index.min(self.features.len()), Feature { id, name, suppressed: false, kind, scope: Vec::new() });
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

    /// Recalcula reutilizando lo que `cache` ya tiene (ver `EvalCache`).
    pub fn evaluate_with(&self, cache: &mut crate::eval::EvalCache) -> Evaluation {
        crate::eval::evaluate_with(self, cache)
    }

    /// Valores de los parámetros (en cualquier orden de dependencia) y el
    /// resultado de cada uno, con su error si no se pudo calcular.
    pub fn parameter_values(&self) -> (HashMap<String, f64>, Vec<ResolvedValue>) {
        let mut values: HashMap<String, f64> = HashMap::new();
        let mut errors: HashMap<usize, String> = HashMap::new();
        let mut parsed: Vec<Option<Expr>> = Vec::new();
        for (i, p) in self.parameters.iter().enumerate() {
            if !valid_name(&p.name) {
                errors.insert(i, format!("«{}» no sirve como nombre", p.name));
                parsed.push(None);
            } else if self.parameters[..i].iter().any(|q| q.name == p.name) {
                errors.insert(i, format!("«{}» está repetido", p.name));
                parsed.push(None);
            } else {
                match Expr::parse(&p.expr) {
                    Ok(e) => parsed.push(Some(e)),
                    Err(e) => {
                        errors.insert(i, e);
                        parsed.push(None);
                    }
                }
            }
        }
        // Calcular en pasadas: cada una resuelve los que ya tienen todo lo que usan
        let mut pending: Vec<usize> = (0..self.parameters.len()).filter(|i| parsed[*i].is_some()).collect();
        loop {
            let before = pending.len();
            pending.retain(|&i| {
                let e = parsed[i].as_ref().unwrap();
                if e.variables().iter().all(|v| values.contains_key(v)) {
                    match e.eval(&values) {
                        Ok(v) => {
                            values.insert(self.parameters[i].name.clone(), v);
                        }
                        Err(err) => {
                            errors.insert(i, err);
                        }
                    }
                    false
                } else {
                    true
                }
            });
            if pending.len() == before {
                break;
            }
        }
        let names: Vec<&str> = self.parameters.iter().map(|p| p.name.as_str()).collect();
        for i in pending {
            let vars = parsed[i].as_ref().unwrap().variables();
            let msg = match vars.iter().find(|v| !names.contains(&v.as_str())) {
                Some(v) => format!("no hay un parámetro «{v}»"),
                None => "referencia circular o a un parámetro con error".to_string(),
            };
            errors.insert(i, msg);
        }
        let report = self
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| ResolvedValue {
                key: p.name.clone(),
                value: (!errors.contains_key(&i)).then(|| values.get(&p.name).copied()).flatten(),
                error: errors.get(&i).cloned(),
            })
            .collect();
        (values, report)
    }

    /// Calcula una expresión con los parámetros del documento.
    pub fn eval_expr(&self, expr: &str) -> Result<f64, String> {
        Expr::parse(expr)?.eval(&self.parameter_values().0)
    }

    /// Aplica las fórmulas: el documento resultante tiene los números
    /// calculados en cada campo vinculado.
    pub fn resolve(&self) -> Resolution {
        let (values, parameters) = self.parameter_values();
        if self.bindings.is_empty() {
            return Resolution { document: self.clone(), parameters, bindings: vec![] };
        }
        let Ok(mut json) = serde_json::to_value(self) else {
            return Resolution { document: self.clone(), parameters, bindings: vec![] };
        };
        let mut bindings = Vec::new();
        for (path, expr) in &self.bindings {
            let result = Expr::parse(expr).and_then(|e| e.eval(&values)).and_then(|v| set_path(&mut json, path, v).map(|_| v));
            bindings.push(ResolvedValue { key: path.clone(), value: result.as_ref().ok().copied(), error: result.err() });
        }
        match serde_json::from_value::<Document>(json) {
            Ok(document) => Resolution { document, parameters, bindings },
            Err(e) => {
                for b in &mut bindings {
                    b.error.get_or_insert_with(|| format!("valor no aceptado: {e}"));
                }
                Resolution { document: self.clone(), parameters, bindings }
            }
        }
    }
}

/// Escribe `v` en el campo numérico de la ruta (`<id operación>.<claves…>`).
fn set_path(doc: &mut serde_json::Value, path: &str, v: f64) -> Result<(), String> {
    let mut parts = path.split('.');
    let id: u32 = parts.next().and_then(|p| p.parse().ok()).ok_or("ruta sin operación")?;
    let features = doc.get_mut("features").and_then(|f| f.as_array_mut()).ok_or("documento sin operaciones")?;
    let mut node = features
        .iter_mut()
        .find(|f| f.get("id").and_then(|x| x.as_u64()) == Some(id as u64))
        .ok_or_else(|| format!("la operación {id} ya no existe"))?;
    for key in parts {
        node = match node {
            serde_json::Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
            serde_json::Value::Object(o) => o.get_mut(key),
            _ => None,
        }
        .ok_or_else(|| format!("el campo «{path}» ya no existe"))?;
    }
    *node = match node {
        // Campos enteros (cantidades): redondear
        serde_json::Value::Number(n) if n.is_u64() || n.is_i64() => {
            if v < 0.0 {
                return Err("tiene que ser un entero positivo".into());
            }
            serde_json::json!(v.round() as u64)
        }
        serde_json::Value::Number(_) => serde_json::json!(v),
        _ => return Err(format!("el campo «{path}» no es un número")),
    };
    Ok(())
}
