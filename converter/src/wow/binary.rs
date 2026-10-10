use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct DbcHeader {
    pub record_count: u32,
    pub field_count: u32,
    pub record_size: u32,
    pub string_block_size: u32,
}

#[derive(Debug, Clone)]
pub struct M2Header {
    pub version: u32,
    pub name_count: u32,
    pub animation_count: u32,
    pub bones_count: u32,
    pub vertices_count: u32,
    pub views_count: u32,
}

pub fn parse_dbc_header(bytes: &[u8]) -> Result<DbcHeader> {
    if bytes.len() < 20 {
        bail!("DBC file too small: {} bytes", bytes.len());
    }
    if &bytes[0..4] != b"WDBC" {
        bail!("invalid DBC signature; expected WDBC");
    }
    Ok(DbcHeader {
        record_count: read_u32_le(bytes, 4)?,
        field_count: read_u32_le(bytes, 8)?,
        record_size: read_u32_le(bytes, 12)?,
        string_block_size: read_u32_le(bytes, 16)?,
    })
}

pub fn parse_m2_header(bytes: &[u8]) -> Result<M2Header> {
    if bytes.len() < 72 {
        bail!("M2 file too small: {} bytes", bytes.len());
    }
    if &bytes[0..4] != b"MD20" {
        bail!("invalid M2 signature; expected MD20");
    }

    Ok(M2Header {
        version: read_u32_le(bytes, 4)?,
        name_count: read_u32_le(bytes, 8)?,
        animation_count: read_u32_le(bytes, 28)?,
        bones_count: read_u32_le(bytes, 52)?,
        vertices_count: read_u32_le(bytes, 60)?,
        views_count: read_u32_le(bytes, 68)?,
    })
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let chunk = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow::anyhow!("missing u32 at offset {offset}"))?;
    let mut arr = [0u8; 4];
    arr.copy_from_slice(chunk);
    Ok(u32::from_le_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_dbc_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"WDBC");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes());
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&123u32.to_le_bytes());
        let header = parse_dbc_header(&bytes).expect("dbc header");
        assert_eq!(header.record_count, 10);
        assert_eq!(header.field_count, 5);
    }

    #[test]
    fn parse_valid_m2_header() {
        let mut bytes = vec![0u8; 72];
        bytes[0..4].copy_from_slice(b"MD20");
        bytes[4..8].copy_from_slice(&256u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&14u32.to_le_bytes());
        bytes[52..56].copy_from_slice(&64u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&2048u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&4u32.to_le_bytes());
        let header = parse_m2_header(&bytes).expect("m2 header");
        assert_eq!(header.animation_count, 14);
        assert_eq!(header.bones_count, 64);
        assert_eq!(header.views_count, 4);
    }
}
