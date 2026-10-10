use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use serde_json::{Value, json};

use crate::character::{
    AnimationClip, AssembledCharacter, BoneTrack, CharacterPrimitive,
};

pub struct GlbExporter;

impl GlbExporter {
    pub fn export(character: &AssembledCharacter, output_path: &Path) -> Result<()> {
        let mut bin = Vec::<u8>::new();
        let mut views = Vec::<Value>::new();
        let mut accessors = Vec::<Value>::new();

        let ibm_accessor = push_inverse_bind_matrices(
            &mut bin,
            &mut views,
            &mut accessors,
            &character
                .bones
                .iter()
                .map(|b| wow_to_blender_mat(Mat4::from_cols_array_2d(&b.inverse_bind_matrix)).to_cols_array())
                .collect::<Vec<_>>(),
        );

        let mut meshes = Vec::new();
        for primitive in &character.primitives {
            meshes.push(build_mesh_primitive(
                primitive,
                &character.materials,
                &mut bin,
                &mut views,
                &mut accessors,
            )?);
        }

        let mut image_defs = Vec::<Value>::new();
        let mut texture_defs = Vec::<Value>::new();
        let mut material_defs = Vec::<Value>::new();
        for material in &character.materials {
            let png = if let Some((w, h, rgba)) = character.embedded_textures.get(&material.name) {
                encode_png(*w, *h, rgba)
            } else {
                material_debug_png(&material.name)
            };
            let slice = append_u8(&mut bin, &png);
            let view = push_view(&mut views, slice);
            let image_index = image_defs.len();
            image_defs.push(json!({
                "name": format!("{}_img", material.name),
                "mimeType": "image/png",
                "bufferView": view
            }));
            texture_defs.push(json!({
                "name": format!("{}_tex", material.name),
                "source": image_index
            }));
            material_defs.push(json!({
                "name": material.name,
                "pbrMetallicRoughness": {
                    "baseColorTexture": { "index": image_index },
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0
                }
            }));
        }

        let bone_base = character.nodes.len();
        let mut parent_map = Vec::<Option<usize>>::new();
        let mut node_defs = Vec::<Value>::new();
        for node in &character.nodes {
            parent_map.push(node.parent_index);
            let mut out = json!({
                "name": node.name,
                "translation": wow_to_blender(node.translation),
                "rotation": node.rotation,
                "scale": node.scale,
            });
            if let Some(mesh) = node.attached_primitive {
                out["mesh"] = json!(mesh);
                out["skin"] = json!(0);
            }
            node_defs.push(out);
        }
        for (idx, bone) in character.bones.iter().enumerate() {
            let parent = bone.parent_index.map(|p| bone_base + p);
            parent_map.push(parent);
            node_defs.push(json!({
                "name": bone.name,
                "translation": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
                "extras": { "boneIndex": idx }
            }));
        }
        let mut children_map = vec![Vec::<usize>::new(); node_defs.len()];
        for (idx, parent) in parent_map.iter().enumerate() {
            if let Some(parent) = parent
                && *parent < children_map.len()
            {
                children_map[*parent].push(idx);
            }
        }
        for i in 0..node_defs.len() {
            if !children_map[i].is_empty() {
                node_defs[i]["children"] = json!(children_map[i]);
            }
        }

        let mut animations = Vec::new();
        for clip in &character.animations {
            animations.push(build_animation(
                clip,
                &mut bin,
                &mut views,
                &mut accessors,
                character.nodes.len(),
                character.bones.len(),
            )?);
        }

        let mut scene_roots = Vec::<usize>::new();
        for (idx, node) in character.nodes.iter().enumerate() {
            if node.parent_index.is_none() {
                scene_roots.push(idx);
            }
        }
        for idx in 0..character.bones.len() {
            if character.bones[idx].parent_index.is_none() {
                scene_roots.push(bone_base + idx);
            }
        }

        let mut joints = Vec::new();
        for i in 0..character.bones.len() {
            joints.push(bone_base + i);
        }

        let gltf = json!({
            "asset": { "version": "2.0", "generator": "wowmodelexport-converter" },
            "scenes": [{ "nodes": scene_roots }],
            "scene": 0,
            "nodes": node_defs,
            "meshes": meshes,
            "materials": material_defs,
            "images": image_defs,
            "textures": texture_defs,
            "skins": [{
                "name": "CharacterRig",
                "inverseBindMatrices": ibm_accessor,
                "joints": joints,
                "skeleton": if character.bones.is_empty() { 0 } else { bone_base }
            }],
            "animations": animations,
            "buffers": [{ "byteLength": bin.len() }],
            "bufferViews": views,
            "accessors": accessors
        });

        write_glb(output_path, &gltf, &bin)
    }
}

pub fn wow_to_blender(position: [f32; 3]) -> [f32; 3] {
    [position[1], position[0], position[2]]
}

fn wow_to_blender_mat(input: Mat4) -> Mat4 {
    let basis = Mat4::from_cols(
        Vec3::Y.extend(0.0),
        Vec3::X.extend(0.0),
        Vec3::Z.extend(0.0),
        Vec3::ZERO.extend(1.0),
    );
    basis * input
}

