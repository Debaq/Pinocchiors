//! Compresión de primitivas con Draco (`KHR_draco_mesh_compression`).

use crate::geometry;
use crate::options::DracoOptions;
use converter_scene::{Primitive, VertexAttribute};
use draco_core::{
    DataType, EncoderBuffer, EncoderOptions, GeometryAttributeType, Mesh, MeshEncoder, PointAttribute,
};

pub(crate) const EXTENSION: &str = "KHR_draco_mesh_compression";

/// Primitiva comprimida: el bitstream y lo que el JSON necesita describir.
pub(crate) struct EncodedPrimitive {
    pub bytes: Vec<u8>,
    /// Nombre glTF del atributo → id único dentro del bitstream
    pub attribute_ids: Vec<(String, u32)>,
    /// Vértices tras decodificar (el `count` de los accessors de atributos)
    pub num_points: usize,
    pub num_indices: usize,
}

pub(crate) fn encode_primitive(prim: &Primitive, options: &DracoOptions) -> Result<EncodedPrimitive, String> {
    let num_points = geometry::vertex_count(prim);
    let indices = geometry::indices_u32(prim);
    if num_points == 0 || indices.len() < 3 {
        return Err("primitiva sin triángulos".into());
    }

    let mut mesh = Mesh::new();
    mesh.set_num_points(num_points);
    mesh.set_num_faces(indices.len() / 3);
    mesh.set_faces_from_flat_indices(&indices[..indices.len() / 3 * 3]);

    let mut encoder_options = EncoderOptions::new();
    encoder_options.set_compression_level(options.compression_level.min(10) as i32);

    let mut attribute_ids = Vec::new();
    for attr in &prim.attributes {
        let (name, kind, bits, data_type, components, bytes) = match attr {
            VertexAttribute::Positions(v) => (
                "POSITION".to_string(),
                GeometryAttributeType::Position,
                options.position_bits,
                DataType::Float32,
                3,
                f32_bytes(v.iter().flatten()),
            ),
            VertexAttribute::Normals(v) => (
                "NORMAL".to_string(),
                GeometryAttributeType::Normal,
                options.normal_bits,
                DataType::Float32,
                3,
                f32_bytes(v.iter().flatten()),
            ),
            VertexAttribute::Tangents(v) => (
                "TANGENT".to_string(),
                GeometryAttributeType::Generic,
                options.generic_bits,
                DataType::Float32,
                4,
                f32_bytes(v.iter().flatten()),
            ),
            VertexAttribute::TexCoords(set, v) => (
                format!("TEXCOORD_{set}"),
                GeometryAttributeType::TexCoord,
                options.texcoord_bits,
                DataType::Float32,
                2,
                f32_bytes(v.iter().flatten()),
            ),
            VertexAttribute::Colors(v) => (
                "COLOR_0".to_string(),
                GeometryAttributeType::Color,
                options.color_bits,
                DataType::Float32,
                4,
                f32_bytes(v.iter().flatten()),
            ),
            VertexAttribute::JointIndices(v) => (
                "JOINTS_0".to_string(),
                GeometryAttributeType::Generic,
                0,
                DataType::Uint16,
                4,
                v.iter().flatten().flat_map(|j| j.to_le_bytes()).collect(),
            ),
            VertexAttribute::JointWeights(v) => (
                "WEIGHTS_0".to_string(),
                GeometryAttributeType::Generic,
                options.generic_bits,
                DataType::Float32,
                4,
                f32_bytes(v.iter().flatten()),
            ),
        };
        let mut attribute = PointAttribute::new();
        attribute.init(kind, components, data_type, false, num_points);
        attribute.buffer_mut().write(0, &bytes);
        let id = mesh.add_attribute(attribute);
        // Los enteros (joints) se codifican sin pérdida
        if bits > 0 && data_type == DataType::Float32 {
            encoder_options.set_attribute_quantization(id, bits as i32);
        }
        attribute_ids.push((name, id as u32));
    }

    let mut encoder = MeshEncoder::new();
    encoder.set_mesh(mesh);
    let mut buffer = EncoderBuffer::new();
    let info = encoder
        .encode_with_info(&encoder_options, &mut buffer)
        .map_err(|e| format!("Draco: {e:?}"))?;
    Ok(EncodedPrimitive {
        bytes: buffer.data().to_vec(),
        attribute_ids,
        num_points: info.num_encoded_points,
        num_indices: info.num_encoded_faces * 3,
    })
}

fn f32_bytes<'a>(values: impl Iterator<Item = &'a f32>) -> Vec<u8> {
    values.flat_map(|f| f.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::IndexData;
    use draco_core::{DecoderBuffer, MeshDecoder};

    fn cube() -> Primitive {
        let positions: Vec<[f32; 3]> = (0..8)
            .map(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32])
            .collect();
        let normals = positions
            .iter()
            .map(|p| {
                let c = [p[0] - 0.5, p[1] - 0.5, p[2] - 0.5];
                let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                [c[0] / l, c[1] / l, c[2] / l]
            })
            .collect();
        let indices: Vec<u16> = vec![
            0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1,
            3, 5, 3, 7, 5,
        ];
        Primitive {
            attributes: vec![
                VertexAttribute::Positions(positions),
                VertexAttribute::Normals(normals),
                VertexAttribute::JointIndices(vec![[0, 1, 0, 0]; 8]),
            ],
            indices: Some(IndexData::U16(indices)),
            material: None,
        }
    }

    #[test]
    fn roundtrip_keeps_topology_and_positions() {
        let encoded = encode_primitive(&cube(), &DracoOptions::default()).unwrap();
        assert_eq!(encoded.num_indices, 36);
        assert_eq!(encoded.num_points, 8);
        let names: Vec<&str> = encoded.attribute_ids.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["POSITION", "NORMAL", "JOINTS_0"]);

        let mut decoded = Mesh::new();
        let mut buffer = DecoderBuffer::new(&encoded.bytes);
        MeshDecoder::new().decode(&mut buffer, &mut decoded).unwrap();
        assert_eq!(decoded.num_faces(), 12);
        let positions = decoded
            .named_attribute(GeometryAttributeType::Position)
            .unwrap()
            .read_f32s(decoded.num_points(), 3);
        // Cada esquina del cubo sigue presente (cuantizada)
        for i in 0..8 {
            let p = [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32];
            assert!(
                positions.chunks(3).any(|q| (0..3).all(|k| (q[k] - p[k]).abs() < 1e-3)),
                "falta la esquina {p:?}"
            );
        }
    }

    #[test]
    fn empty_primitive_is_rejected() {
        let prim = Primitive { attributes: vec![], indices: None, material: None };
        assert!(encode_primitive(&prim, &DracoOptions::default()).is_err());
    }
}
