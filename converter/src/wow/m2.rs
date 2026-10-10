use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, Copy)]
pub struct M2ArrayRef {
    pub count: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct M2Header {
    pub version: u32,
    pub name: Option<String>,
    pub animations: M2ArrayRef,
    pub animation_lookup: M2ArrayRef,
    pub bones: M2ArrayRef,
    pub vertices: M2ArrayRef,
    pub views: M2ArrayRef,
    pub textures: M2ArrayRef,
    pub texture_lookup: M2ArrayRef,
    pub texture_units: M2ArrayRef,
    pub attachments: M2ArrayRef,
}

#[derive(Debug, Clone)]
pub struct M2Vertex {
    pub position: [f32; 3],
    pub bone_weights: [u8; 4],
    pub bone_indices: [u8; 4],
    pub normal: [f32; 3],
    pub uv0: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct M2Bone {
    pub key_bone_id: i32,
    pub parent_index: i16,
    pub pivot: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct M2Animation {
    pub id: u16,
    pub sub_id: u16,
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone)]
pub struct M2Attachment {
    pub id: u32,
    pub bone: u16,
    pub position: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct M2TextureDef {
    pub texture_type: u32,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct M2Model {
    pub header: M2Header,
    pub vertices: Vec<M2Vertex>,
    pub bones: Vec<M2Bone>,
    pub animations: Vec<M2Animation>,
    pub attachments: Vec<M2Attachment>,
    pub textures: Vec<M2TextureDef>,
}

pub fn parse_m2(bytes: &[u8]) -> Result<M2Model> {
    if bytes.len() < 144 {
        bail!("M2 file too small: {} bytes", bytes.len());
    }
    if &bytes[0..4] != b"MD20" {
        bail!("invalid M2 signature; expected MD20");
    }

    let version = read_u32(bytes, 4)?;
    let name_ref = read_array(bytes, 8)?;
    let header = M2Header {
        version,
        name: read_c_string(bytes, name_ref.offset as usize, name_ref.count as usize),
        animations: read_array(bytes, 24)?,
        animation_lookup: read_array(bytes, 32)?,
        bones: read_array(bytes, 40)?,
        vertices: read_array(bytes, 56)?,
        views: read_array(bytes, 64)?,
        textures: read_array(bytes, 80)?,
        texture_lookup: read_array(bytes, 96)?,
        texture_units: read_array(bytes, 104)?,
        attachments: read_array(bytes, 128)?,
    };

    let vertices = parse_vertices(bytes, header.vertices)?;
    let bones = parse_bones(bytes, header.bones)?;
    let animations = parse_animations(bytes, header.animations)?;
    let attachments = parse_attachments(bytes, header.attachments)?;
    let textures = parse_textures(bytes, header.textures)?;

    Ok(M2Model {
        header,
        vertices,
        bones,
        animations,
        attachments,
        textures,
    })
}

fn parse_vertices(bytes: &[u8], ar: M2ArrayRef) -> Result<Vec<M2Vertex>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    // WoW classic vertex is 48 bytes; tolerate larger rows by stepping 48.
    let needed = base
        .checked_add(count.saturating_mul(48))
        .ok_or_else(|| anyhow!("vertex range overflow"))?;
    if needed > bytes.len() {
        bail!(
            "vertex block out of range: offset={} count={} bytes={}",
            base,
            count,
            bytes.len()
        );
    }

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * 48;
        out.push(M2Vertex {
            position: [
                read_f32(bytes, off)?,
                read_f32(bytes, off + 4)?,
                read_f32(bytes, off + 8)?,
            ],
            bone_weights: [
                read_u8(bytes, off + 12)?,
                read_u8(bytes, off + 13)?,
                read_u8(bytes, off + 14)?,
                read_u8(bytes, off + 15)?,
            ],
            bone_indices: [
                read_u8(bytes, off + 16)?,
                read_u8(bytes, off + 17)?,
                read_u8(bytes, off + 18)?,
                read_u8(bytes, off + 19)?,
            ],
            normal: [
                read_f32(bytes, off + 20)?,
                read_f32(bytes, off + 24)?,
                read_f32(bytes, off + 28)?,
            ],
            uv0: [read_f32(bytes, off + 32)?, read_f32(bytes, off + 36)?],
        });
    }
    Ok(out)
}

fn parse_bones(bytes: &[u8], ar: M2ArrayRef) -> Result<Vec<M2Bone>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    // 1.12 bone defs are >= 88 bytes; we read fields that are stable near the start/end.
    let stride = 88usize;
    let needed = base
        .checked_add(count.saturating_mul(stride))
        .ok_or_else(|| anyhow!("bone range overflow"))?;
    if needed > bytes.len() {
        // Tolerate unknown variants by returning empty bones instead of aborting export.
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * stride;
        let pivot_off = off + (stride - 12);
        out.push(M2Bone {
            key_bone_id: read_i32(bytes, off)?,
            parent_index: read_i16(bytes, off + 8)?,
            pivot: [
                read_f32(bytes, pivot_off)?,
                read_f32(bytes, pivot_off + 4)?,
                read_f32(bytes, pivot_off + 8)?,
            ],
        });
    }
    Ok(out)
}

fn parse_animations(bytes: &[u8], ar: M2ArrayRef) -> Result<Vec<M2Animation>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    // Classic sequence rows are commonly 68 bytes. Read front fields only.
    let stride = 68usize;
    let needed = base
        .checked_add(count.saturating_mul(stride))
        .ok_or_else(|| anyhow!("animation range overflow"))?;
    if needed > bytes.len() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * stride;
        out.push(M2Animation {
            id: read_u16(bytes, off)?,
            sub_id: read_u16(bytes, off + 2)?,
            start_ms: read_u32(bytes, off + 4)?,
            end_ms: read_u32(bytes, off + 8)?,
        });
    }
    Ok(out)
}

