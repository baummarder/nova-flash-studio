//! SWF tag serialization.
//!
//! Only tags needed by the current editor milestone are implemented. Any tag
//! can be added without touching the movie writer, because tags serialize
//! themselves into their payload and share the common record header.

use crate::bits::BitWriter;
use crate::error::SwfResult;
use crate::shape::Shape;
use crate::types::{Color, PlaceObject};

pub const TAG_END: u16 = 0;
pub const TAG_SHOW_FRAME: u16 = 1;
pub const TAG_SET_BACKGROUND_COLOR: u16 = 9;
pub const TAG_DEFINE_SOUND: u16 = 14;
pub const TAG_START_SOUND: u16 = 15;
pub const TAG_DEFINE_BITS_LOSSLESS: u16 = 20;
pub const TAG_DEFINE_BITS_JPEG2: u16 = 21;
pub const TAG_PLACE_OBJECT_2: u16 = 26;
pub const TAG_REMOVE_OBJECT_2: u16 = 28;
pub const TAG_DEFINE_SHAPE_3: u16 = 32;
pub const TAG_DEFINE_BUTTON_2: u16 = 34;
pub const TAG_DEFINE_BITS_JPEG3: u16 = 35;
pub const TAG_DEFINE_BITS_LOSSLESS2: u16 = 36;
pub const TAG_DEFINE_SPRITE: u16 = 39;
pub const TAG_FRAME_LABEL: u16 = 43;
pub const TAG_FILE_ATTRIBUTES: u16 = 69;
pub const TAG_PLACE_OBJECT_3: u16 = 70;
pub const TAG_SYMBOL_CLASS: u16 = 76;
pub const TAG_METADATA: u16 = 77;
/// Original DoABC tag (SWF 9). Kept for inspection; the compiler emits DoABC2.
pub const TAG_DO_ABC: u16 = 72;
/// DoABC2 tag (SWF 10+), with an initialization flag and a name.
pub const TAG_DO_ABC2: u16 = 82;
pub const TAG_DEFINE_SHAPE_4: u16 = 83;
pub const TAG_DO_ACTION: u16 = 12;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    FileAttributes {
        action_script_3: bool,
        use_network: bool,
    },
    SetBackgroundColor(Color),
    DefineShape {
        id: u16,
        shape: Shape,
    },
    DefineSprite {
        id: u16,
        frame_count: u16,
        tags: Vec<Tag>,
    },
    /// 32-bit RGBA bitmap exported as zlib-compressed DefineBitsLossless2
    /// (`bitmapFormat = 5`, ARGB byte order).
    DefineBitsLossless2 {
        id: u16,
        width: u16,
        height: u16,
        rgba: Vec<u8>,
    },
    /// Original JPEG bytes exported without re-encoding.
    DefineBitsJPEG2 {
        id: u16,
        data: Vec<u8>,
    },
    /// JPEG with a zlib-compressed alpha channel (DefineBitsJPEG3).
    DefineBitsJPEG3 {
        id: u16,
        jpeg: Vec<u8>,
        alpha: Vec<u8>,
    },
    /// SWF sound character. `format`: 0 = PCM, 2 = MP3. `rate`: 0=5.5k,
    /// 1=11k, 2=22k, 3=44k. `size`: true = 16 bit. `channels`: 1 = stereo.
    DefineSound {
        id: u16,
        format: u8,
        rate: u8,
        size_16bit: bool,
        stereo: bool,
        sample_count: u32,
        data: Vec<u8>,
    },
    StartSound {
        sound_id: u16,
        loop_count: u16,
        in_point: Option<u32>,
        out_point: Option<u32>,
    },
    PlaceObject2(PlaceObject),
    RemoveObject2 {
        depth: u16,
    },
    ShowFrame,
    End,
    FrameLabel {
        label: String,
        named_anchor: bool,
    },
    Metadata {
        xml: String,
    },
    DoAbc2 {
        /// Bit 0 is the lazy-initialization flag: 1 = lazy, 0 = eager.
        flags: u32,
        name: String,
        data: Vec<u8>,
    },
    SymbolClass {
        links: Vec<(u16, String)>,
    },
    /// Raw AVM1 action bytes (`DoAction`). Exposed for the legacy runtime
    /// milestone; the AS3 pipeline must not emit this tag.
    DoAction(Vec<u8>),
}

