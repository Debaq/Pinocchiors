//! Pestañas del proyecto (como en Onshape): cada pestaña de diseño es un
//! documento con sus operaciones, parámetros y piezas; las de ensamble juntan
//! piezas de cualquier pestaña. Ver `libs/cad/PLAN_PESTANAS.md`.
//!
//! Los proyectos de antes tenían un solo diseño: pasa a ser la pestaña 0, así
//! las referencias guardadas sin pestaña (objetos, instancias) siguen valiendo.

use serde::{Deserialize, Serialize};

use crate::assembly::Assembly;
use crate::document::{Document, PartProps};
use crate::eval::Evaluation;
use crate::feature::{FeatureId, PartId};

pub type TabId = u32;

/// Nombre base de las pestañas nuevas (se numeran: «Diseño 1», «Diseño 2»…).
pub const DESIGN_NAME: &str = "Diseño";
pub const ASSEMBLY_NAME: &str = "Ensamble";

/// Bits de la operación en `tab_feature` (la pestaña va arriba).
const FEATURE_BITS: u32 = 20;

/// La operación `f` de la pestaña `tab` con un número único entre todas las
/// pestañas: así un ensamble de piezas de varias pestañas se arma como si
/// fueran de un solo diseño (ver [`Tabs::flat_assembly`]).
pub fn tab_feature(tab: TabId, f: FeatureId) -> FeatureId {
    debug_assert!(f.0 < 1 << FEATURE_BITS && tab < 1 << (32 - FEATURE_BITS));
    FeatureId(tab << FEATURE_BITS | f.0)
}

/// Al revés de [`tab_feature`]: la pestaña y la operación.
pub fn split_tab_feature(f: FeatureId) -> (TabId, FeatureId) {
    (f.0 >> FEATURE_BITS, FeatureId(f.0 & ((1 << FEATURE_BITS) - 1)))
}

fn tab_part(tab: TabId, p: PartId) -> PartId {
    PartId { feature: tab_feature(tab, p.feature), index: p.index }
}

