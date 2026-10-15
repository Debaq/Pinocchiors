//! Molde de dos mitades alrededor de un escaneo.

use cad_model::geom::P3;
use cad_model::{BodyOp, Document, FeatureId, FeatureKind, PartId, Primitive, PrimitiveShape, ScanBody};
use serde::Deserialize;

/// Cómo se arma el molde (ver [`add_mold`]).
#[derive(Debug, Clone, Deserialize)]
pub struct MoldOptions {
    /// Eje que cruza el plano de partición (0 = X, 1 = Y, 2 = Z)
    pub axis: usize,
    /// Dónde parte, de 0 a 1 a lo largo del escaneo en ese eje
    pub at: f64,
    /// Espesor de las paredes (mm)
    pub wall: f64,
    /// Holgura de la cavidad (mm): positiva la agranda
    pub gap: f64,
    /// Llaves de registro (tres pernos y sus huecos)
    pub keys: bool,
    /// Radio del bebedero (mm); 0 = sin bebedero
    pub sprue: f64,
}

/// Agrega al diseño un molde: dos cajas alrededor del escaneo (`lo`..`hi`)
/// partidas por un plano, la cavidad (escaneo restado con la holgura), tres
/// llaves de registro (perno en la mitad de abajo, hueco en la de arriba) y un
/// bebedero al centro de la de arriba.
pub fn add_mold(doc: &mut Document, lo: P3, hi: P3, o: &MoldOptions) -> Result<(), String> {
    if o.axis > 2 || !(o.at > 0.0 && o.at < 1.0) || o.wall <= 0.0 {
        return Err("Molde: el eje, el punto de partición (entre 0 y 1) y el espesor de pared tienen que ser válidos".into());
    }
    let a = o.axis;
    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
    let w = o.wall;
    let cut = lo[a] + (hi[a] - lo[a]) * o.at;
    let axis_dir = |k: usize| {
        let mut v = [0.0; 3];
        v[k] = 1.0;
        v
    };
    let (z, x) = (axis_dir(a), axis_dir(b));
    // Caja desde `from` hasta `to` en el eje, con las paredes alrededor del escaneo
    let slab = |from: f64, to: f64| {
        let mut origin = [0.0; 3];
        let mut size = [0.0; 3];
        origin[a] = from;
        size[a] = to - from;
        for k in [b, c] {
            origin[k] = lo[k] - w;
            size[k] = hi[k] - lo[k] + 2.0 * w;
        }
        FeatureKind::Primitive(Primitive {
            shape: PrimitiveShape::Box { dx: size[0], dy: size[1], dz: size[2], centered: false, centered_z: false },
            origin,
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::New,
            link: None,
        })
    };
    let cylinder = |center: [f64; 2], from: f64, height: f64, radius: f64, op: BodyOp| {
        let mut origin = [0.0; 3];
        origin[a] = from;
        origin[b] = center[0];
        origin[c] = center[1];
        FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Cylinder { radius, height }, origin, z, x, op, link: None })
    };
    {
        let named = |doc: &mut Document, kind: FeatureKind, name: &str, scope: Option<FeatureId>| {
            let id = doc.add(kind);
            let f = doc.get_mut(id).unwrap();
            f.name = name.to_string();
            if let Some(feature) = scope {
                f.scope = vec![PartId { feature, index: 0 }];
            }
            id
        };
        let lower = named(doc, slab(lo[a] - w, cut), "Molde: mitad de abajo", None);
        let upper = named(doc, slab(cut, hi[a] + w), "Molde: mitad de arriba", None);
        named(doc, FeatureKind::ScanBody(ScanBody { offset: o.gap, op: BodyOp::Cut, ..Default::default() }), "Molde: cavidad", None);
        if o.keys {
            // Tres de las cuatro esquinas, en el medio de la pared: así la tapa solo calza de una forma
            let r = (w * 0.3).clamp(0.5, 5.0);
            let (pb, pc) = ((lo[b] - w / 2.0, hi[b] + w / 2.0), (lo[c] - w / 2.0, hi[c] + w / 2.0));
            let height = (r * 2.0).min((hi[a] - lo[a]) * 0.25).max(r);
            for (k, center) in [[pb.0, pc.0], [pb.1, pc.0], [pb.1, pc.1]].into_iter().enumerate() {
                // Hueco en la de arriba (con 0,2 mm de juego) y perno en la de abajo, que entra 1 mm en su pared
                named(doc, cylinder(center, cut, height + 0.4, r + 0.2, BodyOp::Cut), &format!("Molde: hueco de llave {}", k + 1), Some(upper));
                named(doc, cylinder(center, cut - 1.0, height + 1.0, r, BodyOp::Join), &format!("Molde: llave {}", k + 1), Some(lower));
            }
        }
        if o.sprue > 0.0 {
            let center = [(lo[b] + hi[b]) / 2.0, (lo[c] + hi[c]) / 2.0];
            named(doc, cylinder(center, cut, hi[a] + w + 1.0 - cut, o.sprue, BodyOp::Cut), "Molde: bebedero", Some(upper));
        }
    }
    Ok(())
}
