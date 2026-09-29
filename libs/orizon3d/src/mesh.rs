//! Reconstrucción de malla desde la nube de puntos (campo de distancia con signo
//! por MLS + Surface Nets) y export a OBJ / STL / PLY.
//!
//! En vez de una ocupación binaria (que produce una cáscara de 1 vóxel y, al
//! extraer la isosuperficie, una doble pared arrugada con agujeros), se construye
//! un campo escalar CON SIGNO por mínimos cuadrados móviles (MLS): cada punto
//! reparte en su vecindad la distancia con signo a su plano tangente (su normal,
//! orientada hacia la cámara), ponderada por un núcleo gaussiano. El campo
//! F = Σw·d / Σw cruza el cero UNA sola vez a lo largo de la normal → una sola
//! pared suave que pasa por los puntos; el solape de núcleos rellena huecos. La
//! confianza W = Σw recorta el fondo y deja abiertas las zonas sin datos. Después
//! se extrae la isosuperficie F=0 con Surface Nets (un vértice por celda activa,
//! colocado en el centroide de los puntos reales de la celda, o en el cruce-cero
//! interpolado si la celda es de relleno) y se conectan celdas vecinas cuyo borde
//! cambia de signo. El color de cada vértice se toma de los puntos cercanos.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::pointcloud::PointCloud;

/// Malla triangular con color por vértice.
#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 3]>,
    pub tris: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Simplifica la malla por agrupamiento de vértices: fusiona los vértices que
    /// caen en una misma celda de tamaño `cell` (mm) y descarta triángulos
    /// degenerados. Reduce el número de triángulos conservando la forma.
    pub fn decimate(&self, cell: f32) -> Mesh {
        if cell <= 0.0 || self.vertices.is_empty() {
            return Mesh {
                vertices: self.vertices.clone(),
                colors: self.colors.clone(),
                tris: self.tris.clone(),
            };
        }
        let key = |v: [f32; 3]| {
            (
                (v[0] / cell).floor() as i32,
                (v[1] / cell).floor() as i32,
                (v[2] / cell).floor() as i32,
            )
        };
        let mut map: HashMap<(i32, i32, i32), u32> = HashMap::new();
        let mut remap = vec![0u32; self.vertices.len()];
        let mut psum: Vec<[f64; 3]> = Vec::new();
        let mut csum: Vec<[f64; 3]> = Vec::new();
        let mut cnt: Vec<u32> = Vec::new();
        for (i, v) in self.vertices.iter().enumerate() {
            let k = key(*v);
            let id = *map.entry(k).or_insert_with(|| {
                psum.push([0.0; 3]);
                csum.push([0.0; 3]);
                cnt.push(0);
                (psum.len() - 1) as u32
            });
            let u = id as usize;
            for d in 0..3 {
                psum[u][d] += v[d] as f64;
            }
            let col = self.colors[i];
            for d in 0..3 {
                csum[u][d] += col[d] as f64;
            }
            cnt[u] += 1;
            remap[i] = id;
        }
        let vertices: Vec<[f32; 3]> = psum
            .iter()
            .zip(&cnt)
            .map(|(p, &n)| {
                let n = n.max(1) as f64;
                [(p[0] / n) as f32, (p[1] / n) as f32, (p[2] / n) as f32]
            })
            .collect();
        let colors: Vec<[u8; 3]> = csum
            .iter()
            .zip(&cnt)
            .map(|(c, &n)| {
                let n = n.max(1) as f64;
                [
                    (c[0] / n) as u8,
                    (c[1] / n) as u8,
                    (c[2] / n) as u8,
                ]
            })
            .collect();
        let mut tris = Vec::with_capacity(self.tris.len());
        for t in &self.tris {
            let (a, b, c) = (remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]);
            if a != b && b != c && a != c {
                tris.push([a, b, c]);
            }
        }
        Mesh {
            vertices,
            colors,
            tris,
        }
    }
}