impl Tag {
    pub fn code(&self) -> u16 {
        match self {
            Self::End => TAG_END,
            Self::ShowFrame => TAG_SHOW_FRAME,
            Self::SetBackgroundColor(_) => TAG_SET_BACKGROUND_COLOR,
            Self::PlaceObject2(_) => TAG_PLACE_OBJECT_2,
            Self::RemoveObject2 { .. } => TAG_REMOVE_OBJECT_2,
            Self::DefineShape { .. } => TAG_DEFINE_SHAPE_3,
            Self::DefineSprite { .. } => TAG_DEFINE_SPRITE,
            Self::DefineBitsLossless2 { .. } => TAG_DEFINE_BITS_LOSSLESS2,
            Self::DefineBitsJPEG2 { .. } => TAG_DEFINE_BITS_JPEG2,
            Self::DefineBitsJPEG3 { .. } => TAG_DEFINE_BITS_JPEG3,
            Self::DefineSound { .. } => TAG_DEFINE_SOUND,
            Self::StartSound { .. } => TAG_START_SOUND,
            Self::FrameLabel { .. } => TAG_FRAME_LABEL,
            Self::FileAttributes { .. } => TAG_FILE_ATTRIBUTES,
            Self::SymbolClass { .. } => TAG_SYMBOL_CLASS,
            Self::Metadata { .. } => TAG_METADATA,
            Self::DoAbc2 { .. } => TAG_DO_ABC2,
            Self::DoAction(_) => TAG_DO_ACTION,
        }
    }

    fn write_body(&self, out: &mut Vec<u8>) -> SwfResult<()> {
        let mut w = BitWriter::new();
        match self {
            Self::End | Self::ShowFrame => {}
            Self::SetBackgroundColor(color) => {
                out.extend_from_slice(&[color.r, color.g, color.b]);
            }
            Self::FileAttributes {
                action_script_3,
                use_network,
            } => {
                let mut flags = 0u32;
                if *action_script_3 {
                    flags |= 0x08;
                }
                if *use_network {
                    flags |= 0x01;
                }
                out.extend_from_slice(&flags.to_le_bytes());
            }
            Self::DefineShape { id, shape } => {
                shape.write_define_shape3(*id, out)?;
            }
            Self::DefineSprite {
                id,
                frame_count,
                tags,
            } => {
                out.extend_from_slice(&id.to_le_bytes());
                out.extend_from_slice(&frame_count.to_le_bytes());
                for tag in tags {
                    if matches!(tag, Self::End) {
                        continue;
                    }
                    write_tag(out, tag)?;
                }
                // A sprite's control tag list is terminated by its own End
                // record, separate from the root movie's End tag.
                write_tag(out, &Self::End)?;
            }
            Self::DefineBitsLossless2 {
                id,
                width,
                height,
                rgba,
            } => {
                if *width == 0
                    || *height == 0
                    || rgba.len() != (*width as usize * *height as usize * 4)
                {
                    return Err(crate::SwfError::Invalid(
                        "DefineBitsLossless2 dimensions do not match RGBA payload".into(),
                    ));
                }
                out.extend_from_slice(&id.to_le_bytes());
                out.push(5); // ARGB32
                out.extend_from_slice(&width.to_le_bytes());
                out.extend_from_slice(&height.to_le_bytes());
                // SWF stores ARGB, while decoder APIs hand us RGBA.
                let mut argb = Vec::with_capacity(rgba.len());
                for pixel in rgba.chunks_exact(4) {
                    argb.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
                }
                let compressed = zlib_compress(&argb)?;
                out.extend_from_slice(&compressed);
            }
            Self::DefineBitsJPEG2 { id, data } => {
                out.extend_from_slice(&id.to_le_bytes());
                out.extend_from_slice(data);
            }
            Self::DefineBitsJPEG3 { id, jpeg, alpha } => {
                out.extend_from_slice(&id.to_le_bytes());
                let offset = u32::try_from(jpeg.len())
                    .map_err(|_| crate::SwfError::Invalid("JPEG payload exceeds 4 GiB".into()))?;
                out.extend_from_slice(&offset.to_le_bytes());
                out.extend_from_slice(jpeg);
                let compressed = zlib_compress(alpha)?;
                out.extend_from_slice(&compressed);
            }
            Self::DefineSound {
                id,
                format,
                rate,
                size_16bit,
                stereo,
                sample_count,
                data,
            } => {
                out.extend_from_slice(&id.to_le_bytes());
                let flags = ((format & 0x0f) << 4)
                    | ((rate & 0x03) << 2)
                    | (u8::from(*size_16bit) << 1)
                    | u8::from(*stereo);
                out.push(flags);
                out.extend_from_slice(&sample_count.to_le_bytes());
                out.extend_from_slice(data);
            }
            Self::StartSound {
                sound_id,
                loop_count,
                in_point,
                out_point,
            } => {
                out.extend_from_slice(&sound_id.to_le_bytes());
                let mut flags = 0u8;
                if in_point.is_some() {
                    flags |= 0x01;
                }
                if out_point.is_some() {
                    flags |= 0x02;
                }
                if *loop_count > 1 {
                    flags |= 0x04;
                }
                out.push(flags);
                if let Some(point) = in_point {
                    out.extend_from_slice(&point.to_le_bytes());
                }
                if let Some(point) = out_point {
                    out.extend_from_slice(&point.to_le_bytes());
                }
                if *loop_count > 1 {
                    out.extend_from_slice(&loop_count.to_le_bytes());
                }
            }
            Self::PlaceObject2(place) => {
                place.write(&mut w);
                w.align();
                out.extend_from_slice(&w.into_bytes());
            }
            Self::RemoveObject2 { depth } => {
                out.extend_from_slice(&depth.to_le_bytes());
            }
            Self::FrameLabel {
                label,
                named_anchor,
            } => {
                // SWF strings are null-terminated byte strings; UTF-8 labels
                // are accepted by modern players.
                out.extend_from_slice(label.as_bytes());
                out.push(0);
                out.push(u8::from(*named_anchor));
            }
            Self::Metadata { xml } => {
                out.extend_from_slice(xml.as_bytes());
                out.push(0);
            }
            Self::DoAbc2 { flags, name, data } => {
                out.extend_from_slice(&flags.to_le_bytes());
                out.extend_from_slice(name.as_bytes());
                out.push(0);
                out.extend_from_slice(data);
            }
            Self::SymbolClass { links } => {
                out.extend_from_slice(&(links.len() as u16).to_le_bytes());
                for (id, name) in links {
                    out.extend_from_slice(&id.to_le_bytes());
                    out.extend_from_slice(name.as_bytes());
                    out.push(0);
                }
            }
            Self::DoAction(actions) => {
                out.extend_from_slice(actions);
                out.push(0); // ActionEnd
            }
        }
        Ok(())
    }
}

