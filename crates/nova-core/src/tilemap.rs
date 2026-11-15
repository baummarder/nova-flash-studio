//! Tilemap editing operations shared by the editor UI, the AI operations
//! and the test-suite. Everything here works on the plain document model —
//! no egui, no GPU — so behaviour is identical in every context and fully
//! unit-testable. Undo/redo is provided by the caller's project snapshot.

use crate::model::{TileLayer, TilemapData, Tileset};

/// Result of a paint operation that may span multiple cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintTool {
    /// Single cell (or `brush` stamp centred on the cell).
    Pencil,
    /// Clears cells (`tile` ignored).
    Eraser,
    /// Inclusive rectangle between two cells.
    Rect,
    /// Flood fill over equal-valued cells.
    Flood,
    /// Read the tile under the cursor (no mutation).
    Eyedrop,
    /// Replace every occurrence of `from` with `to` in the layer.
    Replace,
}

/// A rectangular stamp of tile indices (row-major), used for multi-tile
/// brushes. `0` entries are transparent (keep existing cell).
#[derive(Debug, Clone, PartialEq)]
pub struct BrushStamp {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u16>,
}

impl BrushStamp {
    pub fn single(tile: u16) -> Self {
        Self {
            width: 1,
            height: 1,
            cells: vec![tile],
        }
    }

    /// Extracts a `w`×`h` stamp from a tileset starting at tile `index`.
    /// Tiles are read row-major across the tileset's columns; out-of-range
    /// reads produce empty (0) cells.
    pub fn from_tileset(
        _tileset: &Tileset,
        tile_count: usize,
        index: usize,
        width: usize,
        height: usize,
    ) -> Self {
        let mut cells = vec![0u16; width * height];
        for y in 0..height {
            for x in 0..width {
                let tile = index + y * width + x;
                if tile < tile_count {
                    cells[y * width + x] = tile as u16 + 1;
                }
            }
        }
        Self {
            width,
            height,
            cells,
        }
    }
}

/// Paints `stamp` with its top-left at cell (x, y). `tile` semantics:
/// stamp cells are already 1-based tile indices; `0` keeps the existing
/// cell. Returns the number of cells changed.
pub fn paint_stamp(
    map: &mut TilemapData,
    layer: usize,
    x: i32,
    y: i32,
    stamp: &BrushStamp,
) -> usize {
    let Some(target) = map.layers.get_mut(layer) else {
        return 0;
    };
    if target.locked {
        return 0;
    }
    let mut changed = 0;
    for sy in 0..stamp.height as i32 {
        for sx in 0..stamp.width as i32 {
            let value = stamp.cells[sy as usize * stamp.width + sx as usize];
            if value == 0 {
                continue;
            }
            let cx = x + sx;
            let cy = y + sy;
            if cx < 0 || cy < 0 || cx as usize >= map.columns || cy as usize >= map.rows {
                continue;
            }
            let idx = cy as usize * map.columns + cx as usize;
            if target.cells.get(idx).copied() != Some(value)
                && let Some(cell) = target.cells.get_mut(idx)
            {
                *cell = value;
                changed += 1;
            }
        }
    }
    changed
}

/// Applies `tool` between cells (x0,y0) and (x1,y1) on `layer`.
/// `tile` is the 1-based tile index (0 = empty for pencil/rect/flood).
/// `from`/`to` are used by [`PaintTool::Replace`]. Returns changed cells.
pub fn paint(
    map: &mut TilemapData,
    layer: usize,
    tool: PaintTool,
    rect: (i32, i32, i32, i32),
    tile: u16,
    from: u16,
    to: u16,
) -> Result<usize, String> {
    let (x0, y0, x1, y1) = rect;
    let Some(target) = map.layers.get(layer) else {
        return Err(format!("Ebene {layer} existiert nicht"));
    };
    if target.locked {
        return Err("Ebene ist gesperrt".into());
    }
    match tool {
        PaintTool::Pencil => {
            if tile == 0 {
                Ok(set_cell(map, layer, x0, y0, 0) as usize)
            } else {
                Ok(paint_stamp(map, layer, x0, y0, &BrushStamp::single(tile)))
            }
        }
        PaintTool::Eraser => Ok(set_cell(map, layer, x0, y0, 0) as usize),
        PaintTool::Rect => {
            let (ax, bx) = (x0.min(x1), x0.max(x1));
            let (ay, by) = (y0.min(y1), y0.max(y1));
            let mut changed = 0;
            for y in ay..=by {
                for x in ax..=bx {
                    changed += set_cell(map, layer, x, y, tile) as usize;
                }
            }
            Ok(changed)
        }
        PaintTool::Flood => flood_fill(map, layer, x0, y0, tile),
        PaintTool::Eyedrop => Ok(0), // handled by caller via `sample`
        PaintTool::Replace => Ok(replace_tiles(map, layer, from, to)),
    }
}

