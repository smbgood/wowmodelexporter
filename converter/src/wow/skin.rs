use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, Copy)]
pub struct SkinArrayRef {
    pub count: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct SkinHeader {
    pub indices: SkinArrayRef,
    pub triangles: SkinArrayRef,
    pub submeshes: SkinArrayRef,
    pub texture_units: SkinArrayRef,
}

#[derive(Debug, Clone)]
pub struct SkinSubmesh {
    pub geoset_id: u16,
    pub level: u16,
    pub start_vertex: u16,
    pub vertex_count: u16,
    pub start_triangle: u16,
    pub triangle_count: u16,
}

#[derive(Debug, Clone)]
pub struct SkinData {
    pub header: SkinHeader,
    pub indices: Vec<u16>,
    pub triangles: Vec<u16>,
    pub submeshes: Vec<SkinSubmesh>,
}

pub fn parse_skin(bytes: &[u8]) -> Result<SkinData> {
    if bytes.len() < 44 {
        bail!("SKIN file too small: {} bytes", bytes.len());
    }
    if &bytes[0..4] != b"SKIN" {
        bail!("invalid SKIN signature");
    }

    let header = SkinHeader {
        indices: read_array(bytes, 4)?,
        triangles: read_array(bytes, 12)?,
        submeshes: read_array(bytes, 28)?,
        texture_units: read_array(bytes, 36)?,
    };

    Ok(SkinData {
        indices: parse_u16_array(bytes, header.indices)?,
        triangles: parse_u16_array(bytes, header.triangles)?,
        submeshes: parse_submeshes(bytes, header.submeshes)?,
        header,
    })
}

fn parse_u16_array(bytes: &[u8], ar: SkinArrayRef) -> Result<Vec<u16>> {
    let count = ar.count as usize;
    let base = ar.offset as usize;
    let needed = base
        .checked_add(count.saturating_mul(2))
        .ok_or_else(|| anyhow!("u16 block overflow"))?;
    if needed > bytes.len() {
        bail!(
            "u16 block out of range: offset={} count={} bytes={}",
            base,
            count,
            bytes.len()
        );
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_u16(bytes, base + i * 2)?);
    }
    Ok(out)
}

fn parse_submeshes(bytes: &[u8], ar: SkinArrayRef) -> Result<Vec<SkinSubmesh>> {
    let count = ar.count as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let base = ar.offset as usize;
    // Submesh rows are 32 bytes in classic/WotLK families.
    let stride = 32usize;
    let needed = base
        .checked_add(count.saturating_mul(stride))
        .ok_or_else(|| anyhow!("submesh block overflow"))?;
    if needed > bytes.len() {
        bail!(
            "submesh block out of range: offset={} count={} bytes={}",
            base,
            count,
            bytes.len()
        );
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = base + i * stride;
        out.push(SkinSubmesh {
            geoset_id: read_u16(bytes, off)?,
            level: read_u16(bytes, off + 2)?,
            start_vertex: read_u16(bytes, off + 4)?,
            vertex_count: read_u16(bytes, off + 6)?,
            start_triangle: read_u16(bytes, off + 8)?,
            triangle_count: read_u16(bytes, off + 10)?,
        });
    }
    Ok(out)
}

fn read_array(bytes: &[u8], offset: usize) -> Result<SkinArrayRef> {
    Ok(SkinArrayRef {
        count: read_u32(bytes, offset)?,
        offset: read_u32(bytes, offset + 4)?,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let mut arr = [0u8; 2];
    arr.copy_from_slice(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| anyhow!("missing u16 at {offset}"))?,
    );
    Ok(u16::from_le_bytes(arr))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let mut arr = [0u8; 4];
    arr.copy_from_slice(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow!("missing u32 at {offset}"))?,
    );
    Ok(u32::from_le_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_skin() {
        let mut bytes = vec![0u8; 256];
        bytes[0..4].copy_from_slice(b"SKIN");

        bytes[4..8].copy_from_slice(&3u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&64u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&3u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&70u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&1u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&80u32.to_le_bytes());

        bytes[64..66].copy_from_slice(&0u16.to_le_bytes());
        bytes[66..68].copy_from_slice(&1u16.to_le_bytes());
        bytes[68..70].copy_from_slice(&2u16.to_le_bytes());

        bytes[70..72].copy_from_slice(&0u16.to_le_bytes());
        bytes[72..74].copy_from_slice(&1u16.to_le_bytes());
        bytes[74..76].copy_from_slice(&2u16.to_le_bytes());

        bytes[80..82].copy_from_slice(&702u16.to_le_bytes());
        bytes[82..84].copy_from_slice(&0u16.to_le_bytes());
        bytes[84..86].copy_from_slice(&0u16.to_le_bytes());
        bytes[86..88].copy_from_slice(&3u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0u16.to_le_bytes());
        bytes[90..92].copy_from_slice(&3u16.to_le_bytes());

        let skin = parse_skin(&bytes).expect("skin");
        assert_eq!(skin.indices.len(), 3);
        assert_eq!(skin.submeshes[0].geoset_id, 702);
    }
}