// Esquinas del cubo unidad y sus 12 aristas (pares de índices de esquina).
const CORNERS: [[i32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
const EDGES: [(usize, usize); 12] = [
    (0, 1), (1, 2), (2, 3), (3, 0), // cara inferior
    (4, 5), (5, 6), (6, 7), (7, 4), // cara superior
    (0, 4), (1, 5), (2, 6), (3, 7), // verticales
];

/// Posición en mundo (mm) de la esquina `c` de la celda (i,j,k).
fn corner_pos(origin: [f32; 3], voxel: f32, i: usize, j: usize, k: usize, c: usize) -> [f32; 3] {
    let off = CORNERS[c];
    [
        origin[0] + (i + off[0] as usize) as f32 * voxel,
        origin[1] + (j + off[1] as usize) as f32 * voxel,
        origin[2] + (k + off[2] as usize) as f32 * voxel,
    ]
}

/// Orienta las normales (sin signo) hacia la cámara que vio cada punto: la
/// superficie observada mira a la cámara (lado "fuera"). Con ello el campo con
/// signo es positivo delante y negativo dentro del objeto, y la isosuperficie
/// F=0 es una sola pared bien orientada. En un escaneo de 360° cada punto trae
/// su propia dirección de vista; sin ella se asume la cámara en el origen, lo
/// que en la cara de atrás del objeto invertiría el campo.
fn orient_normals(pos: &[[f32; 3]], views: &[[f32; 3]], nrm: &mut [[f32; 3]]) {
    for ((p, v), n) in pos.iter().zip(views).zip(nrm.iter_mut()) {
        let toward_cam = if v[0] != 0.0 || v[1] != 0.0 || v[2] != 0.0 { *v } else { [-p[0], -p[1], -p[2]] };
        if n[0] * toward_cam[0] + n[1] * toward_cam[1] + n[2] * toward_cam[2] < 0.0 {
            n[0] = -n[0];
            n[1] = -n[1];
            n[2] = -n[2];
        }
    }
}

/// Reconstruye una malla de la nube. `voxel` (mm) controla el detalle (menor =
/// más fino); `fill` engrosa el radio de influencia del campo MLS
/// (r = (1.5+fill)·voxel) → cierra huecos mayores a costa de algo de suavizado;
/// `smooth` = pasadas de suavizado laplaciano de la malla (0 = más anguloso, más =
/// más liso pero pierde detalle).
pub fn reconstruct(cloud: &PointCloud, voxel: f32, fill: u32, smooth: u32) -> Mesh {
    if cloud.points.len() < 8 {
        return Mesh::default();
    }
    let voxel = voxel.max(0.5);

    // Caja envolvente.
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in &cloud.points {
        let a = [p.x, p.y, p.z];
        for d in 0..3 {
            mn[d] = mn[d].min(a[d]);
            mx[d] = mx[d].max(a[d]);
        }
    }

    let pad = 2usize;
    let origin = [
        mn[0] - pad as f32 * voxel,
        mn[1] - pad as f32 * voxel,
        mn[2] - pad as f32 * voxel,
    ];
    let dim = |lo: f32, hi: f32| ((hi - lo) / voxel).ceil() as usize + 1 + 2 * pad;
    let nx = dim(mn[0], mx[0]).max(2);
    let ny = dim(mn[1], mx[1]).max(2);
    let nz = dim(mn[2], mx[2]).max(2);
    // Salvaguarda de memoria para nubes enormes / vóxel muy fino.
    if nx * ny * nz > 64_000_000 {
        return Mesh::default();
    }

    let idx = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
    let ng = nx * ny * nz;

    // Campo escalar CON SIGNO (MLS por splatting de planos tangentes). Cada punto
    // reparte en su vecindad (radio `r`) la distancia con signo a su plano
    // tangente, n·(x−p), ponderada por un núcleo gaussiano. F = Σw·sd / Σw es una
    // superficie de mínimos cuadrados que pasa POR los puntos: cruza el cero UNA
    // sola vez a lo largo de la normal → una sola pared (no la doble pared
    // arrugada de la ocupación binaria), y el solape de núcleos rellena huecos sin
    // inflar. W = Σw es la confianza: las celdas sin datos quedan "fuera" (+∞) y no
    // se mallan, así el fondo se recorta y las superficies abiertas no se envuelven
    // en una oblea. `fill` engrosa el radio → cierra huecos mayores.
    let r = (1.5 + fill as f32) * voxel;
    let sigma = r * 0.6;
    let inv2s2 = 1.0 / (2.0 * sigma * sigma);
    let r_norm = (2.5 * voxel).max(3.0);

    let pos: Vec<[f32; 3]> = cloud.points.iter().map(|p| [p.x, p.y, p.z]).collect();
    let views: Vec<[f32; 3]> = cloud.points.iter().map(|p| p.view).collect();
    let mut nrm = crate::scan::estimate_normals(&pos, r_norm);
    orient_normals(&pos, &views, &mut nrm);

    let mut fnum = vec![0f32; ng];
    let mut wsum = vec![0f32; ng];
    let mut psum = vec![[0f32; 3]; ng]; // posiciones reales (vértice data-driven)
    let mut csum = vec![[0f32; 3]; ng]; // color real acumulado
    let mut ccnt = vec![0f32; ng];
    let rv = (r / voxel).ceil() as i64;

    for (pi, p) in pos.iter().enumerate() {
        let n = nrm[pi];
        let gi = (((p[0] - origin[0]) / voxel).round() as i64).clamp(0, nx as i64 - 1);
        let gj = (((p[1] - origin[1]) / voxel).round() as i64).clamp(0, ny as i64 - 1);
        let gk = (((p[2] - origin[2]) / voxel).round() as i64).clamp(0, nz as i64 - 1);
        // Vóxel propio: color + posición real (colocación data-driven del vértice).
        let id0 = idx(gi as usize, gj as usize, gk as usize);
        psum[id0][0] += p[0];
        psum[id0][1] += p[1];
        psum[id0][2] += p[2];
        let c = cloud.points[pi].rgb;
        csum[id0][0] += c[0] as f32;
        csum[id0][1] += c[1] as f32;
        csum[id0][2] += c[2] as f32;
        ccnt[id0] += 1.0;
        // Splat del plano tangente con falloff gaussiano en el cubo de radio r.
        for dk in -rv..=rv {
            for dj in -rv..=rv {
                for di in -rv..=rv {
                    let (i, j, k) = (gi + di, gj + dj, gk + dk);
                    if i < 0
                        || j < 0
                        || k < 0
                        || i >= nx as i64
                        || j >= ny as i64
                        || k >= nz as i64
                    {
                        continue;
                    }
                    let cx = [
                        origin[0] + i as f32 * voxel,
                        origin[1] + j as f32 * voxel,
                        origin[2] + k as f32 * voxel,
                    ];
                    let dx = [cx[0] - p[0], cx[1] - p[1], cx[2] - p[2]];
                    let d2 = dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2];
                    if d2 > r * r {
                        continue;
                    }
                    let w = (-d2 * inv2s2).exp();
                    let sd = n[0] * dx[0] + n[1] * dx[1] + n[2] * dx[2];
                    let id = idx(i as usize, j as usize, k as usize);
                    fnum[id] += w * sd;
                    wsum[id] += w;
                }
            }
        }
    }

    // Campo final con signo; NaN = sin datos suficientes (desconocido). F<0 =
    // dentro del objeto. Lo desconocido no cuenta ni como dentro ni como fuera:
    // tratarlo como "fuera" hacía que el borde del soporte, a un radio `r` DETRÁS
    // de cada superficie, cambiara de signo y se mallara como una segunda pared
    // invertida pegada a la buena
    let wmin = 0.15f32;
    let mut field = vec![f32::NAN; ng];
    for id in 0..ng {
        if wsum[id] >= wmin {
            field[id] = fnum[id] / wsum[id];
        }
    }
    let iso = 0.0f32;

    // Un vértice por celda activa.
    let mut cell_vert: HashMap<(usize, usize, usize), u32> = HashMap::new();
    let mut mesh = Mesh::default();

    for k in 0..nz - 1 {
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let mut below = 0u8;
                let mut known = true;
                for (ci, off) in CORNERS.iter().enumerate() {
                    let f = field[idx(
                        i + off[0] as usize,
                        j + off[1] as usize,
                        k + off[2] as usize,
                    )];
                    if f.is_nan() {
                        known = false;
                        break;
                    }
                    if f < iso {
                        below |= 1 << ci;
                    }
                }
                // Solo celdas con las 8 esquinas conocidas: evita envolver el
                // vacío (fondo, borde de una superficie abierta, atrás del soporte)
                if !known || below == 0 || below == 0xFF {
                    continue;
                }

                // Vértice GUIADO POR DATOS: centroide de los puntos reales en los
                // vóxeles de esquina ocupados → el vértice cae sobre la superficie
                // escaneada (no en una rejilla difusa).
                let mut psum_c = [0f32; 3];
                let mut csum_c = [0f32; 3];
                let mut wsum_c = 0f32;
                for off in CORNERS.iter() {
                    let id = idx(
                        i + off[0] as usize,
                        j + off[1] as usize,
                        k + off[2] as usize,
                    );
                    if ccnt[id] > 0.0 {
                        for d in 0..3 {
                            psum_c[d] += psum[id][d];
                            csum_c[d] += csum[id][d];
                        }
                        wsum_c += ccnt[id];
                    }
                }
                let (vpos, rgb) = if wsum_c > 0.0 {
                    let inv = 1.0 / wsum_c;
                    (
                        [psum_c[0] * inv, psum_c[1] * inv, psum_c[2] * inv],
                        [
                            (csum_c[0] * inv) as u8,
                            (csum_c[1] * inv) as u8,
                            (csum_c[2] * inv) as u8,
                        ],
                    )
                } else {
                    // Celda activa sin puntos reales (zona rellenada por el campo):
                    // vértice en el cruce-cero interpolado sobre las 12 aristas →
                    // cae sobre la isosuperficie, no en el centro (evita facetado).
                    let mut acc = [0f32; 3];
                    let mut cnt = 0f32;
                    for &(a, b) in EDGES.iter() {
                        let ida = idx(
                            i + CORNERS[a][0] as usize,
                            j + CORNERS[a][1] as usize,
                            k + CORNERS[a][2] as usize,
                        );
                        let idb = idx(
                            i + CORNERS[b][0] as usize,
                            j + CORNERS[b][1] as usize,
                            k + CORNERS[b][2] as usize,
                        );
                        let (fa, fb) = (field[ida], field[idb]);
                        if (fa < iso) != (fb < iso) {
                            let t = (iso - fa) / (fb - fa);
                            let pa = corner_pos(origin, voxel, i, j, k, a);
                            let pb = corner_pos(origin, voxel, i, j, k, b);
                            for d in 0..3 {
                                acc[d] += pa[d] + t * (pb[d] - pa[d]);
                            }
                            cnt += 1.0;
                        }
                    }
                    let vpos = if cnt > 0.0 {
                        [acc[0] / cnt, acc[1] / cnt, acc[2] / cnt]
                    } else {
                        [
                            origin[0] + (i as f32 + 0.5) * voxel,
                            origin[1] + (j as f32 + 0.5) * voxel,
                            origin[2] + (k as f32 + 0.5) * voxel,
                        ]
                    };
                    (vpos, [200, 200, 200])
                };

                let vi = mesh.vertices.len() as u32;
                mesh.vertices.push(vpos);
                mesh.colors.push(rgb);
                cell_vert.insert((i, j, k), vi);
            }
        }
    }

    // Caras: por cada arista de rejilla con cambio de signo, conecta las 4
    // celdas que la rodean (todas deben ser activas, así que ambos extremos de
    // la arista son conocidos). `inside` = campo < iso.
    let inside = |i: usize, j: usize, k: usize| field[idx(i, j, k)] < iso;
    let gather = |cv: &HashMap<(usize, usize, usize), u32>,
                  cells: [(usize, usize, usize); 4]|
     -> Option<[u32; 4]> {
        Some([
            *cv.get(&cells[0])?,
            *cv.get(&cells[1])?,
            *cv.get(&cells[2])?,
            *cv.get(&cells[3])?,
        ])
    };

    // Aristas en X.
    for k in 1..nz - 1 {
        for j in 1..ny - 1 {
            for i in 0..nx - 1 {
                if inside(i, j, k) == inside(i + 1, j, k) {
                    continue;
                }
                let cells = [
                    (i, j - 1, k - 1),
                    (i, j, k - 1),
                    (i, j - 1, k),
                    (i, j, k),
                ];
                if let Some(q) = gather(&cell_vert, cells) {
                    push_quad(&mut mesh.tris, q, inside(i, j, k));
                }
            }
        }
    }
    // Aristas en Y.
    for k in 1..nz - 1 {
        for j in 0..ny - 1 {
            for i in 1..nx - 1 {
                if inside(i, j, k) == inside(i, j + 1, k) {
                    continue;
                }
                let cells = [
                    (i - 1, j, k - 1),
                    (i, j, k - 1),
                    (i - 1, j, k),
                    (i, j, k),
                ];
                if let Some(q) = gather(&cell_vert, cells) {
                    push_quad(&mut mesh.tris, q, inside(i, j + 1, k));
                }
            }
        }
    }
    // Aristas en Z.
    for k in 0..nz - 1 {
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                if inside(i, j, k) == inside(i, j, k + 1) {
                    continue;
                }
                let cells = [
                    (i - 1, j - 1, k),
                    (i, j - 1, k),
                    (i - 1, j, k),
                    (i, j, k),
                ];
                if let Some(q) = gather(&cell_vert, cells) {
                    push_quad(&mut mesh.tris, q, inside(i, j, k));
                }
            }
        }
    }

    // Suavizado laplaciano sobre la malla (suaviza sin inflar, a diferencia del
    // difuminado de campo que convertía el objeto en una piedra).
    laplacian_smooth(&mut mesh, smooth);
    mesh
}

