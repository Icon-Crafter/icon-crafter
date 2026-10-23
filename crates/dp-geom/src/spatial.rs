//! Uniform-grid spatial index used for viewport culling, hit testing and
//! marquee selection queries.
//!
//! Items are keyed by caller-provided ids (typically node ids of 16 bytes). The
//! grid stores the bounding box of every item so that queries can filter out
//! false positives from shared cells. It is intentionally simple: insertion and
//! removal are O(cells covered), queries are O(cells in area + candidate items),
//! and no rebalancing is performed. The editor rebuilds pages of tens of
//! thousands of nodes in a few milliseconds, which is well below the frame
//! budget.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::types::{Point, Rect};

/// A uniform grid over 2D space.
#[derive(Debug, Clone)]
pub struct SpatialGrid<T: Copy + Eq + Hash> {
    cell_size: f64,
    origin: Point,
    cells: HashMap<(i64, i64), Vec<T>>,
    item_cells: HashMap<T, Vec<(i64, i64)>>,
    item_bounds: HashMap<T, Rect>,
}

/// Maximum number of cells a single query may iterate before falling back to a
/// linear scan over item bounds.
const MAX_QUERY_CELLS: i64 = 250_000;

impl<T: Copy + Eq + Hash> Default for SpatialGrid<T> {
    fn default() -> SpatialGrid<T> {
        SpatialGrid::new(256.0)
    }
}

impl<T: Copy + Eq + Hash> SpatialGrid<T> {
    /// Creates a grid with the given cell size in world units.
    pub fn new(cell_size: f64) -> SpatialGrid<T> {
        SpatialGrid::with_origin(cell_size, Point::ZERO)
    }

    /// Creates a grid whose cell borders are aligned to `origin`.
    pub fn with_origin(cell_size: f64, origin: Point) -> SpatialGrid<T> {
        SpatialGrid {
            cell_size: cell_size.clamp(1.0, 1_000_000.0),
            origin,
            cells: HashMap::new(),
            item_cells: HashMap::new(),
            item_bounds: HashMap::new(),
        }
    }

    /// Cell size in world units.
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }

    /// Number of indexed items.
    pub fn len(&self) -> usize {
        self.item_bounds.len()
    }

    /// Whether the grid is empty.
    pub fn is_empty(&self) -> bool {
        self.item_bounds.is_empty()
    }

    /// Removes all items.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.item_cells.clear();
        self.item_bounds.clear();
    }

    /// Bounding box stored for an item.
    pub fn bounds_of(&self, item: T) -> Option<Rect> {
        self.item_bounds.get(&item).copied()
    }

    /// Inserts (or replaces) an item with the given bounds.
    pub fn insert(&mut self, item: T, bounds: Rect) {
        if !bounds.is_finite() {
            return;
        }
        let bounds = bounds.normalized();
        if self.item_bounds.contains_key(&item) {
            self.remove(item);
        }
        let keys = self.cell_keys(bounds);
        for key in &keys {
            self.cells.entry(*key).or_default().push(item);
        }
        self.item_cells.insert(item, keys);
        self.item_bounds.insert(item, bounds);
    }

    /// Updates the bounds of an item (same as insert for existing items).
    pub fn update(&mut self, item: T, bounds: Rect) {
        self.insert(item, bounds);
    }

    /// Removes an item. Returns whether it was present.
    pub fn remove(&mut self, item: T) -> bool {
        let Some(keys) = self.item_cells.remove(&item) else {
            return false;
        };
        self.item_bounds.remove(&item);
        for key in keys {
            if let Some(bucket) = self.cells.get_mut(&key) {
                bucket.retain(|candidate| *candidate != item);
                if bucket.is_empty() {
                    self.cells.remove(&key);
                }
            }
        }
        true
    }

    /// Returns every item whose stored bounds intersect `area`.
    pub fn query(&self, area: Rect) -> Vec<T> {
        if area.is_empty() || self.item_bounds.is_empty() {
            return Vec::new();
        }
        let area = area.normalized();
        let (x0, y0, x1, y1) = self.cell_range(area);
        let nx = x1.saturating_sub(x0).saturating_add(1);
        let ny = y1.saturating_sub(y0).saturating_add(1);
        if nx.saturating_mul(ny) > MAX_QUERY_CELLS {
            return self.query_linear(area);
        }
        let mut found: HashSet<T> = HashSet::new();
        for ix in x0..=x1 {
            for iy in y0..=y1 {
                if let Some(bucket) = self.cells.get(&(ix, iy)) {
                    for item in bucket {
                        if let Some(bounds) = self.item_bounds.get(item)
                            && (bounds.overlaps(area) || bounds.contains_point(area.origin()))
                        {
                            found.insert(*item);
                        }
                    }
                }
            }
        }
        found.into_iter().collect()
    }

    /// Returns every item whose bounds contain `point`.
    pub fn query_point(&self, point: Point) -> Vec<T> {
        if !point.is_finite() {
            return Vec::new();
        }
        let key = (self.cell_x(point.x), self.cell_y(point.y));
        match self.cells.get(&key) {
            Some(bucket) => bucket
                .iter()
                .filter(|item| {
                    self.item_bounds
                        .get(item)
                        .map(|bounds| bounds.contains_point(point))
                        .unwrap_or(false)
                })
                .copied()
                .collect(),
            None => Vec::new(),
        }
    }

    fn query_linear(&self, area: Rect) -> Vec<T> {
        self.item_bounds
            .iter()
            .filter(|(_, bounds)| bounds.overlaps(area))
            .map(|(item, _)| *item)
            .collect()
    }

    fn cell_x(&self, x: f64) -> i64 {
        cell_coord(x, self.origin.x, self.cell_size)
    }

    fn cell_y(&self, y: f64) -> i64 {
        cell_coord(y, self.origin.y, self.cell_size)
    }

    fn cell_range(&self, rect: Rect) -> (i64, i64, i64, i64) {
        let x0 = self.cell_x(rect.left());
        let y0 = self.cell_y(rect.top());
        let x1 = self.cell_x(rect.right());
        let y1 = self.cell_y(rect.bottom());
        (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
    }

    fn cell_keys(&self, rect: Rect) -> Vec<(i64, i64)> {
        let (x0, y0, x1, y1) = self.cell_range(rect);
        let mut keys = Vec::new();
        for ix in x0..=x1 {
            for iy in y0..=y1 {
                keys.push((ix, iy));
            }
        }
        keys
    }
}

