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
        for (k, slot) in out.iter_mut().enumerate().take(end).skip(i) {
            let len = area(k) / thick;
            let (rx, ry, rw, rh) = if horizontal_strip {
                (x + off, y, len, thick)
            } else {
                (x, y + off, thick, len)
            };
            off += len;
            *slot = to_cells(rx, ry, rw, rh);
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
    Some(Cell {
        x: x1 as u16,
        y: y1 as u16,
        w: (x2 - x1) as u16,
        h: (y2 - y1) as u16,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(c: &Cell) -> u32 {
        c.w as u32 * c.h as u32
    }

    #[test]
    fn tiles_the_rect_without_overlap() {
        let r = Cell {
            x: 3,
            y: 2,
            w: 60,
            h: 20,
        };
        let sizes = [500, 300, 200, 100, 50, 25, 10];
        let cells = layout(&sizes, r);
        let placed: Vec<Cell> = cells.iter().flatten().copied().collect();
        assert_eq!(placed.iter().map(area).sum::<u32>(), area(&r));
        for (i, a) in placed.iter().enumerate() {
            assert!(a.x >= r.x && a.x + a.w <= r.x + r.w && a.y >= r.y && a.y + a.h <= r.y + r.h);
            for b in &placed[i + 1..] {
                let apart =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn larger_items_get_larger_cells() {
        let cells = layout(
            &[900, 90, 10],
            Cell {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
        );
        let a: Vec<u32> = cells.iter().flatten().map(area).collect();
        assert!(a[0] > a[1] && a[1] >= a[2]);
    }

    #[test]
    fn empty_or_zero_sized_input_places_nothing() {
        assert!(
            layout(
                &[],
                Cell {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                }
            )
            .is_empty()
        );
        assert!(
            layout(
                &[5],
                Cell {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 10
                }
            )[0]
            .is_none()
        );
    }
}
