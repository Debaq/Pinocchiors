//! Simplificar: quita triángulos conservando la forma (meshoptimizer).
//!
//! Colapsa aristas minimizando el error cuadrático de la posición y de los
//! atributos (UV, normales…). Los vértices que comparten posición pero no
//! atributos (costuras de UV, aristas vivas con normales partidas, límites
//! entre grupos) se mueven juntos y solo a lo largo de la costura: la
//! silueta de las islas UV no se rompe. El resultado usa los vértices de la
//! entrada, así cada uno conserva todos sus atributos (también los que no se
//! le pasan, como pesos o colores).

use super::deviation::Reference;
use meshopt::{SimplifyOptions as Flags, VertexDataAdapter};
use std::collections::HashMap;

/// La malla a simplificar.
#[derive(Debug, Clone, Copy)]
pub struct SimplifyInput<'a> {
    pub positions: &'a [[f32; 3]],
    /// Triángulos (3 índices cada uno).
    pub indices: &'a [u32],
    /// Atributos por vértice, `attribute_weights.len()` valores por vértice
    /// (vacío: solo posición).
    pub attributes: &'a [f32],
    /// Peso de cada atributo frente a la posición (a menor peso, más se
    /// permite deformarlo).
    pub attribute_weights: &'a [f32],
    /// Grupo de cada vértice (primitiva, material…; vacío: todos el mismo).
    /// Vértices iguales de grupos distintos no se sueldan: el límite entre
    /// grupos se conserva como una costura.
    pub groups: &'a [u32],
}

/// Qué se busca al simplificar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimplifyOptions {
    /// Triángulos buscados.
    pub target_triangles: usize,
    /// Desviación máxima permitida, en las unidades de la malla (`None`: sin
    /// límite, solo cuenta el objetivo).
    pub max_error: Option<f32>,
    /// No mover los vértices de los bordes abiertos.
    pub lock_borders: bool,
    /// No colapsar a través de las costuras (UV, normales partidas, grupos).
    pub keep_seams: bool,
    /// Llegar al objetivo aunque cambie la topología (une partes cercanas,
    /// cierra agujeros chicos, ignora costuras y atributos).
    pub aggressive: bool,
}

impl Default for SimplifyOptions {
    fn default() -> Self {
        Self { target_triangles: 0, max_error: None, lock_borders: false, keep_seams: true, aggressive: false }
    }
}

/// Triángulos que quedan, con índices a los vértices de la entrada.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Simplified {
    pub indices: Vec<u32>,
    /// Error que estima meshoptimizer, en las unidades de la malla.
    pub error: f32,
}

/// Simplifica la malla. Sin triángulos o con un objetivo igual o mayor que
/// los que hay, la devuelve igual.
///
/// Con `max_error`, el resultado se mide contra la entrada: el error de
/// meshoptimizer es una estimación (cuádricas) que puede quedarse corta a la
/// mitad, así que si se pasa se repite con un límite más estricto.
pub fn simplify(input: &SimplifyInput, options: &SimplifyOptions) -> Simplified {
    let Some(limit) = options.max_error else {
        return simplify_once(input, options);
    };
    let first = simplify_once(input, options);
    if first.indices.len() == input.indices.len() / 3 * 3 {
        return first;
    }
    let original: Vec<[f64; 3]> = input.positions.iter().map(|p| p.map(f64::from)).collect();
    let reference = Reference::new((&original, &triangles(input.indices)));
    let measure = |r: &Simplified| reference.measure((&original, &triangles(&r.indices))).max as f32;
    let mut measured = measure(&first);
    if measured <= limit {
        return first;
    }
    // Se busca el límite para meshoptimizer que da el resultado más chico que
    // cumple: entre `good` (cumple) y `bad` (se pasa), por bisección en escala
    // logarítmica; la primera vez, suponiendo el exceso proporcional
    let (mut good, mut bad) = (None::<f32>, limit);
    let mut best: Option<Simplified> = None;
    let mut next = limit * 0.9 * limit / measured;
    for _ in 0..MAX_RETRIES {
        let result = simplify_once(input, &SimplifyOptions { max_error: Some(next), ..*options });
        measured = measure(&result);
        if measured <= limit {
            good = Some(next);
            if best.as_ref().is_none_or(|b| result.indices.len() < b.indices.len()) {
                best = Some(result);
            }
        } else {
            bad = next;
        }
        next = match good {
            Some(good) if bad / good < 1.15 => break,
            Some(good) => (good * bad).sqrt(),
            None => next * 0.9 * limit / measured,
        };
    }
    // Si nada cumplió, la malla queda como estaba
    best.unwrap_or_else(|| Simplified { indices: input.indices[..input.indices.len() / 3 * 3].to_vec(), error: 0.0 })
}