fn zlib_compress(bytes: &[u8]) -> SwfResult<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

/// Serializes one tag including its record header.
pub fn write_tag(out: &mut Vec<u8>, tag: &Tag) -> SwfResult<()> {
    let mut body = Vec::new();
    tag.write_body(&mut body)?;
    let code = tag.code();
    let length = body.len();

    if length < 0x3f {
        let header = (code << 6) | length as u16;
        out.extend_from_slice(&header.to_le_bytes());
    } else {
        let header = (code << 6) | 0x3f;
        out.extend_from_slice(&header.to_le_bytes());
        out.extend_from_slice(&(length as u32).to_le_bytes());
    }
    out.extend_from_slice(&body);
    Ok(())
}

/// Human-readable tag name for the inspector and console output.
pub fn tag_name(code: u16) -> &'static str {
    match code {
        TAG_END => "End",
        TAG_SHOW_FRAME => "ShowFrame",
        TAG_SET_BACKGROUND_COLOR => "SetBackgroundColor",
        TAG_PLACE_OBJECT_2 => "PlaceObject2",
        TAG_REMOVE_OBJECT_2 => "RemoveObject2",
        TAG_DEFINE_SHAPE_3 => "DefineShape3",
        TAG_DEFINE_SPRITE => "DefineSprite",
        TAG_FRAME_LABEL => "FrameLabel",
        TAG_FILE_ATTRIBUTES => "FileAttributes",
        TAG_SYMBOL_CLASS => "SymbolClass",
        TAG_METADATA => "Metadata",
        TAG_DO_ABC => "DoABC",
        TAG_DO_ABC2 => "DoABC2",
        TAG_DO_ACTION => "DoAction",
        TAG_DEFINE_SOUND => "DefineSound",
        TAG_START_SOUND => "StartSound",
        TAG_DEFINE_BITS_JPEG2 => "DefineBitsJPEG2",
        TAG_DEFINE_BITS_JPEG3 => "DefineBitsJPEG3",
        TAG_DEFINE_BITS_LOSSLESS => "DefineBitsLossless",
        TAG_DEFINE_BITS_LOSSLESS2 => "DefineBitsLossless2",
        TAG_DEFINE_SHAPE_4 => "DefineShape4",
        TAG_DEFINE_BUTTON_2 => "DefineButton2",
        TAG_PLACE_OBJECT_3 => "PlaceObject3",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Color;

    #[test]
    fn short_tag_header_layout() {
        let mut out = Vec::new();
        write_tag(&mut out, &Tag::ShowFrame).unwrap();
        // code 1 << 6 = 0x40, little endian -> [0x40, 0x00]
        assert_eq!(out, vec![0x40, 0x00]);
    }

    #[test]
    fn long_tag_header_layout() {
        let tag = Tag::Metadata {
            xml: "x".repeat(100),
        };
        let mut out = Vec::new();
        write_tag(&mut out, &tag).unwrap();
        let header = u16::from_le_bytes([out[0], out[1]]);
        assert_eq!(header >> 6, TAG_METADATA);
        assert_eq!(header & 0x3f, 0x3f);
        let len = u32::from_le_bytes([out[2], out[3], out[4], out[5]]);
        assert_eq!(len, 101);
    }

    #[test]
    fn end_tag_is_two_zero_bytes() {
        let mut out = Vec::new();
        write_tag(&mut out, &Tag::End).unwrap();
        assert_eq!(out, vec![0, 0]);
    }

    #[test]
    fn background_color_has_no_alpha() {
        let mut out = Vec::new();
        write_tag(&mut out, &Tag::SetBackgroundColor(Color::rgb(1, 2, 3))).unwrap();
        assert_eq!(&out[2..], &[1, 2, 3]);
    }
}