fn parse_attachments(bytes: &[u8], ar: M2ArrayRef) -> Result<Vec<M2Attachment>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    let stride = 40usize;
    let needed = base
        .checked_add(count.saturating_mul(stride))
        .ok_or_else(|| anyhow!("attachment range overflow"))?;
    if needed > bytes.len() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * stride;
        out.push(M2Attachment {
            id: read_u32(bytes, off)?,
            bone: read_u16(bytes, off + 4)?,
            position: [
                read_f32(bytes, off + 8)?,
                read_f32(bytes, off + 12)?,
                read_f32(bytes, off + 16)?,
            ],
        });
    }
    Ok(out)
}

fn parse_textures(bytes: &[u8], ar: M2ArrayRef) -> Result<Vec<M2TextureDef>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    let stride = 16usize;
    let needed = base
        .checked_add(count.saturating_mul(stride))
        .ok_or_else(|| anyhow!("texture range overflow"))?;
    if needed > bytes.len() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * stride;
        let texture_type = read_u32(bytes, off)?;
        let name_count = read_u32(bytes, off + 4)? as usize;
        let name_ofs = read_u32(bytes, off + 8)? as usize;
        out.push(M2TextureDef {
            texture_type,
            path: read_c_string(bytes, name_ofs, name_count),
        });
    }
    Ok(out)
}

fn read_array(bytes: &[u8], offset: usize) -> Result<M2ArrayRef> {
    Ok(M2ArrayRef {
        count: read_u32(bytes, offset)?,
        offset: read_u32(bytes, offset + 4)?,
    })
}

fn read_c_string(bytes: &[u8], offset: usize, count: usize) -> Option<String> {
    if count == 0 || offset >= bytes.len() {
        return None;
    }
    let end = offset.saturating_add(count).min(bytes.len());
    let slice = &bytes[offset..end];
    let end_zero = slice.iter().position(|b| *b == 0).unwrap_or(slice.len());
    if end_zero == 0 {
        return None;
    }
    String::from_utf8(slice[..end_zero].to_vec()).ok()
}

fn read_u8(bytes: &[u8], offset: usize) -> Result<u8> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| anyhow!("missing u8 at offset {offset}"))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let mut arr = [0u8; 2];
    arr.copy_from_slice(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| anyhow!("missing u16 at offset {offset}"))?,
    );
    Ok(u16::from_le_bytes(arr))
}

fn read_i16(bytes: &[u8], offset: usize) -> Result<i16> {
    Ok(read_u16(bytes, offset)? as i16)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let mut arr = [0u8; 4];
    arr.copy_from_slice(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow!("missing u32 at offset {offset}"))?,
    );
    Ok(u32::from_le_bytes(arr))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
    Ok(read_u32(bytes, offset)? as i32)
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32> {
    let mut arr = [0u8; 4];
    arr.copy_from_slice(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow!("missing f32 at offset {offset}"))?,
    );
    Ok(f32::from_le_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_m2_layout() {
        let mut bytes = vec![0u8; 512];
        bytes[0..4].copy_from_slice(b"MD20");
        bytes[4..8].copy_from_slice(&256u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&1u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&200u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&1u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&248u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&1u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&336u32.to_le_bytes());

        // Vertex.
        bytes[200..204].copy_from_slice(&1.0f32.to_le_bytes());
        bytes[204..208].copy_from_slice(&2.0f32.to_le_bytes());
        bytes[208..212].copy_from_slice(&3.0f32.to_le_bytes());
        bytes[212..216].copy_from_slice(&[255, 0, 0, 0]);
        bytes[216..220].copy_from_slice(&[0, 0, 0, 0]);
        bytes[220..224].copy_from_slice(&0.0f32.to_le_bytes());
        bytes[224..228].copy_from_slice(&1.0f32.to_le_bytes());
        bytes[228..232].copy_from_slice(&0.0f32.to_le_bytes());
        bytes[232..236].copy_from_slice(&0.25f32.to_le_bytes());
        bytes[236..240].copy_from_slice(&0.75f32.to_le_bytes());

        // Bone.
        bytes[248..252].copy_from_slice(&2i32.to_le_bytes());
        bytes[256..258].copy_from_slice(&(-1i16).to_le_bytes());
        bytes[324..328].copy_from_slice(&10.0f32.to_le_bytes());
        bytes[328..332].copy_from_slice(&11.0f32.to_le_bytes());
        bytes[332..336].copy_from_slice(&12.0f32.to_le_bytes());

        // Animation.
        bytes[336..338].copy_from_slice(&4u16.to_le_bytes());
        bytes[338..340].copy_from_slice(&0u16.to_le_bytes());
        bytes[340..344].copy_from_slice(&0u32.to_le_bytes());
        bytes[344..348].copy_from_slice(&1600u32.to_le_bytes());

        let model = parse_m2(&bytes).expect("parse m2");
        assert_eq!(model.vertices.len(), 1);
        assert_eq!(model.vertices[0].uv0, [0.25, 0.75]);
        assert_eq!(model.bones.len(), 1);
        assert_eq!(model.bones[0].parent_index, -1);
        assert_eq!(model.animations.len(), 1);
        assert_eq!(model.animations[0].id, 4);
    }
}
