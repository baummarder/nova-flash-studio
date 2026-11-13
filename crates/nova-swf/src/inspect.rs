//! Independent SWF inspector.
//!
//! This parser is deliberately separate from the writer: tests and the CLI use
//! it to verify that emitted bytes have the header, frame table and tag
//! boundaries they claim to have. It is not a full SWF decoder.

use std::io::Read;
use std::path::Path;

use crate::error::{SwfError, SwfResult};
use crate::tags::{TAG_DO_ABC, TAG_DO_ABC2, TAG_END, TAG_SYMBOL_CLASS, tag_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagInfo {
    pub code: u16,
    pub length: u32,
    pub name: &'static str,
}

#[derive(Debug, Clone)]
pub struct SwfInfo {
    pub signature: String,
    pub version: u8,
    pub declared_length: u32,
    pub actual_length: usize,
    pub compressed: bool,
    pub width: f32,
    pub height: f32,
    pub frame_rate: f32,
    pub frame_count: u16,
    pub tags: Vec<TagInfo>,
    pub has_end: bool,
    pub warnings: Vec<String>,
}

impl SwfInfo {
    pub fn has_tag(&self, code: u16) -> bool {
        self.tags.iter().any(|t| t.code == code)
    }

    pub fn has_avm2(&self) -> bool {
        self.has_tag(TAG_DO_ABC) || self.has_tag(TAG_DO_ABC2) || self.has_tag(TAG_SYMBOL_CLASS)
    }

    pub fn summary(&self) -> String {
        format!(
            "SWF v{} · {}×{} px · {} fps · {} Frames · {} Tags{}",
            self.version,
            self.width,
            self.height,
            self.frame_rate,
            self.frame_count,
            self.tags.len(),
            if self.compressed {
                " · zlib"
            } else {
                " · unkomprimiert"
            }
        )
    }
}

pub fn inspect_file(path: &Path) -> SwfResult<SwfInfo> {
    let bytes = std::fs::read(path)?;
    inspect(&bytes)
}

pub fn inspect(bytes: &[u8]) -> SwfResult<SwfInfo> {
    if bytes.len() < 8 {
        return Err(SwfError::Invalid(
            "file shorter than the 8-byte SWF header".into(),
        ));
    }
    let signature = std::str::from_utf8(&bytes[0..3])
        .map_err(|_| SwfError::Signature(String::from_utf8_lossy(&bytes[0..3]).into()))?
        .to_string();
    let version = bytes[3];
    let declared_length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

    let (body, compressed) = match signature.as_str() {
        "FWS" => (bytes[8..].to_vec(), false),
        "CWS" => {
            let mut decoder = flate2::read::ZlibDecoder::new(&bytes[8..]);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| SwfError::Invalid(format!("cannot decompress CWS body: {e}")))?;
            (out, true)
        }
        other => return Err(SwfError::Signature(other.to_string())),
    };

    let mut warnings = Vec::new();
    let expected = 8 + body.len();
    if declared_length as usize != expected {
        warnings.push(format!(
            "Header length {declared_length} != actual {expected} bytes"
        ));
    }

    let mut reader = BitReader::new(&body);
    let rect = reader
        .read_rect()
        .ok_or_else(|| SwfError::Invalid("truncated stage rectangle".into()))?;
    reader.align();
    let frame_rate_raw = reader
        .read_u16()
        .ok_or_else(|| SwfError::Invalid("truncated frame rate".into()))?;
    let frame_count = reader
        .read_u16()
        .ok_or_else(|| SwfError::Invalid("truncated frame count".into()))?;

    let mut tags = Vec::new();
    let mut pos = reader.byte_pos().min(body.len());
    let mut has_end = false;
    while pos + 2 <= body.len() {
        let header = u16::from_le_bytes([body[pos], body[pos + 1]]);
        let code = header >> 6;
        let mut length = (header & 0x3f) as u32;
        let mut header_len = 2;
        if length == 0x3f {
            if pos + 6 > body.len() {
                warnings.push("truncated long tag header at end of file".into());
                break;
            }
            length =
                u32::from_le_bytes([body[pos + 2], body[pos + 3], body[pos + 4], body[pos + 5]]);
            header_len = 6;
        }
        let payload_end = pos + header_len + length as usize;
        if payload_end > body.len() {
            warnings.push(format!(
                "tag {} (code {code}) claims {length} bytes past end of file",
                tag_name(code)
            ));
            break;
        }
        tags.push(TagInfo {
            code,
            length,
            name: tag_name(code),
        });
        pos = payload_end;
        if code == TAG_END {
            has_end = true;
            break;
        }
    }
    if !has_end {
        warnings.push("no End tag found".into());
    }
    if pos < body.len() && has_end {
        warnings.push(format!("{} unused bytes after End tag", body.len() - pos));
    }

    let width = (rect.1 - rect.0) as f32 / 20.0;
    let height = (rect.3 - rect.2) as f32 / 20.0;

    Ok(SwfInfo {
        signature,
        version,
        declared_length,
        actual_length: bytes.len(),
        compressed,
        width,
        height,
        frame_rate: frame_rate_raw as f32 / 256.0,
        frame_count,
        tags,
        has_end,
        warnings,
    })
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit: 0,
        }
    }

    fn read_bit(&mut self) -> Option<bool> {
        let byte = *self.data.get(self.pos)?;
        let value = (byte >> (7 - self.bit)) & 1 == 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Some(value)
    }

    fn read_bits(&mut self, count: u8) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Some(value)
    }

    fn read_sbits(&mut self, count: u8) -> Option<i32> {
        let raw = self.read_bits(count)?;
        if count == 32 {
            return Some(raw as i32);
        }
        let sign = 1u32 << (count - 1);
        if raw & sign != 0 {
            Some((raw as i32) - (1i32 << count))
        } else {
            Some(raw as i32)
        }
    }

    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }

    fn byte_pos(&self) -> usize {
        self.pos
    }

    fn read_u16(&mut self) -> Option<u16> {
        let bytes = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Returns (x_min, x_max, y_min, y_max).
    fn read_rect(&mut self) -> Option<(i32, i32, i32, i32)> {
        let bits = self.read_bits(5)? as u8;
        if bits == 0 || bits > 31 {
            return None;
        }
        Some((
            self.read_sbits(bits)?,
            self.read_sbits(bits)?,
            self.read_sbits(bits)?,
            self.read_sbits(bits)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movie::{Compression, Movie};
    use crate::tags::Tag;

    #[test]
    fn inspects_own_movie() {
        let mut movie = Movie::new_avm2(320.0, 240.0, 24.0, 10);
        movie.tags.push(Tag::ShowFrame);
        let bytes = movie.to_bytes(Compression::Zlib).unwrap();
        let info = inspect(&bytes).unwrap();
        assert_eq!(info.version, 10);
        assert_eq!(info.width, 320.0);
        assert_eq!(info.height, 240.0);
        assert_eq!(info.frame_rate, 24.0);
        assert_eq!(info.frame_count, 1);
        assert!(info.compressed);
        assert!(info.has_end);
        assert!(info.warnings.is_empty(), "{:?}", info.warnings);
    }

    #[test]
    fn reports_truncation() {
        let movie = Movie::new_avm1(100.0, 100.0, 12.0, 8);
        let mut bytes = movie.to_bytes(Compression::None).unwrap();
        bytes.truncate(12);
        let result = inspect(&bytes);
        assert!(result.is_err() || !result.unwrap().warnings.is_empty());
    }

    #[test]
    fn rejects_unknown_signature() {
        assert!(inspect(b"XYZ\x0a\x00\x00\x00\x00\x00\x00\x00\x00").is_err());
    }
}