/// Suavizado laplaciano: mueve cada vértice hacia la media de sus vecinos.
fn laplacian_smooth(mesh: &mut Mesh, iters: u32) {
    if iters == 0 || mesh.vertices.is_empty() {
        return;
    }
    let n = mesh.vertices.len();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for t in &mesh.tris {
        let e = [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
        for (a, b) in e {
            if !adj[a as usize].contains(&b) {
                adj[a as usize].push(b);
            }
            if !adj[b as usize].contains(&a) {
                adj[b as usize].push(a);
            }
        }
    }
    for _ in 0..iters {
        let mut np = mesh.vertices.clone();
        for v in 0..n {
            if adj[v].is_empty() {
                continue;
            }
            let mut s = [0f32; 3];
            for &nb in &adj[v] {
                let p = mesh.vertices[nb as usize];
                for d in 0..3 {
                    s[d] += p[d];
                }
            }
            let m = adj[v].len() as f32;
            for d in 0..3 {
                np[v][d] = mesh.vertices[v][d] * 0.5 + (s[d] / m) * 0.5;
            }
        }
        mesh.vertices = np;
    }
}

/// Emite un quad (como 2 triángulos) a partir de las 4 celdas [bl,br,tl,tr],
/// con el bucle bl→br→tr→tl. `flip` invierte el sentido para orientar la normal.
fn push_quad(tris: &mut Vec<[u32; 3]>, q: [u32; 4], flip: bool) {
    let loop_ = [q[0], q[1], q[3], q[2]];
    if flip {
        tris.push([loop_[0], loop_[1], loop_[2]]);
        tris.push([loop_[0], loop_[2], loop_[3]]);
    } else {
        tris.push([loop_[0], loop_[2], loop_[1]]);
        tris.push([loop_[0], loop_[3], loop_[2]]);
    }
}

// ---------------------------------------------------------------------------
// Exportadores
// ---------------------------------------------------------------------------

impl Mesh {
    /// OBJ (con color por vértice como extensión `v x y z r g b`).
    pub fn export_obj(&self, path: &Path) -> io::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "# malla generada por Orizon3D")?;
        for (v, c) in self.vertices.iter().zip(&self.colors) {
            writeln!(
                out,
                "v {} {} {} {} {} {}",
                v[0],
                v[1],
                v[2],
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0
            )?;
        }
        for t in &self.tris {
            writeln!(out, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
        }
        out.flush()
    }

    /// STL binario (geometría; sin color). Normales por geometría.
    pub fn export_stl(&self, path: &Path) -> io::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(&[0u8; 80])?; // cabecera
        out.write_all(&(self.tris.len() as u32).to_le_bytes())?;
        let wr = |out: &mut BufWriter<File>, v: [f32; 3]| -> io::Result<()> {
            out.write_all(&v[0].to_le_bytes())?;
            out.write_all(&v[1].to_le_bytes())?;
            out.write_all(&v[2].to_le_bytes())
        };
        for t in &self.tris {
            let a = self.vertices[t[0] as usize];
            let b = self.vertices[t[1] as usize];
            let c = self.vertices[t[2] as usize];
            let n = normal(a, b, c);
            wr(&mut out, n)?;
            wr(&mut out, a)?;
            wr(&mut out, b)?;
            wr(&mut out, c)?;
            out.write_all(&[0u8; 2])?; // atributo
        }
        out.flush()
    }

    /// PLY de malla (ASCII) con color por vértice y caras.
    pub fn export_ply(&self, path: &Path) -> io::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "ply")?;
        writeln!(out, "format ascii 1.0")?;
        writeln!(out, "comment generado por Orizon3D")?;
        writeln!(out, "element vertex {}", self.vertices.len())?;
        writeln!(out, "property float x")?;
        writeln!(out, "property float y")?;
        writeln!(out, "property float z")?;
        writeln!(out, "property uchar red")?;
        writeln!(out, "property uchar green")?;
        writeln!(out, "property uchar blue")?;
        writeln!(out, "element face {}", self.tris.len())?;
        writeln!(out, "property list uchar int vertex_indices")?;
        writeln!(out, "end_header")?;
        for (v, c) in self.vertices.iter().zip(&self.colors) {
            writeln!(out, "{} {} {} {} {} {}", v[0], v[1], v[2], c[0], c[1], c[2])?;
        }
        for t in &self.tris {
            writeln!(out, "3 {} {} {}", t[0], t[1], t[2])?;
        }
        out.flush()
    }
}

fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    [n[0] / len, n[1] / len, n[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointcloud::Point;

    /// Nube en forma de cubo macizo (muestreo en rejilla) → debe mallar.
    fn cube_cloud(side: f32, step: f32) -> PointCloud {
        let mut pts = Vec::new();
        let n = (side / step) as i32;
        for i in 0..=n {
            for j in 0..=n {
                for k in 0..=n {
                    // Solo la cáscara para simular una superficie escaneada.
                    if i == 0 || j == 0 || k == 0 || i == n || j == n || k == n {
                        pts.push(Point {
                            x: i as f32 * step,
                            y: j as f32 * step,
                            z: k as f32 * step,
                            rgb: [120, 130, 140],
                            view: [0.0; 3],
                        });
                    }
                }
            }
        }
        PointCloud {
            points: pts,
            has_color: true,
        }
    }

    #[test]
    fn meshes_a_cube() {
        let cloud = cube_cloud(40.0, 2.0);
        let mesh = reconstruct(&cloud, 3.0, 0, 1);
        assert!(!mesh.is_empty(), "no generó triángulos");
        assert!(mesh.vertices.len() >= 8);
        // Todos los índices válidos.
        for t in &mesh.tris {
            for &i in t {
                assert!((i as usize) < mesh.vertices.len());
            }
        }
    }

    #[test]
    fn decimate_reduces_and_keeps_valid_indices() {
        let cloud = cube_cloud(40.0, 2.0);
        let fine = reconstruct(&cloud, 2.0, 1, 1);
        let coarse = fine.decimate(8.0);
        assert!(!coarse.is_empty());
        assert!(coarse.vertices.len() <= fine.vertices.len());
        for t in &coarse.tris {
            for &i in t {
                assert!((i as usize) < coarse.vertices.len());
            }
        }
    }

    #[test]
    fn exports_have_valid_headers() {
        let cloud = cube_cloud(30.0, 2.0);
        let mesh = reconstruct(&cloud, 3.0, 0, 1);
        let dir = std::env::temp_dir();
        let obj = dir.join("revoscan_test.obj");
        let stl = dir.join("revoscan_test.stl");
        let ply = dir.join("revoscan_test_mesh.ply");
        mesh.export_obj(&obj).unwrap();
        mesh.export_stl(&stl).unwrap();
        mesh.export_ply(&ply).unwrap();

        let obj_txt = std::fs::read_to_string(&obj).unwrap();
        assert!(obj_txt.contains("\nv "));
        assert!(obj_txt.contains("\nf "));
        let ply_txt = std::fs::read_to_string(&ply).unwrap();
        assert!(ply_txt.starts_with("ply\n"));
        assert!(ply_txt.contains("element face"));
        let stl_len = std::fs::metadata(&stl).unwrap().len();
        assert_eq!(stl_len, 84 + 50 * mesh.tris.len() as u64);

        let _ = std::fs::remove_file(obj);
        let _ = std::fs::remove_file(stl);
        let _ = std::fs::remove_file(ply);
    }
}
