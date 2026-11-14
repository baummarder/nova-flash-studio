//! Tilemap and tileset document model (format version 4).
//!
//! A tilemap is stored as [`crate::model::ObjectKind::Tilemap`] so it
//! participates in the normal object pipeline: placement, transform,
//! selection, undo and (where supported) export. The bitmap it samples is a
//! regular [`crate::model::Asset`]; slicing and per-tile metadata live in
//! [`Tileset`] records on the project so several tilemaps can share one
//! tileset without duplicating image data.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One grid layer inside a tilemap. Cells are stored row-major; `0` means
/// empty, any other value is `tile_index + 1` into the tileset grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TileLayer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    /// 0.0..=1.0, applied when rendering.
    pub opacity: f32,
    pub cells: Vec<u16>,
}

impl Default for TileLayer {
    fn default() -> Self {
        Self {
            name: String::new(),
            visible: true,
            locked: false,
            opacity: 1.0,
            cells: Vec::new(),
        }
    }
}

impl TileLayer {
    pub fn new(name: impl Into<String>, columns: usize, rows: usize) -> Self {
        Self {
            name: name.into(),
            cells: vec![0; columns.saturating_mul(rows)],
            ..Default::default()
        }
    }

    /// Tile index at (x, y); `None` outside the grid.
    pub fn get(&self, x: i32, y: i32, columns: usize) -> Option<u16> {
        if x < 0 || y < 0 || x as usize >= columns {
            return None;
        }
        self.cells.get(y as usize * columns + x as usize).copied()
    }

    pub fn set(&mut self, x: i32, y: i32, columns: usize, tile: u16) {
        if x < 0 || y < 0 || x as usize >= columns {
            return;
        }
        if let Some(cell) = self.cells.get_mut(y as usize * columns + x as usize) {
            *cell = tile;
        }
    }
}

/// Grid data of a tilemap object. The owning [`crate::model::ObjectInstance`]
/// supplies position/rotation/scale through its transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TilemapData {
    /// Tileset id from `Project::tilesets`.
    pub tileset: Uuid,
    /// Tile size in pixels inside the tileset image.
    pub tile_width: f32,
    pub tile_height: f32,
    pub columns: usize,
    pub rows: usize,
    pub layers: Vec<TileLayer>,
}

impl Default for TilemapData {
    fn default() -> Self {
        Self {
            tileset: Uuid::nil(),
            tile_width: 16.0,
            tile_height: 16.0,
            columns: 0,
            rows: 0,
            layers: Vec::new(),
        }
    }
}

impl TilemapData {
    /// World-space size of the grid in pixels.
    pub fn width(&self) -> f32 {
        self.columns as f32 * self.tile_width
    }

    pub fn height(&self) -> f32 {
        self.rows as f32 * self.tile_height
    }

    /// Ensures every layer's cell buffer matches `columns * rows`.
    pub fn normalize(&mut self) {
        let len = self.columns.saturating_mul(self.rows);
        for layer in &mut self.layers {
            layer.cells.resize(len, 0);
            layer.cells.truncate(len);
        }
    }
}

/// Slicing and per-tile metadata for one bitmap asset. The image itself is
/// never duplicated: `asset` references the shared [`crate::model::Asset`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tileset {
    pub id: Uuid,
    pub name: String,
    /// Bitmap asset containing the tiles, row-major from the top-left.
    pub asset: Uuid,
    /// Tile size in pixels; `columns` is derived from the asset width.
    pub tile_width: u32,
    pub tile_height: u32,
    /// Number of tile columns in the source image.
    pub columns: u32,
    /// Pixel margin between the image edge and the first/last tile.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub margin_x: u32,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub margin_y: u32,
    /// Pixel gap between neighbouring tiles (0 = tightly packed).
    #[serde(skip_serializing_if = "is_zero", default)]
    pub spacing_x: u32,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub spacing_y: u32,
    /// Per-tile collision flag, indexed by tile index (0-based, matching
    /// `cells` value minus one). Missing entries mean "not solid".
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub solid: Vec<bool>,
    /// Optional per-tile display names, same indexing as `solid`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub names: Vec<String>,
}

impl Default for Tileset {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            asset: Uuid::nil(),
            tile_width: 16,
            tile_height: 16,
            columns: 0,
            margin_x: 0,
            margin_y: 0,
            spacing_x: 0,
            spacing_y: 0,
            solid: Vec::new(),
            names: Vec::new(),
        }
    }
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Tileset {
    /// Horizontal stride between tile origins in the source image.
    pub fn stride_x(&self) -> u32 {
        self.tile_width.saturating_add(self.spacing_x)
    }

    /// Vertical stride between tile origins in the source image.
    pub fn stride_y(&self) -> u32 {
        self.tile_height.saturating_add(self.spacing_y)
    }

    /// Number of tile columns that fit `width_px` with the configured
    /// margins and spacing.
    pub fn columns_for(&self, width_px: u32) -> u32 {
        let stride = self.stride_x();
        if stride == 0 {
            return 0;
        }
        let usable = width_px.saturating_sub(self.margin_x.saturating_mul(2));
        if usable == 0 {
            return 0;
        }
        // usable + spacing accounts for the gap after the last tile.
        (usable + self.spacing_x) / stride
    }

    /// Number of tile rows that fit `height_px` with margins and spacing.
    pub fn rows_for(&self, height_px: u32) -> u32 {
        let stride = self.stride_y();
        if stride == 0 {
            return 0;
        }
        let usable = height_px.saturating_sub(self.margin_y.saturating_mul(2));
        if usable == 0 {
            return 0;
        }
        (usable + self.spacing_y) / stride
    }

    /// Total number of tiles inside an image of `width_px`×`height_px`.
    /// Uses the configured `columns` when set, otherwise derives the column
    /// count from the image width.
    pub fn tile_count(&self, asset_width_px: u32, asset_height_px: u32) -> usize {
        if self.tile_width == 0 || self.tile_height == 0 {
            return 0;
        }
        let columns = if self.columns > 0 {
            self.columns
        } else {
            self.columns_for(asset_width_px)
        };
        columns as usize * self.rows_for(asset_height_px) as usize
    }

    /// Pixel rect `(x, y, w, h)` of tile `index` (0-based, row-major) inside
    /// an image of `img_w`×`img_h`. `None` when the tile falls outside the
    /// image or the slicing parameters are invalid.
    pub fn tile_rect(&self, index: usize, img_w: u32, img_h: u32) -> Option<(u32, u32, u32, u32)> {
        if self.tile_width == 0 || self.tile_height == 0 || self.columns == 0 {
            return None;
        }
        let col = index as u32 % self.columns;
        let row = index as u32 / self.columns;
        let x = self.margin_x + col * self.stride_x();
        let y = self.margin_y + row * self.stride_y();
        if x + self.tile_width > img_w || y + self.tile_height > img_h {
            return None;
        }
        Some((x, y, self.tile_width, self.tile_height))
    }

    /// Whether tile `index` (0-based) is solid. Out-of-range means false.
    pub fn is_solid(&self, index: usize) -> bool {
        self.solid.get(index).copied().unwrap_or(false)
    }

    /// Tile display name, falling back to `Tile N`.
    pub fn tile_name(&self, index: usize) -> String {
        self.names
            .get(index)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("Tile {}", index + 1))
    }
}
