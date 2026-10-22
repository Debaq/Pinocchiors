//! Pestañas del proyecto (como en Onshape): cada pestaña de diseño es un
//! documento con sus operaciones, parámetros y piezas; las de ensamble juntan
//! piezas de cualquier pestaña. Ver `libs/cad/PLAN_PESTANAS.md`.
//!
//! Los proyectos de antes tenían un solo diseño: pasa a ser la pestaña 0, así
//! las referencias guardadas sin pestaña (objetos, instancias) siguen valiendo.

use serde::{Deserialize, Serialize};

use crate::assembly::Assembly;
use crate::document::Document;

pub type TabId = u32;

/// Nombre base de las pestañas nuevas (se numeran: «Diseño 1», «Diseño 2»…).
pub const DESIGN_NAME: &str = "Diseño";
pub const ASSEMBLY_NAME: &str = "Ensamble";

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
}