/// Junta los recálculos de varias pestañas en uno: sus piezas y engranes con
/// los números de [`tab_feature`] (lo que el ensamble lee de un recálculo).
pub fn merge_evaluations<'a>(evals: impl IntoIterator<Item = (TabId, &'a Evaluation)>) -> Evaluation {
    let mut out = Evaluation::default();
    for (tab, ev) in evals {
        out.parts.extend(ev.parts.iter().map(|p| crate::eval::Part { id: tab_part(tab, p.id), ..p.clone() }));
        out.gears.extend(ev.gears.iter().map(|g| {
            let mut g = g.clone();
            g.feature = tab_feature(tab, g.feature);
            if let Some(link) = &mut g.part.link {
                link.feature = tab_feature(tab, link.feature);
            }
            g
        }));
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    /// En el orden de la barra.
    #[serde(default)]
    pub tabs: Vec<Tab>,
    /// La que se edita (ignorado si no hay pestañas).
    #[serde(default)]
    pub active: TabId,
    #[serde(default)]
    pub next_id: TabId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub name: String,
    pub kind: TabKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TabKind {
    Design { document: Box<Document> },
    Assembly { assembly: Assembly },
}

impl TabKind {
    pub fn is_design(&self) -> bool {
        matches!(self, TabKind::Design { .. })
    }
}

impl Tabs {
    /// Las pestañas de un proyecto de antes (un solo diseño o ninguno): el
    /// diseño queda tal cual en la pestaña 0.
    pub fn from_legacy(document: Option<Document>) -> Self {
        match document {
            None => Self::default(),
            Some(document) => Self {
                tabs: vec![Tab { id: 0, name: format!("{DESIGN_NAME} 1"), kind: TabKind::Design { document: Box::new(document) } }],
                active: 0,
                next_id: 1,
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn design(&self, id: TabId) -> Option<&Document> {
        match &self.get(id)?.kind {
            TabKind::Design { document } => Some(document.as_ref()),
            TabKind::Assembly { .. } => None,
        }
    }

    pub fn design_mut(&mut self, id: TabId) -> Option<&mut Document> {
        match &mut self.get_mut(id)?.kind {
            TabKind::Design { document } => Some(document.as_mut()),
            TabKind::Assembly { .. } => None,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.get(self.active)
    }

    /// El documento de la pestaña activa, si es de diseño.
    pub fn active_design(&self) -> Option<&Document> {
        self.design(self.active)
    }

    pub fn active_design_mut(&mut self) -> Option<&mut Document> {
        self.design_mut(self.active)
    }

    /// «Diseño 3»: el primer número libre para `base`.
    pub fn fresh_name(&self, base: &str) -> String {
        (1..).map(|n| format!("{base} {n}")).find(|name| !self.name_taken(name, None)).unwrap()
    }

    fn name_taken(&self, name: &str, except: Option<TabId>) -> bool {
        let name = name.trim().to_lowercase();
        self.tabs.iter().any(|t| Some(t.id) != except && t.name.trim().to_lowercase() == name)
    }

    fn fresh_id(&mut self) -> TabId {
        let used = self.tabs.iter().map(|t| t.id + 1).max().unwrap_or(0);
        let id = self.next_id.max(used);
        self.next_id = id + 1;
        id
    }

    /// Agrega una pestaña al final (con nombre numerado si no se da) y la deja activa.
    pub fn add(&mut self, kind: TabKind, name: Option<String>) -> TabId {
        let name = match name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
            Some(n) if !self.name_taken(&n, None) => n,
            Some(n) => self.fresh_name(&n),
            None => self.fresh_name(if kind.is_design() { DESIGN_NAME } else { ASSEMBLY_NAME }),
        };
        let id = self.fresh_id();
        self.tabs.push(Tab { id, name, kind });
        self.active = id;
        id
    }

    pub fn add_design(&mut self, name: Option<String>) -> TabId {
        self.add(TabKind::Design { document: Box::default() }, name)
    }

    pub fn add_assembly(&mut self, name: Option<String>) -> TabId {
        self.add(TabKind::Assembly { assembly: Assembly::default() }, name)
    }

    pub fn rename(&mut self, id: TabId, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("El nombre no puede quedar vacío".into());
        }
        if self.name_taken(name, Some(id)) {
            return Err(format!("Ya hay una pestaña «{name}»"));
        }
        self.get_mut(id).ok_or("Esa pestaña ya no está")?.name = name.to_string();
        Ok(())
    }

    /// Copia la pestaña al lado de la original («Diseño 1 (copia)») y la deja activa.
    pub fn duplicate(&mut self, id: TabId) -> Result<TabId, String> {
        let index = self.tabs.iter().position(|t| t.id == id).ok_or("Esa pestaña ya no está")?;
        let source = &self.tabs[index];
        let base = format!("{} (copia)", source.name);
        let name = if self.name_taken(&base, None) { self.fresh_name(&base) } else { base };
        let kind = source.kind.clone();
        let new_id = self.fresh_id();
        self.tabs.insert(index + 1, Tab { id: new_id, name, kind });
        self.active = new_id;
        Ok(new_id)
    }

    /// Saca la pestaña; si era la activa pasa a la de al lado.
    pub fn remove(&mut self, id: TabId) -> Result<Tab, String> {
        let index = self.tabs.iter().position(|t| t.id == id).ok_or("Esa pestaña ya no está")?;
        if self.tabs.len() == 1 {
            return Err("No se puede borrar la única pestaña".into());
        }
        let tab = self.tabs.remove(index);
        if self.active == id {
            self.active = self.tabs[index.min(self.tabs.len() - 1)].id;
        }
        Ok(tab)
    }

    /// Mueve la pestaña a la posición `to` de la barra.
    pub fn move_to(&mut self, id: TabId, to: usize) -> Result<(), String> {
        let from = self.tabs.iter().position(|t| t.id == id).ok_or("Esa pestaña ya no está")?;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to.min(self.tabs.len()), tab);
        Ok(())
    }

    pub fn activate(&mut self, id: TabId) -> Result<(), String> {
        self.get(id).ok_or("Esa pestaña ya no está")?;
        self.active = id;
        Ok(())
    }

    /// Ensambles que usan piezas de la pestaña `id` (para avisar antes de borrarla).
    pub fn users(&self, id: TabId) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|t| matches!(&t.kind, TabKind::Assembly { assembly } if assembly.instances.iter().any(|i| i.studio == id)))
            .map(|t| t.id)
            .collect()
    }

    /// Los ensambles que estaban dentro de un diseño (proyectos de antes) pasan
    /// a su propia pestaña, justo después del diseño. Se puede llamar las veces
    /// que sea: un diseño sin ensamble no cambia. Devuelve cuántos se mudaron.
    pub fn lift_assemblies(&mut self) -> usize {
        let mut moved = 0;
        let mut i = 0;
        while i < self.tabs.len() {
            let tab = &mut self.tabs[i];
            let TabKind::Design { document } = &mut tab.kind else {
                i += 1;
                continue;
            };
            let studio = tab.id;
            let Some(mut assembly) = document.assembly.take().filter(|a| !a.instances.is_empty()) else {
                document.assembly = None;
                i += 1;
                continue;
            };
            for inst in &mut assembly.instances {
                inst.studio = studio;
            }
            let name = self.fresh_name(ASSEMBLY_NAME);
            let id = self.fresh_id();
            self.tabs.insert(i + 1, Tab { id, name, kind: TabKind::Assembly { assembly } });
            moved += 1;
            i += 2;
        }
        moved
    }

    /// El ensamble de la pestaña `id` como si fuera de un solo diseño: sus
    /// piezas con los números de [`tab_feature`].
    pub fn flat_assembly(&self, id: TabId) -> Option<Assembly> {
        let TabKind::Assembly { assembly } = &self.get(id)?.kind else { return None };
        let mut flat = assembly.clone();
        for inst in &mut flat.instances {
            inst.part = tab_part(inst.studio, inst.part);
            inst.studio = 0;
        }
        Some(flat)
    }

    /// Guarda en la pestaña `id` un ensamble armado con [`flat_assembly`](Self::flat_assembly).
    pub fn set_flat_assembly(&mut self, id: TabId, mut flat: Assembly) -> Result<(), String> {
        for inst in &mut flat.instances {
            let (studio, feature) = split_tab_feature(inst.part.feature);
            inst.studio = studio;
            inst.part.feature = feature;
        }
        match &mut self.get_mut(id).ok_or("Esa pestaña ya no está")?.kind {
            TabKind::Assembly { assembly } => *assembly = flat,
            TabKind::Design { .. } => return Err("Esa pestaña no es un ensamble".into()),
        }
        Ok(())
    }

    /// La pestaña de ensamble `id` vista como un diseño sin operaciones: su
    /// ensamble con [`flat_assembly`](Self::flat_assembly) y las piezas de todos
    /// los diseños con [`flat_part_props`](Self::flat_part_props). Así la
    /// interfaz y los comandos del ensamble la tratan como el ensamble de un diseño.
    pub fn assembly_document(&self, id: TabId, names: &[(PartId, String)]) -> Option<Document> {
        let assembly = self.flat_assembly(id)?;
        let (parts, fabrication) = self.flat_part_props(names);
        Some(Document { assembly: Some(assembly), parts, fabrication, ..Document::default() })
    }

    /// Lo que un ensamble necesita de los diseños, con los números de
    /// [`tab_feature`]: nombre, color, visibilidad y material de cada pieza
    /// (`names`: los nombres de las piezas sin nombre puesto, de su recálculo),
    /// y la fabricación del primer diseño que la tenga (para las holguras).
    pub fn flat_part_props(&self, names: &[(PartId, String)]) -> (Vec<PartProps>, Option<crate::document::Fabrication>) {
        let mut props = Vec::new();
        let mut fabrication = None;
        for tab in &self.tabs {
            let TabKind::Design { document } = &tab.kind else { continue };
            fabrication = fabrication.or_else(|| document.fabrication.clone());
            for p in &document.parts {
                let mut p = p.clone();
                p.part = tab_part(tab.id, p.part);
                p.material = p.material.or_else(|| document.material.clone());
                props.push(p);
            }
        }
        for (id, name) in names {
            match props.iter_mut().find(|p| p.part == *id) {
                Some(p) if p.name.is_none() => p.name = Some(name.clone()),
                Some(_) => {}
                None => props.push(PartProps { part: *id, name: Some(name.clone()), color: None, hidden: false, material: None }),
            }
        }
        (props, fabrication)
    }
}