/// Intentos extra cuando el resultado se pasa del error pedido
const MAX_RETRIES: usize = 5;

fn triangles(indices: &[u32]) -> Vec<[usize; 3]> {
    indices.chunks_exact(3).map(|c| [c[0] as usize, c[1] as usize, c[2] as usize]).collect()
}

fn simplify_once(input: &SimplifyInput, options: &SimplifyOptions) -> Simplified {
    let triangles = input.indices.len() / 3;
    let indices = &input.indices[..triangles * 3];
    if triangles == 0 || options.target_triangles >= triangles {
        return Simplified { indices: indices.to_vec(), error: 0.0 };
    }
    let stride = input.attribute_weights.len();
    debug_assert_eq!(input.attributes.len(), input.positions.len() * stride);

    // Soldar los vértices idénticos (los formatos sin índices o el STL traen
    // un vértice por esquina): si no, cada arista sería una costura
    let (welded, representative) = weld(input, stride);
    let indices: Vec<u32> = indices.iter().map(|&i| welded[i as usize]).collect();
    let positions: Vec<[f32; 3]> = representative.iter().map(|&i| input.positions[i]).collect();
    let attributes: Vec<f32> = representative
        .iter()
        .flat_map(|&i| &input.attributes[i * stride..(i + 1) * stride])
        .copied()
        .collect();

    let bytes: Vec<u8> = positions.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
    let adapter = VertexDataAdapter::new(&bytes, 12, 0).expect("posiciones de 12 bytes");
    let target = (options.target_triangles.max(1)) * 3;
    let mut error = 0.0f32;
    let result = if options.aggressive {
        // El modo rápido mide el error relativo al tamaño de la malla
        let scale = meshopt::simplify_scale(&adapter).max(f32::MIN_POSITIVE);
        let limit = options.max_error.map_or(f32::MAX, |e| e / scale);
        let result = meshopt::simplify_sloppy(&indices, &adapter, target, limit, Some(&mut error));
        error *= scale;
        result
    } else {
        let mut flags = Flags::ErrorAbsolute;
        if options.lock_borders {
            flags |= Flags::LockBorder;
        }
        if !options.keep_seams {
            flags |= Flags::Permissive;
        }
        let limit = options.max_error.unwrap_or(f32::MAX);
        if stride == 0 {
            meshopt::simplify(&indices, &adapter, target, limit, flags, Some(&mut error))
        } else {
            let locks = vec![false; positions.len()];
            meshopt::simplify_with_attributes_and_locks(
                &indices,
                &adapter,
                &attributes,
                input.attribute_weights,
                stride * 4,
                &locks,
                target,
                limit,
                flags,
                Some(&mut error),
            )
        }
    };
    let result = meshopt::optimize_vertex_cache(&result, positions.len());
    Simplified { indices: result.into_iter().map(|i| representative[i as usize] as u32).collect(), error }
}

/// Funde los vértices iguales en grupo, posición y atributos. Devuelve el
/// vértice soldado de cada uno y el primer vértice de entrada de cada soldado.
fn weld(input: &SimplifyInput, stride: usize) -> (Vec<u32>, Vec<usize>) {
    let mut map: HashMap<Vec<u32>, u32> = HashMap::with_capacity(input.positions.len());
    let mut representative = Vec::new();
    let welded = (0..input.positions.len())
        .map(|i| {
            let group = input.groups.get(i).copied().unwrap_or(0);
            let key: Vec<u32> = std::iter::once(group)
                .chain(input.positions[i].iter().map(|f| canonical(*f)))
                .chain(input.attributes[i * stride..(i + 1) * stride].iter().map(|f| canonical(*f)))
                .collect();
            *map.entry(key).or_insert_with(|| {
                representative.push(i);
                representative.len() as u32 - 1
            })
        })
        .collect();
    (welded, representative)
}

