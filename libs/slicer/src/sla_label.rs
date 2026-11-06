//! Nombre en la balsa (SLA16 de `PLAN_SLA.md`): una pestaña pegada al frente de la balsa,
//! del mismo grosor, con el texto en relieve encima (placa, resina, fecha…).
//!
//! En relieve y no grabado: las letras hundidas encierran aire (cada «O» sería una ventosa
//! contra el film); en relieve se apoyan enteras en la pestaña. Sale como triángulos, así
//! se lamina como los soportes y se ve en el visor. Fuente propia de 5 × 7 en mayúsculas;
//! las letras con tilde pasan a la letra sola.

use crate::sla::ResinPrinter;
use crate::sla_supports::Tri;

/// Alto del relieve (mm), margen de la pestaña alrededor del texto (mm) y cuánto se mete en la balsa.
const EMBOSS: f32 = 0.5;
const MARGIN: f32 = 1.0;
const OVERLAP: f32 = 1.0;

/// Una letra lista para imprimir: los ojos cerrados (A, B, D, O, P, R, 0, 8…) se abren con
/// un paso de un píxel hacia afuera, como un esténcil: si no, cada ojo sería una ventosa
/// mientras se imprime el relieve.
fn glyph(c: char) -> [u8; 7] {
    stencil(shape(c))
}

fn stencil(mut g: [u8; 7]) -> [u8; 7] {
    // Grilla de 9 × 7 con un borde vacío alrededor.
    let on = |g: &[u8; 7], x: i32, y: i32| (0..5).contains(&x) && (0..7).contains(&y) && g[y as usize] & (1 << (4 - x)) != 0;
    loop {
        // Aire de afuera: inundar desde el borde.
        let mut out = [[false; 7]; 9];
        let mut stack = vec![(-1i32, -1i32)];
        while let Some((x, y)) = stack.pop() {
            if !(-1..6).contains(&x) || !(-1..8).contains(&y) || out[(y + 1) as usize][(x + 1) as usize] || on(&g, x, y) {
                continue;
            }
            out[(y + 1) as usize][(x + 1) as usize] = true;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        let outside = |x: i32, y: i32| out[(y + 1) as usize][(x + 1) as usize];
        // Un píxel de la letra entre un ojo y el afuera: el de más arriba y más al centro.
        let open = (0..7)
            .flat_map(|y| (0..5).map(move |x| (x, y)))
            .filter(|&(x, y)| on(&g, x, y))
            .filter(|&(x, y)| {
                let n = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];
                let eye = n.iter().any(|&(a, b)| (0..5).contains(&a) && (0..7).contains(&b) && !on(&g, a, b) && !outside(a, b));
                eye && n.iter().any(|&(a, b)| outside(a, b))
            })
            .min_by_key(|&(x, y)| (y, (x - 2).abs()));
        match open {
            Some((x, y)) => g[y as usize] &= !(1 << (4 - x)),
            None => return g,
        }
    }
}

