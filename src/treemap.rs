//! Squarified treemap layout, in terminal cell coordinates.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Cell {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Terminal cells are roughly twice as tall as wide; lay out in square units.
const ASPECT: f64 = 2.0;

/// Lay out `sizes` (sorted descending, all > 0) inside `r`. Returns one cell per
/// item; items too small to occupy a whole cell get `None`.
pub fn layout(sizes: &[u64], r: Cell) -> Vec<Option<Cell>> {
    let mut out = vec![None; sizes.len()];
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if sizes.is_empty() || total <= 0.0 || r.w == 0 || r.h == 0 {
        return out;
    }
    let (x0, y0) = (r.x as f64, r.y as f64 * ASPECT);
    let (w0, h0) = (r.w as f64, r.h as f64 * ASPECT);
    let scale = w0 * h0 / total;

    let (mut x, mut y, mut w, mut h) = (x0, y0, w0, h0);
    let mut i = 0;
    while i < sizes.len() {
        let short = w.min(h);
        let area = |k: usize| sizes[k] as f64 * scale;
        let worst = |sum: f64, max: f64, min: f64| {
            let s2 = short * short;
            (s2 * max / (sum * sum)).max(sum * sum / (s2 * min))
        };
        let mut end = i + 1;
        let mut sum = area(i);
        let mut best = worst(sum, area(i), area(i));
        while end < sizes.len() {
            let a = area(end);
            let ns = sum + a;
            let nw = worst(ns, area(i), a);
            if nw > best {
                break;
            }
            sum = ns;
            best = nw;
            end += 1;
        }
        // Place items i..end as one strip along the short side.
        let horizontal_strip = w < h; // strip spans the full width, stacked on top
        let thick = if horizontal_strip { sum / w } else { sum / h };
        let mut off = 0.0;
        for k in i..end {
            let len = area(k) / thick;
            let (rx, ry, rw, rh) = if horizontal_strip {
                (x + off, y, len, thick)
            } else {
                (x, y + off, thick, len)
            };
            off += len;
            out[k] = to_cells(rx, ry, rw, rh);
        }
        if horizontal_strip {
            y += thick;
            h -= thick;
        } else {
            x += thick;
            w -= thick;
        }
        i = end;
    }
    out
}

fn to_cells(x: f64, y: f64, w: f64, h: f64) -> Option<Cell> {
    let (x1, x2) = (x.round(), (x + w).round());
    let (y1, y2) = ((y / ASPECT).round(), ((y + h) / ASPECT).round());
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    Some(Cell { x: x1 as u16, y: y1 as u16, w: (x2 - x1) as u16, h: (y2 - y1) as u16 })
}
