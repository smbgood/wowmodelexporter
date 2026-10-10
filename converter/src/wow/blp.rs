use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlpCompression {
    Jpeg,
    Paletted,
    Dxt,
    Unknown(u32),
}

#[derive(Debug, Clone)]
pub struct BlpHeader {
    pub compression: BlpCompression,
    pub alpha_bits: u8,
    pub alpha_encoding: u8,
    pub has_mips: u8,
    pub width: u32,
    pub height: u32,
    pub mip_offsets: [u32; 16],
    pub mip_sizes: [u32; 16],
}

#[derive(Debug, Clone)]
pub struct BlpImage {
    pub header: BlpHeader,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn parse_blp(bytes: &[u8]) -> Result<BlpHeader> {
    if bytes.len() < 148 {
        bail!("BLP file too small: {} bytes", bytes.len());
    }
    if &bytes[0..4] != b"BLP1" {
        bail!("invalid BLP signature; expected BLP1");
    }
    let compression_raw = read_u32(bytes, 4)?;
    let compression = match compression_raw {
        0 => BlpCompression::Jpeg,
        1 => BlpCompression::Paletted,
        2 => BlpCompression::Dxt,
        other => BlpCompression::Unknown(other),
    };
    let alpha_bits = read_u8(bytes, 8)?;
    let alpha_encoding = read_u8(bytes, 9)?;
    let has_mips = read_u8(bytes, 10)?;
    let width = read_u32(bytes, 12)?;
    let height = read_u32(bytes, 16)?;

    let mut mip_offsets = [0u32; 16];
    let mut mip_sizes = [0u32; 16];
    for (i, item) in mip_offsets.iter_mut().enumerate() {
        *item = read_u32(bytes, 20 + i * 4)?;
    }
    for (i, item) in mip_sizes.iter_mut().enumerate() {
        *item = read_u32(bytes, 84 + i * 4)?;
    }

    Ok(BlpHeader {
        compression,
        alpha_bits,
        alpha_encoding,
        has_mips,
        width,
        height,
        mip_offsets,
        mip_sizes,
    })
}

pub fn decode_blp_rgba(bytes: &[u8]) -> Result<BlpImage> {
    let header = parse_blp(bytes)?;
    let mip_offset = header.mip_offsets[0] as usize;
    let mip_size = header.mip_sizes[0] as usize;
    if mip_offset == 0 || mip_size == 0 {
        bail!("BLP has empty mip[0]");
    }
    if mip_offset
        .checked_add(mip_size)
        .ok_or_else(|| anyhow!("mip range overflow"))?
        > bytes.len()
    {
        bail!("BLP mip[0] out of range");
    }
    let mip_data = &bytes[mip_offset..mip_offset + mip_size];
    let rgba = match header.compression {
        BlpCompression::Paletted => decode_paletted(bytes, &header, mip_data)?,
        BlpCompression::Dxt => decode_dxt(&header, mip_data)?,
        BlpCompression::Jpeg => bail!("BLP JPEG compression is not supported in this exporter"),
        BlpCompression::Unknown(v) => bail!("unsupported BLP compression {v}"),
    };
    Ok(BlpImage {
        header: header.clone(),
        rgba,
        width: header.width,
        height: header.height,
    })
}

fn decode_paletted(bytes: &[u8], header: &BlpHeader, mip_data: &[u8]) -> Result<Vec<u8>> {
    let pixel_count = (header.width as usize)
        .checked_mul(header.height as usize)
        .ok_or_else(|| anyhow!("BLP dimensions overflow"))?;
    if bytes.len() < 148 + 256 * 4 {
        bail!("BLP palette block missing");
    }
    if mip_data.len() < pixel_count {
        bail!("BLP paletted mip too small for index buffer");
    }

    let alpha_bytes = alpha_bytes_for_paletted(header.alpha_bits, pixel_count);
    if mip_data.len() < pixel_count + alpha_bytes {
        bail!("BLP paletted mip too small for alpha payload");
    }

    let palette = &bytes[148..148 + 1024];
    let indices = &mip_data[..pixel_count];
    let alpha = &mip_data[pixel_count..pixel_count + alpha_bytes];
    let mut out = vec![0u8; pixel_count * 4];
    for i in 0..pixel_count {
        let idx = indices[i] as usize;
        let p = idx * 4;
        out[i * 4] = palette[p + 2];
        out[i * 4 + 1] = palette[p + 1];
        out[i * 4 + 2] = palette[p];
        out[i * 4 + 3] = read_alpha(alpha, header.alpha_bits, i);
    }
    Ok(out)
}

fn alpha_bytes_for_paletted(alpha_bits: u8, pixel_count: usize) -> usize {
    match alpha_bits {
        0 => 0,
        1 => pixel_count.div_ceil(8),
        4 => pixel_count.div_ceil(2),
        8 => pixel_count,
        _ => 0,
    }
}

fn read_alpha(alpha: &[u8], alpha_bits: u8, pixel: usize) -> u8 {
    match alpha_bits {
        0 => 255,
        1 => {
            let byte = alpha.get(pixel / 8).copied().unwrap_or(0);
            if (byte >> (pixel % 8)) & 1 == 1 {
                255
            } else {
                0
            }
        }
        4 => {
            let byte = alpha.get(pixel / 2).copied().unwrap_or(0);
            let nibble = if pixel.is_multiple_of(2) {
                byte & 0x0F
            } else {
                byte >> 4
            };
            (nibble << 4) | nibble
        }
        8 => alpha.get(pixel).copied().unwrap_or(255),
        _ => 255,
    }
}

fn decode_dxt(header: &BlpHeader, mip_data: &[u8]) -> Result<Vec<u8>> {
    let width = header.width as usize;
    let height = header.height as usize;
    let mut out = vec![0u8; width * height * 4];
    let block_w = width.div_ceil(4);
    let block_h = height.div_ceil(4);
    let block_size = if header.alpha_bits > 1 { 16 } else { 8 };
    let needed = block_w
        .checked_mul(block_h)
        .and_then(|n| n.checked_mul(block_size))
        .ok_or_else(|| anyhow!("DXT size overflow"))?;
    if mip_data.len() < needed {
        bail!("BLP DXT mip too small");
    }

    let mut offset = 0usize;
    for by in 0..block_h {
        for bx in 0..block_w {
            if block_size == 8 {
                decode_dxt1_block(
                    &mip_data[offset..offset + 8],
                    bx,
                    by,
                    width,
                    height,
                    &mut out,
                )?;
            } else {
                decode_dxt3_block(
                    &mip_data[offset..offset + 16],
                    bx,
                    by,
                    width,
                    height,
                    &mut out,
                )?;
            }
            offset += block_size;
        }
    }
    Ok(out)
}

fn decode_dxt1_block(
    block: &[u8],
    bx: usize,
    by: usize,
    width: usize,
    height: usize,
    out: &mut [u8],
) -> Result<()> {
    if block.len() < 8 {
        bail!("short DXT1 block");
    }
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let mut palette = [[0u8; 4]; 4];
    palette[0] = rgb565(c0);
    palette[1] = rgb565(c1);
    if c0 > c1 {
        palette[2] = [
            ((2 * palette[0][0] as u16 + palette[1][0] as u16) / 3) as u8,
            ((2 * palette[0][1] as u16 + palette[1][1] as u16) / 3) as u8,
            ((2 * palette[0][2] as u16 + palette[1][2] as u16) / 3) as u8,
            255,
        ];
        palette[3] = [
            ((palette[0][0] as u16 + 2 * palette[1][0] as u16) / 3) as u8,
            ((palette[0][1] as u16 + 2 * palette[1][1] as u16) / 3) as u8,
            ((palette[0][2] as u16 + 2 * palette[1][2] as u16) / 3) as u8,
            255,
        ];
    } else {
        palette[2] = [
            ((palette[0][0] as u16 + palette[1][0] as u16) / 2) as u8,
            ((palette[0][1] as u16 + palette[1][1] as u16) / 2) as u8,
            ((palette[0][2] as u16 + palette[1][2] as u16) / 2) as u8,
            255,
        ];
        palette[3] = [0, 0, 0, 0];
    }

    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for y in 0..4 {
        for x in 0..4 {
            let p = (indices >> (2 * (4 * y + x))) & 0x3;
            let px = bx * 4 + x;
            let py = by * 4 + y;
            if px >= width || py >= height {
                continue;
            }
            let dst = (py * width + px) * 4;
            out[dst..dst + 4].copy_from_slice(&palette[p as usize]);
        }
    }
    Ok(())
}

fn decode_dxt3_block(
    block: &[u8],
    bx: usize,
    by: usize,
    width: usize,
    height: usize,
    out: &mut [u8],
) -> Result<()> {
    if block.len() < 16 {
        bail!("short DXT3 block");
    }
    let mut alpha = [0u8; 16];
    for i in 0..8 {
        let byte = block[i];
        alpha[i * 2] = (byte & 0x0F) * 17;
        alpha[i * 2 + 1] = (byte >> 4) * 17;
    }

    decode_dxt1_block(
        &block[8..16],
        bx,
        by,
        width,
        height,
        out,
    )?;
    for y in 0..4 {
        for x in 0..4 {
            let px = bx * 4 + x;
            let py = by * 4 + y;
            if px >= width || py >= height {
                continue;
            }
            let dst = (py * width + px) * 4;
            out[dst + 3] = alpha[y * 4 + x];
        }
    }
    Ok(())
}

fn rgb565(v: u16) -> [u8; 4] {
    let r = ((v >> 11) & 0x1F) as u8;
    let g = ((v >> 5) & 0x3F) as u8;
    let b = (v & 0x1F) as u8;
    [
        (u16::from(r) * 255 / 31) as u8,
        (u16::from(g) * 255 / 63) as u8,
        (u16::from(b) * 255 / 31) as u8,
        255,
    ]
}

fn read_u8(bytes: &[u8], offset: usize) -> Result<u8> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| anyhow!("missing u8 at offset {offset}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_header() -> Vec<u8> {
        let mut bytes = vec![0u8; 148 + 1024];
        bytes[0..4].copy_from_slice(b"BLP1");
        bytes[10] = 1; // has mips
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes
    }

