use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::wow::binary::{DbcHeader, parse_dbc_header};

#[derive(Debug, Clone)]
pub struct DbcTable {
    pub header: DbcHeader,
    rows: Vec<Vec<u32>>,
    strings: Vec<u8>,
}

impl DbcTable {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn rows(&self) -> &[Vec<u32>] {
        &self.rows
    }

    pub fn get_u32(&self, row: &[u32], field: usize) -> Option<u32> {
        row.get(field).copied()
    }

    pub fn get_i32(&self, row: &[u32], field: usize) -> Option<i32> {
        self.get_u32(row, field).map(|v| v as i32)
    }

    pub fn get_string(&self, row: &[u32], field: usize) -> Option<String> {
        let offset = self.get_u32(row, field)? as usize;
        if offset == 0 || offset >= self.strings.len() {
            return None;
        }
        let tail = &self.strings[offset..];
        let end = tail.iter().position(|c| *c == 0).unwrap_or(tail.len());
        if end == 0 {
            return None;
        }
        String::from_utf8(tail[..end].to_vec()).ok()
    }
}

#[derive(Debug, Clone)]
pub struct CharSectionRecord {
    pub id: u32,
    pub race: i32,
    pub gender: i32,
    pub section_type: i32,
    pub color: i32,
    pub variation: i32,
    pub texture_1: Option<String>,
    pub texture_2: Option<String>,
    pub texture_3: Option<String>,
    pub flags: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ItemDisplayInfoRecord {
    pub id: u32,
    pub model_name_1: Option<String>,
    pub model_name_2: Option<String>,
    pub texture_1: Option<String>,
    pub texture_2: Option<String>,
    pub icon_1: Option<String>,
    pub icon_2: Option<String>,
    pub geoset_group_1: Option<i32>,
    pub geoset_group_2: Option<i32>,
    pub geoset_group_3: Option<i32>,
    pub flags: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct CharHairGeosetRecord {
    pub race: i32,
    pub gender: i32,
    pub variation: i32,
    pub geoset: i32,
}

#[derive(Debug, Clone)]
pub struct CharacterFacialHairStyleRecord {
    pub race: i32,
    pub gender: i32,
    pub variation: i32,
    pub geosets: [i32; 5],
}

#[derive(Debug, Clone)]
pub struct HelmetGeosetVisDataRecord {
    pub id: u32,
    pub geoset_vis: Vec<i32>,
}

pub fn parse_dbc_table(bytes: &[u8]) -> Result<DbcTable> {
    let header = parse_dbc_header(bytes)?;
    let row_count = header.record_count as usize;
    let field_count = header.field_count as usize;
    let record_size = header.record_size as usize;

    let expected_record_size = field_count
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("DBC field_count overflow"))?;
    if expected_record_size != record_size {
        bail!(
            "unexpected DBC record size: fields={} expects {} bytes, found {} bytes",
            field_count,
            expected_record_size,
            record_size
        );
    }

    let records_start = 20usize;
    let records_len = row_count
        .checked_mul(record_size)
        .ok_or_else(|| anyhow::anyhow!("DBC records length overflow"))?;
    let strings_start = records_start
        .checked_add(records_len)
        .ok_or_else(|| anyhow::anyhow!("DBC strings start overflow"))?;
    let strings_end = strings_start
        .checked_add(header.string_block_size as usize)
        .ok_or_else(|| anyhow::anyhow!("DBC string block overflow"))?;
    if bytes.len() < strings_end {
        bail!(
            "DBC truncated: need {} bytes, got {} bytes",
            strings_end,
            bytes.len()
        );
    }

    let mut rows = Vec::with_capacity(row_count);
    for row_idx in 0..row_count {
        let base = records_start + row_idx * record_size;
        let row_bytes = &bytes[base..base + record_size];
        let mut row = Vec::with_capacity(field_count);
        for field in 0..field_count {
            let off = field * 4;
            let mut chunk = [0u8; 4];
            chunk.copy_from_slice(&row_bytes[off..off + 4]);
            row.push(u32::from_le_bytes(chunk));
        }
        rows.push(row);
    }

    Ok(DbcTable {
        header,
        rows,
        strings: bytes[strings_start..strings_end].to_vec(),
    })
}

pub fn decode_char_sections(table: &DbcTable) -> Vec<CharSectionRecord> {
    table
        .rows()
        .iter()
        .filter_map(|row| {
            Some(CharSectionRecord {
                id: table.get_u32(row, 0)?,
                race: table.get_i32(row, 1)?,
                gender: table.get_i32(row, 2)?,
                section_type: table.get_i32(row, 3)?,
                color: table.get_i32(row, 4)?,
                variation: table.get_i32(row, 5)?,
                texture_1: table.get_string(row, 6),
                texture_2: table.get_string(row, 7),
                texture_3: table.get_string(row, 8),
                flags: table.get_i32(row, 9),
            })
        })
        .collect()
}

pub fn decode_item_display_info(table: &DbcTable) -> BTreeMap<u32, ItemDisplayInfoRecord> {
    table
        .rows()
        .iter()
        .filter_map(|row| {
            let id = table.get_u32(row, 0)?;
            Some((
                id,
                ItemDisplayInfoRecord {
                    id,
                    model_name_1: table.get_string(row, 1),
                    model_name_2: table.get_string(row, 2),
                    texture_1: table.get_string(row, 3),
                    texture_2: table.get_string(row, 4),
                    icon_1: table.get_string(row, 5),
                    icon_2: table.get_string(row, 6),
                    geoset_group_1: table.get_i32(row, 7),
                    geoset_group_2: table.get_i32(row, 8),
                    geoset_group_3: table.get_i32(row, 9),
                    flags: table.get_i32(row, 10),
                },
            ))
        })
        .collect()
}

pub fn decode_char_hair_geosets(table: &DbcTable) -> Vec<CharHairGeosetRecord> {
    table
        .rows()
        .iter()
        .filter_map(|row| {
            Some(CharHairGeosetRecord {
                race: table.get_i32(row, 0)?,
                gender: table.get_i32(row, 1)?,
                variation: table.get_i32(row, 2)?,
                geoset: table.get_i32(row, 3)?,
            })
        })
        .collect()
}

pub fn decode_character_facial_hair_styles(table: &DbcTable) -> Vec<CharacterFacialHairStyleRecord> {
    table
        .rows()
        .iter()
        .filter_map(|row| {
            Some(CharacterFacialHairStyleRecord {
                race: table.get_i32(row, 0)?,
                gender: table.get_i32(row, 1)?,
                variation: table.get_i32(row, 2)?,
                geosets: [
                    table.get_i32(row, 3)?,
                    table.get_i32(row, 4)?,
                    table.get_i32(row, 5)?,
                    table.get_i32(row, 6)?,
                    table.get_i32(row, 7)?,
                ],
            })
        })
        .collect()
}

pub fn decode_helmet_geoset_vis_data(table: &DbcTable) -> BTreeMap<u32, HelmetGeosetVisDataRecord> {
    table
        .rows()
        .iter()
        .filter_map(|row| {
            let id = table.get_u32(row, 0)?;
            let mut geoset_vis = Vec::new();
            for field in 1..row.len() {
                if let Some(v) = table.get_i32(row, field) {
                    geoset_vis.push(v);
                }
            }
            Some((id, HelmetGeosetVisDataRecord { id, geoset_vis }))
        })
        .collect()
}

pub fn normalize_model_path(input: &str) -> String {
    let normalized = input.replace('\\', "/");
    if normalized.to_ascii_lowercase().ends_with(".m2") {
        return normalized;
    }
    if normalized.to_ascii_lowercase().ends_with(".mdx") {
        return format!("{}.m2", normalized.trim_end_matches(".mdx").trim_end_matches(".MDX"));
    }
    format!("{normalized}.m2")
}

pub fn normalize_texture_path(input: &str) -> String {
    let normalized = input.replace('\\', "/");
    if normalized.to_ascii_lowercase().ends_with(".blp") {
        normalized
    } else {
        format!("{normalized}.blp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_and_strings() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"WDBC");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&42u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0, b'x', 0, 0, 0, 0, 0]);

        let table = parse_dbc_table(&bytes).expect("table");
        assert_eq!(table.row_count(), 1);
        let row = &table.rows()[0];
        assert_eq!(table.get_u32(row, 0), Some(42));
        assert_eq!(table.get_string(row, 1).as_deref(), Some("x"));
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_model_path("foo\\bar.mdx"), "foo/bar.m2");
        assert_eq!(normalize_model_path("foo/bar"), "foo/bar.m2");
        assert_eq!(normalize_texture_path("a\\b"), "a/b.blp");
    }

    #[test]
    fn decodes_hair_and_helmet_rows() {
        let table = DbcTable {
            header: DbcHeader {
                record_count: 1,
                field_count: 8,
                record_size: 32,
                string_block_size: 1,
            },
            rows: vec![vec![4, 1, 2, 701, 11, 12, 13, 14]],
            strings: vec![0],
        };

        let hair = decode_char_hair_geosets(&table);
        assert_eq!(hair[0].geoset, 701);

        let facial = decode_character_facial_hair_styles(&table);
        assert_eq!(facial[0].geosets[0], 701);

        let helmet = decode_helmet_geoset_vis_data(&table);
        assert_eq!(helmet.get(&4).map(|r| r.geoset_vis.len()), Some(7));
    }
}