/// Writes `tile` (0 clears) into one cell; returns whether it changed.
fn set_cell(map: &mut TilemapData, layer: usize, x: i32, y: i32, tile: u16) -> bool {
    if x < 0 || y < 0 || x as usize >= map.columns || y as usize >= map.rows {
        return false;
    }
    let Some(target) = map.layers.get_mut(layer) else {
        return false;
    };
    let idx = y as usize * map.columns + x as usize;
    if target.cells.get(idx).copied() == Some(tile) {
        return false;
    }
    target.cells[idx] = tile;
    true
}

/// Tile under cell (x, y); `None` outside the grid.
pub fn sample(map: &TilemapData, layer: usize, x: i32, y: i32) -> Option<u16> {
    let target = map.layers.get(layer)?;
    if x < 0 || y < 0 || x as usize >= map.columns || y as usize >= map.rows {
        return None;
    }
    target
        .cells
        .get(y as usize * map.columns + x as usize)
        .copied()
}

/// 4-way flood fill from (x, y) replacing the contiguous region of the
/// starting value with `tile`.
fn flood_fill(
    map: &mut TilemapData,
    layer: usize,
    x: i32,
    y: i32,
    tile: u16,
) -> Result<usize, String> {
    if x < 0 || y < 0 || x as usize >= map.columns || y as usize >= map.rows {
        return Ok(0);
    }
    let start = sample(map, layer, x, y).unwrap_or(0);
    if start == tile {
        return Ok(0);
    }
    let target = map.layers.get_mut(layer).unwrap();
    let mut stack = vec![(x, y)];
    let mut changed = 0usize;
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx as usize >= map.columns || cy as usize >= map.rows {
            continue;
        }
        let idx = cy as usize * map.columns + cx as usize;
        if target.cells.get(idx).copied() != Some(start) {
            continue;
        }
        target.cells[idx] = tile;
        changed += 1;
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
    Ok(changed)
}

/// Replaces every cell equal to `from` with `to` on `layer`.
fn replace_tiles(map: &mut TilemapData, layer: usize, from: u16, to: u16) -> usize {
    let Some(target) = map.layers.get_mut(layer) else {
        return 0;
    };
    if from == to {
        return 0;
    }
    let mut changed = 0;
    for cell in target.cells.iter_mut() {
        if *cell == from {
            *cell = to;
            changed += 1;
        }
    }
    changed
}

/// A rectangular snapshot of one tile layer's cells (row-major, `0` = empty).
#[derive(Debug, Clone, PartialEq)]
pub struct TileRegion {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u16>,
}

