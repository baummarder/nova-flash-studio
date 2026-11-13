//! Top-level SWF movie assembly: header, frame rate, compression and tags.

use std::io::Write;

use crate::bits::BitWriter;
use crate::error::{SwfError, SwfResult};
use crate::tags::{TAG_FILE_ATTRIBUTES, Tag, write_tag};
use crate::types::{Color, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    None,
    #[default]
    Zlib,
}

/// A complete SWF movie description.
#[derive(Debug, Clone)]
pub struct Movie {
    pub version: u8,
    pub width: f32,
    pub height: f32,
    pub frame_rate: f32,
    pub background: Color,
    pub tags: Vec<Tag>,
}

impl Movie {
    /// Creates an AVM2-ready movie (SWF 9+ FileAttributes with the AS3 flag).
    pub fn new_avm2(width: f32, height: f32, frame_rate: f32, version: u8) -> Self {
        Self {
            version: version.max(9),
            width,
            height,
            frame_rate,
            background: Color::rgb(0, 0, 0),
            tags: vec![
                Tag::FileAttributes {
                    action_script_3: true,
                    use_network: false,
                },
                Tag::SetBackgroundColor(Color::rgb(0, 0, 0)),
            ],
        }
    }

    /// Creates a legacy AVM1 movie.
    pub fn new_avm1(width: f32, height: f32, frame_rate: f32, version: u8) -> Self {
        Self {
            version,
            width,
            height,
            frame_rate,
            background: Color::rgb(0, 0, 0),
            tags: vec![
                Tag::FileAttributes {
                    action_script_3: false,
                    use_network: false,
                },
                Tag::SetBackgroundColor(Color::rgb(0, 0, 0)),
            ],
        }
    }

    pub fn set_background(&mut self, color: Color) {
        self.background = color;
        if let Some(Tag::SetBackgroundColor(existing)) = self
            .tags
            .iter_mut()
            .find(|t| matches!(t, Tag::SetBackgroundColor(_)))
        {
            *existing = color;
        } else {
            self.tags.insert(0, Tag::SetBackgroundColor(color));
        }
    }

    pub fn frame_count(&self) -> u16 {
        self.tags
            .iter()
            .filter(|tag| matches!(tag, Tag::ShowFrame))
            .count()
            .min(u16::MAX as usize) as u16
    }

    /// Serializes the movie to a complete `.swf` byte stream.
    pub fn to_bytes(&self, compression: Compression) -> SwfResult<Vec<u8>> {
        let body = self.body()?;
        let total_length = 8usize
            .checked_add(body.len())
            .ok_or_else(|| SwfError::Range("SWF length overflow".into()))?;
        let declared =
            u32::try_from(total_length).map_err(|_| SwfError::Range("SWF exceeds 4 GiB".into()))?;

        let mut out = Vec::with_capacity(total_length.min(16 * 1024 * 1024));
        match compression {
            Compression::None => {
                out.extend_from_slice(b"FWS");
                out.push(self.version);
                out.extend_from_slice(&declared.to_le_bytes());
                out.extend_from_slice(&body);
            }
            Compression::Zlib => {
                out.extend_from_slice(b"CWS");
                out.push(self.version);
                out.extend_from_slice(&declared.to_le_bytes());
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&body)?;
                let compressed = encoder.finish()?;
                out.extend_from_slice(&compressed);
            }
        }
        Ok(out)
    }

    fn body(&self) -> SwfResult<Vec<u8>> {
        if !(1.0..=8192.0).contains(&self.width) || !(1.0..=8192.0).contains(&self.height) {
            return Err(SwfError::Range(format!(
                "stage size {}x{} is outside 1..=8192 px",
                self.width, self.height
            )));
        }
        if !(0.01..=255.99).contains(&self.frame_rate) {
            return Err(SwfError::Range(format!(
                "frame rate {} is unsupported",
                self.frame_rate
            )));
        }
        if self
            .tags
            .iter()
            .any(|tag| matches!(tag, Tag::FileAttributes { .. }))
            && !matches!(self.tags.first(), Some(Tag::FileAttributes { .. }))
        {
            return Err(SwfError::Invalid(
                "FileAttributes must be the first tag".into(),
            ));
        }

        let mut body = Vec::new();
        let mut w = BitWriter::new();
        Rect::from_size(self.width, self.height).write(&mut w);
        w.align();
        body.extend_from_slice(&w.into_bytes());

        let rate = (self.frame_rate * 256.0).round() as u32;
        if rate > u16::MAX as u32 {
            return Err(SwfError::Range("frame rate too large".into()));
        }
        body.extend_from_slice(&(rate as u16).to_le_bytes());
        body.extend_from_slice(&self.frame_count().to_le_bytes());

        for tag in &self.tags {
            if matches!(tag, Tag::End) {
                continue;
            }
            write_tag(&mut body, tag)?;
        }
        write_tag(&mut body, &Tag::End)?;
        Ok(body)
    }
}

/// Ensures the first tag carries FileAttributes when a movie was assembled
/// manually. Used by tests and by callers that build tag lists by hand.
pub fn ensure_file_attributes(movie: &mut Movie, action_script_3: bool) {
    if !matches!(movie.tags.first(), Some(Tag::FileAttributes { .. })) {
        movie.tags.retain(|t| t.code() != TAG_FILE_ATTRIBUTES);
        movie.tags.insert(
            0,
            Tag::FileAttributes {
                action_script_3,
                use_network: false,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;
    use crate::tags::Tag;
    use crate::types::Matrix;

    fn sample_movie() -> Movie {
        let mut movie = Movie::new_avm2(550.0, 400.0, 24.0, 10);
        movie.set_background(Color::rgb(16, 18, 24));
        movie.tags.push(Tag::DefineShape {
            id: 1,
            shape: Shape::rect(100.0, 50.0, Color::rgb(255, 128, 0)),
        });
        movie.tags.push(Tag::PlaceObject2(
            crate::types::PlaceObject::new(1)
                .character(1)
                .matrix(Matrix::from_transform(1.0, 1.0, 0.0, 100.0, 100.0))
                .named("Block"),
        ));
        movie.tags.push(Tag::ShowFrame);
        movie
    }

    #[test]
    fn uncompressed_header_is_consistent() {
        let bytes = sample_movie().to_bytes(Compression::None).unwrap();
        assert_eq!(&bytes[0..3], b"FWS");
        assert_eq!(bytes[3], 10);
        let declared = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(declared, bytes.len());
        assert_eq!(&bytes[bytes.len() - 2..], &[0, 0]);
    }

    #[test]
    fn compressed_header_declares_uncompressed_size() {
        let movie = sample_movie();
        let bytes = movie.to_bytes(Compression::Zlib).unwrap();
        let info = crate::inspect::inspect(&bytes).unwrap();
        assert!(info.compressed);
        assert_eq!(info.width, 550.0);
        assert_eq!(info.height, 400.0);
    }

    #[test]
    fn frame_count_counts_show_frames() {
        let mut movie = sample_movie();
        movie.tags.push(Tag::ShowFrame);
        assert_eq!(movie.frame_count(), 2);
    }

    #[test]
    fn invalid_stage_size_is_rejected() {
        let mut movie = sample_movie();
        movie.width = 0.0;
        assert!(movie.to_bytes(Compression::None).is_err());
    }
}