    #[test]
    fn decode_paletted_blp() {
        let mut bytes = blank_header();
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes()); // paletted
        bytes[8] = 8; // alpha bits
        bytes[20..24].copy_from_slice(&(1172u32).to_le_bytes());
        bytes[84..88].copy_from_slice(&(2u32).to_le_bytes());

        // palette index 1 => red
        let pal = 148 + 4;
        bytes[pal] = 0;
        bytes[pal + 1] = 0;
        bytes[pal + 2] = 255;
        bytes[pal + 3] = 255;

        bytes.push(1); // pixel index
        bytes.push(200); // alpha

        let image = decode_blp_rgba(&bytes).expect("decode");
        assert_eq!(image.width, 1);
        assert_eq!(image.rgba, vec![255, 0, 0, 200]);
    }

    #[test]
    fn decode_dxt1_blp() {
        let mut bytes = blank_header();
        bytes[4..8].copy_from_slice(&2u32.to_le_bytes()); // dxt
        bytes[12..16].copy_from_slice(&4u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&4u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&(1172u32).to_le_bytes());
        bytes[84..88].copy_from_slice(&(8u32).to_le_bytes());

        // One red block (c0 = red565), c1 = red565, indices all 0
        bytes.extend_from_slice(&0xF800u16.to_le_bytes());
        bytes.extend_from_slice(&0xF800u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let image = decode_blp_rgba(&bytes).expect("decode");
        assert_eq!(image.width, 4);
        assert_eq!(image.height, 4);
        assert_eq!(image.rgba[0], 255);
    }
}