/// Copies the inclusive rectangle (x0,y0)-(x1,y1) of `layer` into a
/// [`TileRegion`]. Out-of-bounds edges are clipped; `None` when nothing
/// inside the grid was selected.
pub fn copy_region(
    map: &TilemapData,
    layer: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Option<TileRegion> {
    let target = map.layers.get(layer)?;
    let (ax, bx) = (x0.min(x1).max(0), x0.max(x1).min(map.columns as i32 - 1));
    let (ay, by) = (y0.min(y1).max(0), y0.max(y1).min(map.rows as i32 - 1));
    if ax > bx || ay > by || map.columns == 0 || map.rows == 0 {
        return None;
    }
    let width = (bx - ax + 1) as usize;
    let height = (by - ay + 1) as usize;
    let mut cells = vec![0u16; width * height];
    for ry in 0..height {
        for rx in 0..width {
            let sx = ax as usize + rx;
            let sy = ay as usize + ry;
            cells[ry * width + rx] = target
                .cells
                .get(sy * map.columns + sx)
                .copied()
                .unwrap_or(0);
        }
    }
    Some(TileRegion {
        x: ax,
        y: ay,
        width,
        height,
        cells,
    })
}

/// Pastes `region` with its top-left at cell (x, y) on `layer`, overwriting
/// every covered cell (including empty ones). Returns changed cells.
pub fn paste_region(
    map: &mut TilemapData,
    layer: usize,
    x: i32,
    y: i32,
    region: &TileRegion,
) -> usize {
    let Some(target) = map.layers.get_mut(layer) else {
        return 0;
    };
    if target.locked {
        return 0;
    }
    let mut changed = 0;
    for ry in 0..region.height as i32 {
        for rx in 0..region.width as i32 {
            let cx = x + rx;
            let cy = y + ry;
            if cx < 0 || cy < 0 || cx as usize >= map.columns || cy as usize >= map.rows {
                continue;
            }
            let value = region.cells[ry as usize * region.width + rx as usize];
            let idx = cy as usize * map.columns + cx as usize;
            if target.cells.get(idx).copied() != Some(value) {
                target.cells[idx] = value;
                changed += 1;
            }
        }
    }
    changed
}

/// Clears the inclusive rectangle (x0,y0)-(x1,y1) on `layer`.
/// Returns changed cells.
pub fn clear_region(
    map: &mut TilemapData,
    layer: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> usize {
    let (ax, bx) = (x0.min(x1), x0.max(x1));
    let (ay, by) = (y0.min(y1), y0.max(y1));
    let mut changed = 0;
    for y in ay..=by {
        for x in ax..=bx {
            changed += set_cell(map, layer, x, y, 0) as usize;
        }
    }
    changed
}

/// Flips the cells inside the inclusive rectangle horizontally and/or
/// vertically, in place. Returns changed cells.
pub fn flip_region(
    map: &mut TilemapData,
    layer: usize,
    rect: (i32, i32, i32, i32),
    flip_h: bool,
    flip_v: bool,
) -> usize {
    let (x0, y0, x1, y1) = rect;
    if !flip_h && !flip_v {
        return 0;
    }
    let Some(target) = map.layers.get(layer) else {
        return 0;
    };
    if target.locked {
        return 0;
    }
    let (ax, bx) = (x0.min(x1).max(0), x0.max(x1).min(map.columns as i32 - 1));
    let (ay, by) = (y0.min(y1).max(0), y0.max(y1).min(map.rows as i32 - 1));
    if ax > bx || ay > by {
        return 0;
    }
    let w = (bx - ax + 1) as usize;
    let h = (by - ay + 1) as usize;
    let mut snapshot = vec![0u16; w * h];
    for ry in 0..h {
        for rx in 0..w {
            let sx = ax as usize + rx;
            let sy = ay as usize + ry;
            snapshot[ry * w + rx] = target
                .cells
                .get(sy * map.columns + sx)
                .copied()
                .unwrap_or(0);
        }
    }
    let Some(target) = map.layers.get_mut(layer) else {
        return 0;
    };
    let mut changed = 0;
    for ry in 0..h {
        for rx in 0..w {
            let (fx, fy) = if flip_h {
                (w - 1 - rx, if flip_v { h - 1 - ry } else { ry })
            } else {
                (rx, if flip_v { h - 1 - ry } else { ry })
            };
            let value = snapshot[fy * w + fx];
            let idx = (ay as usize + ry) * map.columns + ax as usize + rx;
            if target.cells.get(idx).copied() != Some(value) {
                target.cells[idx] = value;
                changed += 1;
            }
        }
    }
    changed
}

/// Mirrors a brush stamp in place (used by the editor's flip toggles).
pub fn flip_stamp(stamp: &mut BrushStamp, flip_h: bool, flip_v: bool) {
    if !flip_h && !flip_v {
        return;
    }
    let (w, h) = (stamp.width, stamp.height);
    let mut out = vec![0u16; w * h];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (
                if flip_h { w - 1 - x } else { x },
                if flip_v { h - 1 - y } else { y },
            );
            out[y * w + x] = stamp.cells[fy * w + fx];
        }
    }
    stamp.cells = out;
}

/// Mirrors a copied region's cell data in place (width/height unchanged).
pub fn flip_region_cells(region: &mut TileRegion, flip_h: bool, flip_v: bool) {
    if !flip_h && !flip_v {
        return;
    }
    let (w, h) = (region.width, region.height);
    let mut out = vec![0u16; w * h];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (
                if flip_h { w - 1 - x } else { x },
                if flip_v { h - 1 - y } else { y },
            );
            out[y * w + x] = region.cells[fy * w + fx];
        }
    }
    region.cells = out;
}