fn build_mesh_primitive(
    primitive: &CharacterPrimitive,
    materials: &[crate::character::CharacterMaterial],
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> Result<Value> {
    let pos = primitive.vertices.iter().map(|v| wow_to_blender(v.position)).collect::<Vec<_>>();
    let norm = primitive.vertices.iter().map(|v| wow_to_blender(v.normal)).collect::<Vec<_>>();
    let uv = primitive.vertices.iter().map(|v| v.uv).collect::<Vec<_>>();
    let joints = primitive.vertices.iter().map(|v| v.joints).collect::<Vec<_>>();
    let weights = primitive.vertices.iter().map(|v| v.weights).collect::<Vec<_>>();
    let idx = primitive.indices.clone();

    let idx_slice = append_u32(bin, &idx);
    let pos_slice = append_f32x3(bin, &pos);
    let norm_slice = append_f32x3(bin, &norm);
    let uv_slice = append_f32x2(bin, &uv);
    let joint_slice = append_u16x4(bin, &joints);
    let weight_slice = append_f32x4(bin, &weights);

    let idx_accessor = push_accessor_u32_scalar(accessors, views, idx_slice, idx.len());
    let pos_accessor = push_accessor_f32_vec3(accessors, views, pos_slice, pos);
    let norm_accessor = push_accessor_f32_vec3(accessors, views, norm_slice, norm);
    let uv_accessor = push_accessor_f32_vec2(accessors, views, uv_slice, uv.len());
    let joint_accessor = push_accessor_u16_vec4(accessors, views, joint_slice, joints.len());
    let weight_accessor = push_accessor_f32_vec4(accessors, views, weight_slice, weights.len());

    let material = materials
        .iter()
        .position(|m| m.name == primitive.material)
        .unwrap_or(0);

    Ok(json!({
        "name": primitive.name,
        "primitives": [{
            "attributes": {
                "POSITION": pos_accessor,
                "NORMAL": norm_accessor,
                "TEXCOORD_0": uv_accessor,
                "JOINTS_0": joint_accessor,
                "WEIGHTS_0": weight_accessor
            },
            "indices": idx_accessor,
            "material": material
        }],
        "extras": {
            "geoset": primitive.geoset_id
        }
    }))
}

fn build_animation(
    clip: &AnimationClip,
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    node_count: usize,
    _bone_count: usize,
) -> Result<Value> {
    let mut samplers = Vec::<Value>::new();
    let mut channels = Vec::<Value>::new();
    for track in &clip.tracks {
        append_track_sampler(
            track,
            node_count + track.bone_index,
            "translation",
            &track.times_sec,
            &track.translations,
            bin,
            views,
            accessors,
            &mut samplers,
            &mut channels,
        );
        append_track_sampler(
            track,
            node_count + track.bone_index,
            "rotation",
            &track.times_sec,
            &track.rotations,
            bin,
            views,
            accessors,
            &mut samplers,
            &mut channels,
        );
        append_track_sampler(
            track,
            node_count + track.bone_index,
            "scale",
            &track.times_sec,
            &track.scales,
            bin,
            views,
            accessors,
            &mut samplers,
            &mut channels,
        );
    }
    Ok(json!({
        "name": clip.name,
        "samplers": samplers,
        "channels": channels
    }))
}

fn append_track_sampler<const N: usize>(
    _track: &BoneTrack,
    target_node: usize,
    path: &str,
    times: &[f32],
    values: &[[f32; N]],
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    samplers: &mut Vec<Value>,
    channels: &mut Vec<Value>,
) {
    let t_slice = append_f32(bin, times);
    let v_slice = append_f32x(bin, values);
    let input = push_accessor_f32_scalar(accessors, views, t_slice, times.to_vec());
    let output = push_accessor_f32(accessors, views, v_slice, N, values.len());
    let sampler_index = samplers.len();
    samplers.push(json!({
        "input": input,
        "output": output,
        "interpolation": "LINEAR"
    }));
    channels.push(json!({
        "sampler": sampler_index,
        "target": {
            "node": target_node,
            "path": path
        }
    }));
}

#[derive(Copy, Clone)]
struct DataSlice {
    byte_offset: usize,
    byte_length: usize,
}

fn align4(buffer: &mut Vec<u8>) {
    while !buffer.len().is_multiple_of(4) {
        buffer.push(0);
    }
}

fn append_u8(buffer: &mut Vec<u8>, values: &[u8]) -> DataSlice {
    align4(buffer);
    let offset = buffer.len();
    buffer.extend_from_slice(values);
    DataSlice {
        byte_offset: offset,
        byte_length: values.len(),
    }
}

fn append_u32(buffer: &mut Vec<u8>, values: &[u32]) -> DataSlice {
    align4(buffer);
    let offset = buffer.len();
    for v in values {
        buffer.extend_from_slice(&v.to_le_bytes());
    }
    DataSlice {
        byte_offset: offset,
        byte_length: values.len() * 4,
    }
}

fn append_u16x4(buffer: &mut Vec<u8>, values: &[[u16; 4]]) -> DataSlice {
    align4(buffer);
    let offset = buffer.len();
    for v in values {
        for n in v {
            buffer.extend_from_slice(&n.to_le_bytes());
        }
    }
    DataSlice {
        byte_offset: offset,
        byte_length: values.len() * 8,
    }
}

fn append_f32(buffer: &mut Vec<u8>, values: &[f32]) -> DataSlice {
    align4(buffer);
    let offset = buffer.len();
    for v in values {
        buffer.extend_from_slice(&v.to_le_bytes());
    }
    DataSlice {
        byte_offset: offset,
        byte_length: values.len() * 4,
    }
}

fn append_f32x<const N: usize>(buffer: &mut Vec<u8>, values: &[[f32; N]]) -> DataSlice {
    align4(buffer);
    let offset = buffer.len();
    for v in values {
        for n in v {
            buffer.extend_from_slice(&n.to_le_bytes());
        }
    }
    DataSlice {
        byte_offset: offset,
        byte_length: values.len() * N * 4,
    }
}

fn append_f32x2(buffer: &mut Vec<u8>, values: &[[f32; 2]]) -> DataSlice {
    append_f32x(buffer, values)
}

fn append_f32x3(buffer: &mut Vec<u8>, values: &[[f32; 3]]) -> DataSlice {
    append_f32x(buffer, values)
}

fn append_f32x4(buffer: &mut Vec<u8>, values: &[[f32; 4]]) -> DataSlice {
    append_f32x(buffer, values)
}

fn push_view(views: &mut Vec<Value>, slice: DataSlice) -> usize {
    views.push(json!({
        "buffer": 0,
        "byteOffset": slice.byte_offset,
        "byteLength": slice.byte_length
    }));
    views.len() - 1
}

fn push_accessor_u32_scalar(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    count: usize,
) -> usize {
    let view = push_view(views, slice);
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5125,
        "count": count,
        "type": "SCALAR"
    }));
    accessors.len() - 1
}

