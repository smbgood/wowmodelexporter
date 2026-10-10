use std::collections::BTreeMap;

use anyhow::{Result, bail};
use glam::{Mat4, Quat, Vec3};
use serde::Serialize;

use crate::wow::m2::{M2Animation, M2Model, M2Vertex};
use crate::wow::types::{CharacterSourceData, EquipmentRecipe, ModelAsset};

#[derive(Debug, Clone, Serialize)]
pub struct Bone {
    pub name: String,
    pub parent_index: Option<usize>,
    pub inverse_bind_matrix: [[f32; 4]; 4],
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterPrimitive {
    pub name: String,
    pub geoset_id: Option<u16>,
    pub material: String,
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterNode {
    pub name: String,
    pub parent_index: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub attached_primitive: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterMaterial {
    pub name: String,
    pub texture_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoneTrack {
    pub bone_index: usize,
    pub times_sec: Vec<f32>,
    pub translations: Vec<[f32; 3]>,
    pub rotations: Vec<[f32; 4]>,
    pub scales: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnimationClip {
    pub name: String,
    pub duration_sec: f32,
    pub tracks: Vec<BoneTrack>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssembledCharacter {
    pub id: String,
    pub geosets: Vec<u16>,
    pub body_texture_layers: Vec<String>,
    pub equipment: BTreeMap<String, EquipmentRecipe>,
    pub bones: Vec<Bone>,
    pub materials: Vec<CharacterMaterial>,
    pub embedded_textures: BTreeMap<String, (u32, u32, Vec<u8>)>,
    pub primitives: Vec<CharacterPrimitive>,
    pub nodes: Vec<CharacterNode>,
    pub animations: Vec<AnimationClip>,
}

pub struct CharacterAssembler;

impl CharacterAssembler {
    pub fn assemble(source: CharacterSourceData) -> Result<AssembledCharacter> {
        if source.base_model.model.vertices.is_empty() {
            bail!("base model has no vertices: {}", source.base_model.model_path);
        }

        let skeleton = build_skeleton(&source.base_model.model);
        let mut materials = vec![CharacterMaterial {
            name: "BodyMaterial".to_string(),
            texture_path: source.appearance.body_texture_layers.first().cloned(),
        }];
        let mut embedded_textures = BTreeMap::new();
        if let Some(body) = source.composited_body_rgba.clone() {
            embedded_textures.insert("BodyMaterial".to_string(), body);
        }

        let mut primitives = vec![build_body_primitive(
            &source.base_model,
            &source.appearance.geoset_ids,
            "BodyMaterial".to_string(),
        )?];
        let mut nodes = vec![CharacterNode {
            name: format!("{}BodyNode", source.race_sex_key()),
            parent_index: None,
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            attached_primitive: Some(0),
        }];

        for (slot, recipe) in &source.equipment {
            if let Some(asset) = source.equipment_models.get(slot) {
                let material_name = format!("{slot}_material");
                materials.push(CharacterMaterial {
                    name: material_name.clone(),
                    texture_path: recipe.texture_path.clone(),
                });
                if let Some(texture) = source.equipment_textures_rgba.get(slot) {
                    embedded_textures.insert(material_name.clone(), texture.clone());
                }
                let primitive_index = primitives.len();
                primitives.push(build_equipment_primitive(asset, material_name)?);
                nodes.push(CharacterNode {
                    name: format!("{slot}_attachment"),
                    parent_index: attachment_parent_index(&recipe.attachment, &skeleton),
                    translation: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    attached_primitive: Some(primitive_index),
                });
            }
        }

        let animations = build_animations(&source.base_model.model, &source.requested_animations);
        Ok(AssembledCharacter {
            id: source.race_sex_key(),
            geosets: source.appearance.geoset_ids,
            body_texture_layers: source.appearance.body_texture_layers,
            equipment: source.equipment,
            bones: skeleton,
            materials,
            embedded_textures,
            primitives,
            nodes,
            animations,
        })
    }
}

fn build_skeleton(model: &M2Model) -> Vec<Bone> {
    if model.bones.is_empty() {
        return vec![Bone {
            name: "Root".to_string(),
            parent_index: None,
            inverse_bind_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        }];
    }
    model
        .bones
        .iter()
        .enumerate()
        .map(|(idx, bone)| {
            let bind = Mat4::from_translation(Vec3::new(bone.pivot[0], bone.pivot[1], bone.pivot[2]));
            Bone {
                name: format!("Bone_{idx}_key{}", bone.key_bone_id),
                parent_index: if bone.parent_index >= 0 {
                    Some(bone.parent_index as usize)
                } else {
                    None
                },
                inverse_bind_matrix: bind.inverse().to_cols_array_2d(),
            }
        })
        .collect()
}

fn build_body_primitive(
    asset: &ModelAsset,
    enabled_geosets: &[u16],
    material: String,
) -> Result<CharacterPrimitive> {
    let vertices = convert_vertices(&asset.model.vertices);
    let mut indices = if let Some(skin) = &asset.skin {
        let mut filtered = Vec::new();
        for sub in &skin.submeshes {
            if !enabled_geosets.is_empty() && !enabled_geosets.contains(&sub.geoset_id) {
                continue;
            }
            let start = usize::from(sub.start_triangle);
            let end = start.saturating_add(usize::from(sub.triangle_count));
            for tri in &skin.triangles[start.min(skin.triangles.len())..end.min(skin.triangles.len())] {
                filtered.push(u32::from(*tri));
            }
        }
        filtered
    } else {
        Vec::new()
    };

    if indices.is_empty() {
        indices = build_triangle_list(vertices.len())?;
    }
    Ok(CharacterPrimitive {
        name: "Body".to_string(),
        geoset_id: None,
        material,
        vertices,
        indices,
    })
}

fn build_equipment_primitive(asset: &ModelAsset, material: String) -> Result<CharacterPrimitive> {
    let vertices = convert_vertices(&asset.model.vertices);
    let indices = build_triangle_list(vertices.len())?;
    Ok(CharacterPrimitive {
        name: asset.model_path.clone(),
        geoset_id: None,
        material,
        vertices,
        indices,
    })
}

fn convert_vertices(vertices: &[M2Vertex]) -> Vec<MeshVertex> {
    vertices
        .iter()
        .map(|v| MeshVertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv0,
            joints: [
                u16::from(v.bone_indices[0]),
                u16::from(v.bone_indices[1]),
                u16::from(v.bone_indices[2]),
                u16::from(v.bone_indices[3]),
            ],
            weights: normalize_weights(v.bone_weights),
        })
        .collect()
}

fn normalize_weights(weights: [u8; 4]) -> [f32; 4] {
    let sum = u32::from(weights[0]) + u32::from(weights[1]) + u32::from(weights[2]) + u32::from(weights[3]);
    if sum == 0 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    [
        weights[0] as f32 / sum as f32,
        weights[1] as f32 / sum as f32,
        weights[2] as f32 / sum as f32,
        weights[3] as f32 / sum as f32,
    ]
}

fn build_triangle_list(vertex_count: usize) -> Result<Vec<u32>> {
    if vertex_count < 3 {
        bail!("need at least 3 vertices for primitive")
    }
    let mut out = Vec::new();
    for i in 2..vertex_count {
        out.push(0u32);
        out.push((i - 1) as u32);
        out.push(i as u32);
    }
    Ok(out)
}

fn build_animations(model: &M2Model, requested: &[u16]) -> Vec<AnimationClip> {
    let defaults = if requested.is_empty() {
        vec![0, 4, 5, 69]
    } else {
        requested.to_vec()
    };
    defaults
        .into_iter()
        .filter_map(|id| {
            let source = choose_animation(&model.animations, id)?;
            let duration_sec = (source.end_ms.saturating_sub(source.start_ms)) as f32 / 1000.0;
            let mid = if duration_sec > 0.0 { duration_sec * 0.5 } else { 0.5 };
            Some(AnimationClip {
                name: animation_name(id).to_string(),
                duration_sec: duration_sec.max(1.0),
                tracks: vec![BoneTrack {
                    bone_index: 0,
                    times_sec: vec![0.0, mid, duration_sec.max(1.0)],
                    translations: vec![[0.0, 0.0, 0.0], [0.1, 0.0, 0.0], [0.0, 0.0, 0.0]],
                    rotations: vec![
                        [0.0, 0.0, 0.0, 1.0],
                        quat_to_arr(Quat::from_rotation_z(id as f32 * 0.0005)),
                        [0.0, 0.0, 0.0, 1.0],
                    ],
                    scales: vec![[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]],
                }],
            })
        })
        .collect()
}

fn choose_animation(animations: &[M2Animation], requested_id: u16) -> Option<&M2Animation> {
    animations
        .iter()
        .find(|anim| anim.id == requested_id)
        .or_else(|| animations.first())
}

fn animation_name(id: u16) -> &'static str {
    match id {
        0 => "stand",
        4 => "walk",
        5 => "run",
        69 => "emote",
        _ => "anim",
    }
}

fn quat_to_arr(q: Quat) -> [f32; 4] {
    [q.x, q.y, q.z, q.w]
}

fn attachment_parent_index(attachment: &str, bones: &[Bone]) -> Option<usize> {
    let key = attachment.to_ascii_lowercase();
    for (idx, bone) in bones.iter().enumerate() {
        if bone.name.to_ascii_lowercase().contains(&key) {
            return Some(idx);
        }
    }
    if bones.is_empty() { None } else { Some(0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_are_normalized() {
        let w = normalize_weights([10, 10, 0, 0]);
        assert!((w[0] - 0.5).abs() < 0.001);
        assert!((w[1] - 0.5).abs() < 0.001);
    }
}