/// Adds an empty named tile layer at `index` (clamped to the end).
pub fn add_layer(map: &mut TilemapData, name: &str, index: Option<usize>) -> usize {
    let layer = TileLayer::new(name, map.columns, map.rows);
    let at = index.unwrap_or(map.layers.len()).min(map.layers.len());
    map.layers.insert(at, layer);
    at
}

/// Removes tile layer `index`; refuses to remove the last remaining layer.
/// Returns the removed layer name on success.
pub fn remove_layer(map: &mut TilemapData, index: usize) -> Result<String, String> {
    if map.layers.len() <= 1 {
        return Err("Eine Kachelkarte braucht mindestens eine Ebene.".into());
    }
    if index >= map.layers.len() {
        return Err(format!("Ebene {index} existiert nicht"));
    }
    Ok(map.layers.remove(index).name)
}

/// Creates a tilemap object payload: `columns`×`rows` grid, `layers`
/// empty named layers.
pub fn new_tilemap(
    tileset: uuid::Uuid,
    tile_width: f32,
    tile_height: f32,
    columns: usize,
    rows: usize,
    layer_names: &[&str],
) -> TilemapData {
    let mut map = TilemapData {
        tileset,
        tile_width,
        tile_height,
        columns,
        rows,
        layers: Vec::new(),
    };
    for (index, name) in layer_names.iter().enumerate() {
        let mut layer = TileLayer::new(
            if name.is_empty() {
                format!("Ebene {}", index + 1)
            } else {
                (*name).to_string()
            },
            columns,
            rows,
        );
        layer.cells.resize(columns * rows, 0);
        map.layers.push(layer);
    }
    if map.layers.is_empty() {
        map.layers.push(TileLayer::new("Ebene 1", columns, rows));
    }
    map.normalize();
    map
}

/// Resizes a tilemap grid, anchoring at the top-left. Existing cells keep
/// their values; new areas are empty. Layer buffers are rebuilt.
pub fn resize(map: &mut TilemapData, columns: usize, rows: usize) {
    let old_columns = map.columns;
    for layer in &mut map.layers {
        let mut cells = vec![0u16; columns * rows];
        for y in 0..rows.min(map.rows) {
            for x in 0..columns.min(old_columns) {
                cells[y * columns + x] = layer.cells[y * old_columns + x];
            }
        }
        layer.cells = cells;
    }
    map.columns = columns;
    map.rows = rows;
    map.normalize();
}

/// Number of tiles that fit a bitmap of `width`×`height` pixels.
pub fn tileset_columns(width_px: u32, tile_width: u32) -> u32 {
    if tile_width == 0 {
        return 0;
    }
    width_px / tile_width
}

/// Total tile count for a tileset given the asset pixel size.
pub fn tileset_tile_count(tileset: &Tileset, asset_width: u32, asset_height: u32) -> usize {
    tileset.tile_count(asset_width, asset_height)
}