/// La forma de cada letra: 7 filas de 5 bits (el más alto a la izquierda), de arriba hacia abajo.
fn shape(c: char) -> [u8; 7] {
    let rows: [&str; 7] = match c {
        'A' => ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
        'B' => ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
        'C' => ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
        'D' => ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
        'E' => ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'F' => ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
        'G' => ["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
        'H' => ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
        'I' => ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
        'J' => ["00111", "00010", "00010", "00010", "00010", "10010", "01100"],
        'K' => ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
        'L' => ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
        'M' => ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
        'N' => ["10001", "10001", "11001", "10101", "10011", "10001", "10001"],
        'O' => ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
        'P' => ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
        'Q' => ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
        'R' => ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'S' => ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        'U' => ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
        'V' => ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
        'W' => ["10001", "10001", "10001", "10101", "10101", "10101", "01010"],
        'X' => ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
        'Y' => ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
        'Z' => ["11111", "00001", "00010", "00100", "01000", "10000", "11111"],
        '0' => ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
        '1' => ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
        '2' => ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
        '3' => ["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
        '4' => ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
        '5' => ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
        '6' => ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
        '7' => ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
        '8' => ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
        '9' => ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
        ' ' => ["00000"; 7],
        '-' => ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
        '.' => ["00000", "00000", "00000", "00000", "00000", "01100", "01100"],
        ',' => ["00000", "00000", "00000", "00000", "01100", "00100", "01000"],
        ':' => ["00000", "01100", "01100", "00000", "01100", "01100", "00000"],
        '/' => ["00000", "00001", "00010", "00100", "01000", "10000", "00000"],
        '·' => ["00000", "00000", "00000", "01100", "01100", "00000", "00000"],
        '#' => ["01010", "01010", "11111", "01010", "11111", "01010", "01010"],
        '(' => ["00010", "00100", "01000", "01000", "01000", "00100", "00010"],
        ')' => ["01000", "00100", "00010", "00010", "00010", "00100", "01000"],
        '%' => ["11000", "11001", "00010", "00100", "01000", "10011", "00011"],
        '+' => ["00000", "00100", "00100", "11111", "00100", "00100", "00000"],
        '_' => ["00000", "00000", "00000", "00000", "00000", "00000", "11111"],
        _ => ["01110", "10001", "00001", "00010", "00100", "00000", "00100"],
    };
    rows.map(|r| u8::from_str_radix(r, 2).unwrap_or(0))
}

/// El texto como lo puede escribir la fuente: mayúsculas, sin tildes.
pub fn normalize(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'A',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'O',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            'ñ' | 'Ñ' => 'N',
            'ç' | 'Ç' => 'C',
            '\t' | '\n' | '\r' => ' ',
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

/// Dónde quedó la etiqueta.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub triangles: Vec<Tri>,
    /// El texto que entró (puede venir cortado si no cabía en la pantalla).
    pub text: String,
    /// Caja XY de la pestaña.
    pub lo: [f32; 2],
    pub hi: [f32; 2],
}

/// La pestaña con el texto, delante de la balsa (caja XY `raft_lo`–`raft_hi`, grosor
/// `thickness`) o detrás si delante no cabe. Letras de 7 × `cell` mm de alto. `None` si
/// no hay texto o no cabe en ningún lado.
pub fn raft_label(text: &str, raft_lo: [f32; 2], raft_hi: [f32; 2], thickness: f32, cell: f32, printer: &ResinPrinter) -> Option<Label> {
    let mut chars: Vec<char> = normalize(text).chars().collect();
    let cell = cell.max(0.1);
    let edge = 0.5;
    // Cortar lo que no entre a lo ancho de la pantalla.
    let fit = ((printer.bed_x - 2.0 * edge - 2.0 * MARGIN + cell) / (6.0 * cell)).floor() as usize;
    chars.truncate(fit);
    while chars.last() == Some(&' ') {
        chars.pop();
    }
    if chars.is_empty() {
        return None;
    }
    let text_w = chars.len() as f32 * 6.0 * cell - cell;
    let (tab_w, tab_h) = (text_w + 2.0 * MARGIN, 7.0 * cell + 2.0 * MARGIN);
    let x0 = raft_lo[0].min(printer.bed_x - edge - tab_w).max(edge);
    // Delante (y más chica); si no cabe, detrás.
    let y0 = if raft_lo[1] + OVERLAP - tab_h >= edge {
        raft_lo[1] + OVERLAP - tab_h
    } else if raft_hi[1] - OVERLAP + tab_h <= printer.bed_y - edge {
        raft_hi[1] - OVERLAP
    } else {
        return None;
    };
    let mut tris = Vec::new();
    cuboid(&mut tris, [x0, y0, 0.0], [x0 + tab_w, y0 + tab_h, thickness]);
    // Texto: una caja por cada tramo seguido de píxeles de cada fila (de arriba, y más grande, hacia abajo).
    let (tx, ty) = (x0 + MARGIN, y0 + MARGIN);
    let (z0, z1) = (thickness - 0.05, thickness + EMBOSS);
    for r in 0..7 {
        let bits: Vec<bool> = chars.iter().flat_map(|&c| {
            let row = glyph(c)[r];
            (0..6).map(move |k| k < 5 && row & (1 << (4 - k)) != 0)
        }).collect();
        let y = ty + (6 - r) as f32 * cell;
        let mut k = 0;
        while k < bits.len() {
            if !bits[k] {
                k += 1;
                continue;
            }
            let a = k;
            while k < bits.len() && bits[k] {
                k += 1;
            }
            cuboid(&mut tris, [tx + a as f32 * cell, y, z0], [tx + k as f32 * cell, y + cell, z1]);
        }
    }
    Some(Label { triangles: tris, text: chars.into_iter().collect(), lo: [x0, y0], hi: [x0 + tab_w, y0 + tab_h] })
}

/// Caja entre dos esquinas, con las caras hacia afuera.
fn cuboid(out: &mut Vec<Tri>, a: [f32; 3], b: [f32; 3]) {
    let p = |i: usize| [if i & 1 == 0 { a[0] } else { b[0] }, if i & 2 == 0 { a[1] } else { b[1] }, if i & 4 == 0 { a[2] } else { b[2] }];
    // Cada cara: cuatro esquinas en orden antihorario visto desde afuera.
    for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        out.push([p(f[0]), p(f[1]), p(f[2])]);
        out.push([p(f[0]), p(f[2]), p(f[3])]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::{slice, LD_002H};
    use crate::sla_check::check;

    #[test]
    fn caja_con_las_caras_hacia_afuera() {
        let mut t = Vec::new();
        cuboid(&mut t, [0.0; 3], [1.0, 2.0, 3.0]);
        // Volumen con signo (teorema de la divergencia): positivo = caras hacia afuera.
        let v: f32 = t
            .iter()
            .map(|t| {
                let [a, b, c] = *t;
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum();
        assert!((v - 6.0).abs() < 1e-4, "{v}");
    }

    #[test]
    fn letras_sin_ojos_cerrados() {
        // La «O» queda abierta arriba; la «I» no cambia.
        assert_eq!(glyph('O')[0], 0b01010);
        assert_eq!(glyph('D')[0], 0b11010, "la D también (su esquina de arriba a la izquierda está encendida)");
        assert_eq!(glyph('I'), shape('I'));
        assert_eq!(glyph('8').iter().zip(shape('8')).filter(|(a, b)| *a != b).count(), 2, "dos ojos, dos pasos");
    }

    #[test]
    fn texto_normalizado() {
        assert_eq!(normalize(" Placa 1 · Resina estándar, ñandú "), "PLACA 1 · RESINA ESTANDAR, NANDU");
    }

    #[test]
    fn etiqueta_delante_de_la_balsa_sin_islas_ni_huecos() {
        let l = raft_label("P1 ABS 2026-11-06 #8 BQ", [20.0, 30.0], [60.0, 70.0], 1.0, 0.35, &LD_002H).unwrap();
        assert_eq!(l.text, "P1 ABS 2026-11-06 #8 BQ");
        assert!(l.hi[1] - 30.0 - OVERLAP < 1e-4 && l.lo[1] < 30.0 && l.lo[0] == 20.0, "{l:?}");
        let layers = slice(&l.triangles, &LD_002H, 0.05).unwrap();
        assert_eq!(layers.len(), 30);
        let r = check(&layers, &LD_002H, 0.05);
        assert!(r.islands.is_empty() && r.cavities.is_empty(), "{:?} {:?}", r.islands, r.cavities);
        // Encima de la pestaña solo el texto: bastante menos que la pestaña.
        assert!(layers[25].area() * 3 < layers[5].area());
        assert!(layers[25].area() > 0);
    }

    #[test]
    fn largo_se_corta_y_si_no_hay_lugar_va_detras() {
        let long = "X".repeat(200);
        let l = raft_label(&long, [5.0, 3.0], [60.0, 70.0], 1.0, 0.35, &LD_002H).unwrap();
        assert!(l.text.len() < 200 && l.hi[0] <= LD_002H.bed_x - 0.5);
        // Delante no hay lugar (la balsa empieza en y = 3): detrás.
        assert!(l.lo[1] > 60.0, "{l:?}");
        assert!(raft_label("   ", [5.0, 30.0], [60.0, 70.0], 1.0, 0.35, &LD_002H).is_none());
    }
}
