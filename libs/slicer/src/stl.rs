use std::io::Write;
use std::path::Path;

/// Escribe triángulos (mm) como STL binario.
pub fn write_binary_stl(path: &Path, triangles: &[[[f32; 3]; 3]]) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(84 + triangles.len() * 50);
    let mut header = [0u8; 80];
    let tag = b"pinocchio-slicer";
    header[..tag.len()].copy_from_slice(tag);
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for t in triangles {
        let n = normal(t);
        for c in n.iter().chain(t.iter().flatten()) {
            buf.extend_from_slice(&c.to_le_bytes());
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&buf)
}

fn normal(t: &[[f32; 3]; 3]) -> [f32; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 0.0] }
}