/// Validates a tileset configuration; returns an error message.
pub fn validate_tileset(
    tileset: &Tileset,
    asset_width: u32,
    asset_height: u32,
) -> Result<(), String> {
    if tileset.tile_width == 0 || tileset.tile_height == 0 {
        return Err("Kachelgröße muss > 0 sein".into());
    }
    if tileset_columns(asset_width, tileset.tile_width) == 0 {
        return Err(format!(
            "Bild ist schmaler als eine Kachel ({}px < {}px)",
            asset_width, tileset.tile_width
        ));
    }
    if asset_height / tileset.tile_height == 0 {
        return Err(format!(
            "Bild ist niedriger als eine Kachel ({}px < {}px)",
            asset_height, tileset.tile_height
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;

    use crate::model::Asset;
    use uuid::Uuid;

    fn map() -> TilemapData {
        new_tilemap(Uuid::new_v4(), 16.0, 16.0, 4, 3, &["Ground", "Decor"])
    }

    #[test]
    fn pencil_and_eraser_paint_cells() {
        let mut m = map();
        assert_eq!(
            paint(&mut m, 0, PaintTool::Pencil, (1, 1, 1, 1), 5, 0, 0).unwrap(),
            1
        );
        assert_eq!(sample(&m, 0, 1, 1), Some(5));
        assert_eq!(sample(&m, 0, 1, 2), Some(0));
        assert_eq!(
            paint(&mut m, 0, PaintTool::Eraser, (1, 1, 1, 1), 0, 0, 0).unwrap(),
            1
        );
        assert_eq!(sample(&m, 0, 1, 1), Some(0));
    }

    #[test]
    fn rect_fill_is_inclusive() {
        let mut m = map();
        paint(&mut m, 0, PaintTool::Rect, (0, 0, 2, 1), 3, 0, 0).unwrap();
        for y in 0..=1 {
            for x in 0..=2 {
                assert_eq!(sample(&m, 0, x, y), Some(3));
            }
        }
        assert_eq!(sample(&m, 0, 3, 0), Some(0));
        assert_eq!(sample(&m, 0, 0, 2), Some(0));
    }

    #[test]
    fn flood_fill_respects_region_boundary() {
        let mut m = map();
        // Fill left half with tile 1 via rect, right half stays empty.
        paint(&mut m, 0, PaintTool::Rect, (0, 0, 1, 2), 1, 0, 0).unwrap();
        paint(&mut m, 0, PaintTool::Flood, (0, 0, 0, 0), 7, 0, 0).unwrap();
        assert_eq!(sample(&m, 0, 1, 2), Some(7));
        assert_eq!(sample(&m, 0, 2, 0), Some(0)); // untouched
        // Flood on empty region fills the rest.
        paint(&mut m, 0, PaintTool::Flood, (3, 0, 3, 0), 9, 0, 0).unwrap();
        assert_eq!(sample(&m, 0, 3, 2), Some(9));
    }

    #[test]
    fn replace_swaps_all_matching_cells() {
        let mut m = map();
        paint(&mut m, 0, PaintTool::Rect, (0, 0, 3, 2), 4, 0, 0).unwrap();
        assert_eq!(
            paint(&mut m, 0, PaintTool::Replace, (0, 0, 0, 0), 0, 4, 8).unwrap(),
            12
        );
        assert_eq!(sample(&m, 0, 3, 2), Some(8));
        assert_eq!(sample(&m, 0, 0, 0), Some(8));
    }

    #[test]
    fn locked_layer_rejects_painting() {
        let mut m = map();
        m.layers[0].locked = true;
        assert!(paint(&mut m, 0, PaintTool::Pencil, (0, 0, 0, 0), 1, 0, 0).is_err());
        assert_eq!(paint_stamp(&mut m, 0, 0, 0, &BrushStamp::single(1)), 0);
    }

    #[test]
    fn brush_stamp_honours_transparent_cells() {
        let mut m = map();
        paint(&mut m, 0, PaintTool::Pencil, (0, 0, 0, 0), 2, 0, 0).unwrap();
        let stamp = BrushStamp {
            width: 2,
            height: 1,
            cells: vec![0, 9],
        };
        paint_stamp(&mut m, 0, 0, 0, &stamp);
        assert_eq!(sample(&m, 0, 0, 0), Some(2)); // kept
        assert_eq!(sample(&m, 0, 1, 0), Some(9)); // painted
    }

    #[test]
    fn resize_preserves_top_left_content() {
        let mut m = map();
        paint(&mut m, 0, PaintTool::Pencil, (1, 1, 1, 1), 6, 0, 0).unwrap();
        resize(&mut m, 8, 5);
        assert_eq!(m.columns, 8);
        assert_eq!(m.rows, 5);
        assert_eq!(sample(&m, 0, 1, 1), Some(6));
        assert_eq!(sample(&m, 0, 7, 4), Some(0));
        assert!(m.layers.iter().all(|l| l.cells.len() == 40));
    }

    #[test]
    fn tileset_slicing_math() {
        let asset = Asset::bitmap("tiles", "tiles.png", "image/png", 64, 32, vec![]);
        let tileset = Tileset {
            asset: asset.id,
            tile_width: 16,
            tile_height: 16,
            columns: 4,
            ..Default::default()
        };
        assert_eq!(tileset_columns(64, 16), 4);
        assert_eq!(tileset_tile_count(&tileset, 64, 32), 8);
        assert!(validate_tileset(&tileset, 64, 32).is_ok());
        assert!(validate_tileset(&tileset, 8, 32).is_err());
        assert!(validate_tileset(&tileset, 64, 8).is_err());
    }

    #[test]
    fn serialization_roundtrip() {
        let mut project = Project::new_empty("T");
        let mut m = map();
        paint(&mut m, 1, PaintTool::Pencil, (2, 2, 2, 2), 11, 0, 0).unwrap();
        m.layers[1].opacity = 0.5;
        project.tilesets.push(Tileset {
            name: "Terrain".into(),
            asset: Uuid::new_v4(),
            tile_width: 16,
            tile_height: 16,
            columns: 8,
            solid: vec![true, false, true],
            names: vec!["Grass".into()],
            ..Default::default()
        });
        let json = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.tilesets.len(), 1);
        assert_eq!(loaded.tilesets[0].solid, vec![true, false, true]);
        let _ = m; // model covered by project roundtrip tests
    }

    #[test]
    fn copy_paste_and_clear_region() {
        let mut m = map();
        paint(&mut m, 0, PaintTool::Rect, (0, 0, 1, 1), 5, 0, 0).unwrap();
        let region = copy_region(&m, 0, 0, 0, 1, 1).unwrap();
        assert_eq!((region.width, region.height), (2, 2));
        assert!(region.cells.iter().all(|c| *c == 5));
        // Paste at (2,1): overwrites cells including empties.
        assert_eq!(paste_region(&mut m, 0, 2, 1, &region), 4);
        assert_eq!(sample(&m, 0, 2, 1), Some(5));
        assert_eq!(sample(&m, 0, 3, 2), Some(5));
        // Clear the source.
        assert_eq!(clear_region(&mut m, 0, 0, 0, 1, 1), 4);
        assert_eq!(sample(&m, 0, 0, 0), Some(0));
        assert_eq!(sample(&m, 0, 2, 1), Some(5)); // paste survives
    }

    #[test]
    fn copy_region_clips_to_grid() {
        let m = map();
        // Selection partially outside -> clipped, never panics.
        let region = copy_region(&m, 0, -2, -2, 1, 1).unwrap();
        assert_eq!((region.x, region.y), (0, 0));
        assert_eq!((region.width, region.height), (2, 2));
        assert!(copy_region(&m, 0, 10, 10, 12, 12).is_none());
        assert!(copy_region(&m, 9, 0, 0, 1, 1).is_none());
    }

    #[test]
    fn flip_region_mirrors_cells() {
        let mut m = map();
        // Distinct pattern in a 3x2 rect: A B C / D E F (as tile ids 1..6)
        for (i, &(x, y)) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
            .iter()
            .enumerate()
        {
            paint(
                &mut m,
                0,
                PaintTool::Pencil,
                (x, y, x, y),
                i as u16 + 1,
                0,
                0,
            )
            .unwrap();
        }
        flip_region(&mut m, 0, (0, 0, 2, 1), true, false);
        assert_eq!(sample(&m, 0, 0, 0), Some(3));
        assert_eq!(sample(&m, 0, 2, 0), Some(1));
        assert_eq!(sample(&m, 0, 0, 1), Some(6));
        flip_region(&mut m, 0, (0, 0, 2, 1), false, true);
        assert_eq!(sample(&m, 0, 0, 0), Some(6));
        assert_eq!(sample(&m, 0, 0, 1), Some(3));
        // Both flips = 180° rotation of the region.
        flip_region(&mut m, 0, (0, 0, 2, 1), true, true);
        assert_eq!(sample(&m, 0, 0, 0), Some(1));
        assert_eq!(sample(&m, 0, 2, 1), Some(6));
    }

    #[test]
    fn add_and_remove_layer() {
        let mut m = map();
        assert_eq!(m.layers.len(), 2);
        let at = add_layer(&mut m, "Extra", None);
        assert_eq!(at, 2);
        assert_eq!(m.layers[2].name, "Extra");
        assert_eq!(m.layers[2].cells.len(), 12);
        // Removing down to one layer is refused.
        remove_layer(&mut m, 0).unwrap();
        remove_layer(&mut m, 1).unwrap();
        assert!(remove_layer(&mut m, 0).is_err());
        assert_eq!(m.layers.len(), 1);
    }
}