fn push_accessor_u16_vec4(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    count: usize,
) -> usize {
    let view = push_view(views, slice);
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5123,
        "count": count,
        "type": "VEC4"
    }));
    accessors.len() - 1
}

fn push_accessor_f32_scalar(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    values: Vec<f32>,
) -> usize {
    let view = push_view(views, slice);
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5126,
        "count": values.len(),
        "type": "SCALAR",
        "min": [min],
        "max": [max]
    }));
    accessors.len() - 1
}

fn push_accessor_f32(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    lanes: usize,
    count: usize,
) -> usize {
    let view = push_view(views, slice);
    let kind = match lanes {
        2 => "VEC2",
        3 => "VEC3",
        4 => "VEC4",
        16 => "MAT4",
        _ => "SCALAR",
    };
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5126,
        "count": count,
        "type": kind
    }));
    accessors.len() - 1
}

fn push_accessor_f32_vec2(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    count: usize,
) -> usize {
    push_accessor_f32(accessors, views, slice, 2, count)
}

fn push_accessor_f32_vec3(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    values: Vec<[f32; 3]>,
) -> usize {
    let view = push_view(views, slice);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in &values {
        for i in 0..3 {
            min[i] = min[i].min(v[i]);
            max[i] = max[i].max(v[i]);
        }
    }
    accessors.push(json!({
        "bufferView": view,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC3",
        "min": min,
        "max": max
    }));
    accessors.len() - 1
}

fn push_accessor_f32_vec4(
    accessors: &mut Vec<Value>,
    views: &mut Vec<Value>,
    slice: DataSlice,
    count: usize,
) -> usize {
    push_accessor_f32(accessors, views, slice, 4, count)
}

fn push_inverse_bind_matrices(
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    matrices: &[[f32; 16]],
) -> usize {
    let slice = append_f32x(bin, matrices);
    push_accessor_f32(accessors, views, slice, 16, matrices.len())
}

fn material_debug_png(seed: &str) -> Vec<u8> {
    let hash = seed
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(33).wrapping_add(u32::from(b)));
    let color = [
        (hash & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        255u8,
    ];

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(&color);
    }
    out
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(rgba);
    }
    out
}

fn write_glb(path: &Path, gltf: &Value, bin: &[u8]) -> Result<()> {
    let mut json_bytes = serde_json::to_vec(gltf).context("serialize glTF JSON")?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }

    let mut bin_bytes = bin.to_vec();
    while !bin_bytes.len().is_multiple_of(4) {
        bin_bytes.push(0);
    }

    let total_len = 12 + 8 + json_bytes.len() + 8 + bin_bytes.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&0x46546C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E4942u32.to_le_bytes());
    out.extend_from_slice(&bin_bytes);

    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_transform_swaps_xy() {
        assert_eq!(wow_to_blender([1.0, 2.0, 3.0]), [2.0, 1.0, 3.0]);
    }
}