/// Bits del número con −0 igual a 0
fn canonical(f: f32) -> u32 {
    if f == 0.0 { 0 } else { f.to_bits() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla ondulada de n×n cuadrados con UV, sin soldar si `split`
    fn grid(n: u32, split: bool) -> (Vec<[f32; 3]>, Vec<f32>, Vec<u32>) {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
                positions.push([u, v, 0.02 * (u * 6.0).sin()]);
                uvs.extend([u, v]);
            }
        }
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                let (b, c, d) = (a + 1, a + n + 1, a + n + 2);
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        if !split {
            return (positions, uvs, indices);
        }
        let p = indices.iter().map(|&i| positions[i as usize]).collect();
        let t = indices.iter().flat_map(|&i| [uvs[2 * i as usize], uvs[2 * i as usize + 1]]).collect();
        (p, t, (0..indices.len() as u32).collect())
    }

    fn run(positions: &[[f32; 3]], uvs: &[f32], indices: &[u32], options: SimplifyOptions) -> Simplified {
        let input = SimplifyInput { positions, indices, attributes: uvs, attribute_weights: &[0.5, 0.5], groups: &[] };
        simplify(&input, &options)
    }

    #[test]
    fn reaches_the_target() {
        let (p, uv, t) = grid(40, false);
        let r = run(&p, &uv, &t, SimplifyOptions { target_triangles: 800, ..Default::default() });
        let n = r.indices.len() / 3;
        assert!(n <= 800 && n >= 760, "{n}");
        assert!(r.indices.iter().all(|&i| (i as usize) < p.len()));
    }

    #[test]
    fn unwelded_input_simplifies_like_welded() {
        let (p, uv, t) = grid(30, true);
        let r = run(&p, &uv, &t, SimplifyOptions { target_triangles: 300, ..Default::default() });
        assert!(r.indices.len() / 3 <= 300, "{}", r.indices.len() / 3);
    }

    #[test]
    fn error_limit_stops_reduction() {
        let (p, uv, t) = grid(40, false);
        let loose = run(&p, &uv, &t, SimplifyOptions { target_triangles: 10, max_error: Some(0.1), ..Default::default() });
        let strict = run(&p, &uv, &t, SimplifyOptions { target_triangles: 10, max_error: Some(1e-5), ..Default::default() });
        assert!(strict.indices.len() > loose.indices.len());
        assert!(strict.error <= 1e-5);
    }

    #[test]
    fn locked_borders_do_not_move() {
        let (p, uv, t) = grid(20, false);
        let r = run(&p, &uv, &t, SimplifyOptions { target_triangles: 20, lock_borders: true, ..Default::default() });
        // Los 80 vértices del borde siguen usados
        let used: std::collections::HashSet<u32> = r.indices.iter().copied().collect();
        let border = (0..p.len()).filter(|&i| {
            let (x, y) = (i % 21, i / 21);
            x == 0 || y == 0 || x == 20 || y == 20
        });
        assert!(border.into_iter().all(|i| used.contains(&(i as u32))));
    }

    #[test]
    fn groups_keep_their_boundary() {
        // Mitad izquierda grupo 0, derecha grupo 1: vértices del medio duplicados
        let n = 20u32;
        let (p, uv, t) = grid(n, true);
        let groups: Vec<u32> = (0..p.len()).map(|i| u32::from(p[t[i / 3 * 3] as usize][0] + p[t[i / 3 * 3 + 1] as usize][0] + p[t[i / 3 * 3 + 2] as usize][0] > 1.5)).collect();
        let input = SimplifyInput { positions: &p, indices: &t, attributes: &uv, attribute_weights: &[0.5, 0.5], groups: &groups };
        let r = simplify(&input, &SimplifyOptions { target_triangles: 50, ..Default::default() });
        // Ningún triángulo mezcla grupos
        for tri in r.indices.chunks(3) {
            assert!(tri.iter().all(|&i| groups[i as usize] == groups[tri[0] as usize]));
        }
    }

    #[test]
    fn aggressive_reaches_tiny_targets() {
        let (p, uv, t) = grid(40, false);
        let r = run(&p, &uv, &t, SimplifyOptions { target_triangles: 20, aggressive: true, ..Default::default() });
        assert!(r.indices.len() / 3 <= 20);
        assert!(!r.indices.is_empty());
    }

    #[test]
    fn target_above_count_leaves_mesh() {
        let (p, uv, t) = grid(4, false);
        let r = run(&p, &uv, &t, SimplifyOptions { target_triangles: 1000, ..Default::default() });
        assert_eq!(r.indices, t);
    }
}