fn cell_coord(value: f64, origin: f64, cell_size: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    let raw = ((value - origin) / cell_size).floor();
    if !raw.is_finite() {
        return 0;
    }
    raw.clamp(-4_000_000_000.0, 4_000_000_000.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_query_and_remove() {
        let mut grid: SpatialGrid<u32> = SpatialGrid::new(100.0);
        grid.insert(1, Rect::new(0.0, 0.0, 10.0, 10.0));
        grid.insert(2, Rect::new(500.0, 500.0, 10.0, 10.0));
        grid.insert(3, Rect::new(50.0, 50.0, 100.0, 100.0));
        assert_eq!(grid.len(), 3);
        let visible = grid.query(Rect::new(0.0, 0.0, 120.0, 120.0));
        assert_eq!(visible.len(), 2);
        assert!(visible.contains(&1) && visible.contains(&3));
        let point = grid.query_point(Point::new(505.0, 505.0));
        assert_eq!(point, vec![2]);
        assert!(grid.remove(2));
        assert!(!grid.remove(2));
        assert_eq!(grid.len(), 2);
        assert!(grid.query(Rect::new(500.0, 500.0, 100.0, 100.0)).is_empty());
    }

    #[test]
    fn update_moves_item_between_cells() {
        let mut grid: SpatialGrid<u32> = SpatialGrid::new(50.0);
        grid.insert(1, Rect::new(0.0, 0.0, 10.0, 10.0));
        grid.update(1, Rect::new(400.0, 400.0, 10.0, 10.0));
        assert!(grid.query(Rect::new(0.0, 0.0, 20.0, 20.0)).is_empty());
        assert_eq!(grid.query(Rect::new(400.0, 400.0, 20.0, 20.0)), vec![1]);
        assert_eq!(grid.bounds_of(1), Some(Rect::new(400.0, 400.0, 10.0, 10.0)));
    }

    #[test]
    fn huge_query_falls_back_to_linear_scan() {
        let mut grid: SpatialGrid<u32> = SpatialGrid::new(1.0);
        for i in 0..100u32 {
            grid.insert(i, Rect::new(i as f64 * 10.0, 0.0, 5.0, 5.0));
        }
        let all = grid.query(Rect::new(
            -1_000_000.0,
            -1_000_000.0,
            2_000_000.0,
            2_000_000.0,
        ));
        assert_eq!(all.len(), 100);
    }

    #[test]
    fn empty_queries() {
        let grid: SpatialGrid<u32> = SpatialGrid::new(10.0);
        assert!(grid.is_empty());
        assert!(grid.query(Rect::new(0.0, 0.0, 10.0, 10.0)).is_empty());
        assert!(grid.query_point(Point::new(1.0, 1.0)).is_empty());
    }

    #[test]
    fn touching_items_are_found_by_point_query() {
        let mut grid: SpatialGrid<u32> = SpatialGrid::new(100.0);
        grid.insert(7, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(grid.query_point(Point::new(10.0, 10.0)), vec![7]);
        assert!(grid.query_point(Point::new(10.1, 10.1)).is_empty());
    }
}
