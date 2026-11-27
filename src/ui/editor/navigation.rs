// Pointer and view: hit-testing the pages, clicks, scrolling to a source offset, zoom.
use super::*;

impl Doc {
    /// Where `pos` falls on `page`, in sp, and the block under it (within a few points).
    pub(super) fn hit(&self, page: u32, pos: Point<Pixels>) -> Option<(f64, f64, Option<usize>)> {
        let bounds = self.page_bounds.take();
        let origin = bounds
            .iter()
            .find(|(n, _)| *n == page)
            .map(|(_, b)| b.origin);
        self.page_bounds.set(bounds);
        let origin = origin?;
        let mut y = f32::from(pos.y - origin.y);
        // below an open source box, the page is shifted down by the box's extra height
        if let (Some(bb), Some((p, cut))) = (self.box_bounds.get(), self.box_cut.get())
            && p == page
            && self.active.is_some()
            && pos.y > bb.origin.y
        {
            y -= (f32::from(bb.size.height) - cut).max(0.0);
        }
        let to_sp = |v: f32| v as f64 / self.zoom as f64 * SP_PER_BP;
        let (x, y) = (to_sp(f32::from(pos.x - origin.x)), to_sp(y));
        let mut best: Option<(f64, usize)> = None;
        for (bi, b) in self.blocks.iter().enumerate() {
            for r in b.rows.iter().filter(|r| r.page == page) {
                let dy = if y < r.bbox[1] {
                    r.bbox[1] - y
                } else if y > r.bbox[3] {
                    y - r.bbox[3]
                } else {
                    0.0
                };
                if best.is_none_or(|(d, _)| dy < d) {
                    best = Some((dy, bi));
                }
            }
        }
        // between the lines of a block: that block; in wider empty space: none
        let bi = best.filter(|(dy, _)| *dy < 4.0 * 65536.0).map(|(_, bi)| bi);
        Some((x, y, bi))
    }

    /// A click on text puts the caret there; on a formula or a picture it selects that block
    /// (Shift: blocks up to it). A double-click on a formula, a macro's text or a picture edits
    /// its source, with the caret at the clicked character. A click beside the lines while a
    /// box is open only closes the box. (New blocks come from the insert line, `gap_at`.)
    pub(super) fn on_page_click(
        &mut self,
        page: u32,
        pos: Point<Pixels>,
        clicks: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // a double-click edits what its first click hit: that click closed any open box, and the
        // page below it moved back up under the pointer
        let first = self
            .first_click
            .take()
            .filter(|f| clicks >= 2 && f.0 == page);
        let Some((x, y, bi)) = first
            .map(|f| (f.1, f.2, f.3))
            .or_else(|| self.hit(page, pos))
        else {
            return;
        };
        if clicks < 2 {
            self.first_click = Some((page, x, y, bi));
        }
        // Shift-click with the caret: text selected from it to the click
        if shift && clicks < 2 && self.active.is_none() && self.caret.is_some() {
            self.extend_to_click(page, x, y, cx);
            self.dragging = true;
            self.focus.focus(window, cx);
            return;
        }
        // Shift-click with no box open selects from the selection's anchor to this block
        if let (true, None, Some(i)) = (shift, &self.active, bi) {
            let anchor = self
                .selected
                .and_then(|(a, _)| self.block_at(a))
                .unwrap_or(i);
            self.set_selection(anchor, i, cx);
            self.focus.focus(window, cx);
            return;
        }
        self.selected = None;
        // with a box open, only a click on another block's text switches to it; the space
        // beside the lines closes the box
        let near = 4.0 * 65536.0;
        let bi = bi.filter(|&i| {
            self.active.is_none()
                || self.blocks[i]
                    .rows
                    .iter()
                    .any(|r| r.page == page && x >= r.bbox[0] - near && x <= r.bbox[2] + near)
        });
        if clicks < 2 {
            // the first click of a double-click lands here too: it places the caret (or
            // selects), the second edits
            self.leave(window, cx);
            self.caret = None;
            let placed = bi.is_some_and(|i| self.caret_click(i, page, x, y, cx));
            // a drag from here selects
            self.dragging = placed;
            if !placed {
                self.selected =
                    bi.map(|i| (self.blocks[i].range.start, self.blocks[i].range.start));
            }
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        let Some(bi) = bi else {
            return;
        };
        // running text: a double-click selects the word, a triple-click the paragraph
        if self.is_plain_text_at(bi, page, x, y) {
            if clicks >= 3 {
                self.select_block_text(bi, cx);
            } else {
                self.select_word(bi, page, x, y, cx);
            }
            return;
        }
        let b = &self.blocks[bi];
        // the character under the click, among the block's glyphs on that page
        let rows: Vec<&render::Row> = b
            .rows
            .iter()
            .filter(|r| r.page == page)
            .map(|r| &r.row)
            .collect();
        let gt = glyphmap::glyph_text_of(&rows);
        let Some(src) = self.source.get(b.range.clone()) else {
            return;
        };
        let caret = glyphmap::char_at(&gt, x, y)
            .and_then(|(i, after)| glyphmap::source_offset(&gt, i, after, src));
        // a formula opens in the formula editor, anything else as source
        let at = self.blocks[bi].range.start + caret.unwrap_or(0);
        self.checkpoint();
        if !self.open_formula(at, at, false, window, cx)
            && !self.open_table(bi, window, cx)
            && !self.open_source_panel(bi, window, cx)
        {
            self.activate(bi, caret.unwrap_or(0), window, cx);
        }
    }

    /// Where in this file a problem is: the start of its line, when it names this file (LaTeX
    /// says `./main.tex`) or none.
    pub fn problem_offset(&self, p: &engine::Diagnostic) -> Option<usize> {
        let line = p.line?.checked_sub(1)?;
        if p.file
            .as_deref()
            .is_some_and(|f| f.trim_start_matches("./") != self.path)
        {
            return None;
        }
        let off = self
            .source
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum();
        (off <= self.source.len()).then_some(off)
    }

    /// The files this one \input's or \include's, when its own body typesets nothing (a main
    /// file that only gathers others): its text is edited there.
    pub fn text_elsewhere(&self) -> Vec<String> {
        if !self.tex
            || self.blocks.is_empty()
            || (0..self.blocks.len()).any(|b| self.is_text_block(b))
        {
            return vec![];
        }
        let mut files = vec![];
        for cmd in ["\\input{", "\\include{"] {
            for (at, _) in self.source.match_indices(cmd) {
                let rest = &self.source[at + cmd.len()..];
                let name = rest[..rest.find('}').unwrap_or(0)].trim();
                let file = if name.ends_with(".tex") {
                    name.to_string()
                } else {
                    format!("{name}.tex")
                };
                if !name.is_empty() && self.root.join(&file).is_file() && !files.contains(&file) {
                    files.push(file);
                }
            }
        }
        files
    }

    /// A source box is open on the page.
    pub fn box_open(&self) -> bool {
        self.active.is_some()
    }

    /// Where the open source box's text starts in the source.
    pub fn box_at(&self) -> Option<usize> {
        self.active.as_ref().map(|a| a.range.start)
    }

    pub fn jump_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.scroll_to(offset, false, cx);
    }

    /// Where the reader is in the source: the start of the last block that has begun by the top of
    /// the view (a block an outline jump put there counts, as it sits `MARGIN` down); above the
    /// first block, that one.
    pub fn reading_offset(&self) -> Option<usize> {
        let line = -f32::from(self.scroll.offset().y) + MARGIN + 1.0;
        let top = |b: &document::Block| {
            b.rows
                .first()
                .map(|r| self.page_y(r.page) + page_px(r.bbox[1], self.zoom))
        };
        let mut begun = self
            .blocks
            .iter()
            .filter(|b| top(b).is_some_and(|t| t <= line));
        begun
            .next_back()
            .or_else(|| self.blocks.iter().find(|b| top(b).is_some()))
            .map(|b| b.range.start)
    }

    /// Bring the block at `offset` into view. Without `if_hidden` (a jump from the outline) its
    /// top goes near the view's top. With it, the view moves only if what is at hand is out of
    /// sight (the caret's line when the caret is at `offset`, else any of the block), and then
    /// only as far as it must: a click never moves the page under the pointer.
    pub(super) fn scroll_to(&mut self, offset: usize, if_hidden: bool, cx: &mut Context<Self>) {
        let Some(b) = self
            .blocks
            .iter()
            .find(|b| b.range.contains(&offset) || b.range.start >= offset)
        else {
            return;
        };
        let (Some(first), Some(last)) = (b.rows.first(), b.rows.last()) else {
            return;
        };
        let block = (
            self.page_y(first.page) + page_px(first.bbox[1], self.zoom),
            self.page_y(last.page) + page_px(last.bbox[3], self.zoom),
        );
        let view = (
            -f32::from(self.scroll.offset().y),
            f32::from(self.scroll.bounds().size.height),
        );
        let top = if !if_hidden {
            block.0 - MARGIN
        } else {
            // the caret's line, from its baseline and font size; else the block
            let caret = self
                .caret
                .as_ref()
                .filter(|c| c.off == offset)
                .and_then(|_| self.caret_at())
                .map(|(page, _, base, size)| {
                    let y = self.page_y(page);
                    (
                        y + page_px(base - size, self.zoom),
                        y + page_px(base + size * 0.3, self.zoom),
                    )
                });
            match into_view(caret.unwrap_or(block), view, caret.is_none()) {
                Some(top) => top,
                None => return,
            }
        };
        let x = self.scroll.offset().x;
        self.scroll.set_offset(point(x, px(-top.max(0.0))));
        cx.notify();
    }

    /// Where page `page`'s top is in the column, in px.
    pub(super) fn page_y(&self, page: u32) -> f32 {
        let mut y = PAGES_TOP;
        for (&n, dl) in &self.preview.pages {
            if n == page {
                break;
            }
            y += page_px(dl.page_height.unwrap_or(0.0), self.zoom) + PAGE_GAP;
        }
        y
    }

    /// The pages' sizes at zoom 1 (px): width and height, in order.
    fn page_sizes(&self) -> Vec<(f32, f32)> {
        self.preview
            .pages
            .values()
            .map(|dl| {
                (
                    page_px(dl.page_width.unwrap_or(40258437.0), 1.0),
                    page_px(dl.page_height.unwrap_or(0.0), 1.0),
                )
            })
            .collect()
    }

    /// The least the page column is wide at the zoom now: its widest page and the space beside
    /// it, so a page wider than the view scrolls sideways to both of its edges.
    pub(super) fn column_min_width(&self) -> f32 {
        column_width(&self.page_sizes(), self.zoom)
    }

    /// Zoom to `z`, keeping the point `anchor` (window coordinates; default the view's centre)
    /// where it is on screen.
    pub fn set_zoom(
        &mut self,
        z: f32,
        anchor: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let z = z.clamp(0.25, 4.0);
        if (z - self.zoom).abs() < 1e-4 {
            return;
        }
        let view = self.scroll.bounds();
        let p = anchor.unwrap_or(view.center()) - view.origin;
        let off = self.scroll.offset();
        let (x, y) = zoom_offset(
            &self.page_sizes(),
            (f32::from(view.size.width), f32::from(view.size.height)),
            (self.zoom, z),
            (f32::from(p.x), f32::from(p.y)),
            (f32::from(off.x), f32::from(off.y)),
        );
        self.zoom = z;
        self.zoomed_at = Instant::now();
        self.scroll.set_offset(point(px(x), px(y)));
        self.render_live(window, cx);
        cx.notify();
    }

    /// One step along `ZOOMS`.
    /// The zoom at which the widest page fills the column, with the gap beside it.
    pub fn fit_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let widest = self
            .preview
            .pages
            .values()
            .filter_map(|dl| dl.page_width)
            .fold(0.0, f64::max);
        let column = f32::from(self.scroll.bounds().size.width) - 2.0 * PAGE_GAP;
        if widest > 0.0 && column > 0.0 {
            self.set_zoom(column / page_px(widest, 1.0), None, window, cx);
        }
    }

    pub fn zoom_step(&mut self, dir: i32, window: &mut Window, cx: &mut Context<Self>) {
        let z = self.zoom;
        let next = if dir > 0 {
            ZOOMS.iter().find(|&&v| v > z + 0.001)
        } else {
            ZOOMS.iter().rev().find(|&&v| v < z - 0.001)
        };
        if let Some(&v) = next {
            self.set_zoom(v, None, window, cx);
        }
    }
}

/// Room kept between what scrolls into view and the view's edge, in px.
const MARGIN: f32 = 40.0;

/// The space beside the pages in their column (its padding), in px.
pub(super) const COLUMN_PAD: f32 = 64.0;

/// How wide the page column is at zoom `z` for `pages` (width, height at zoom 1): the view's
/// width when every page fits in it, else the widest page and its space.
fn column_width(pages: &[(f32, f32)], z: f32) -> f32 {
    pages.iter().map(|p| p.0 * z).fold(0.0, f32::max) + COLUMN_PAD
}

/// The scroll offset after zooming from `zoom.0` to `zoom.1` that keeps the point `p` (px from
/// the view's corner) on the same spot of the page it is over. `pages` are the pages' sizes at
/// zoom 1, `view` the view's size, `off` the offset now (zero or less). The pages scale, but the
/// gaps between them do not, and a page is centred in a column no narrower than the view: the
/// point is placed by the page it is over, not by scaling the whole column.
pub(super) fn zoom_offset(
    pages: &[(f32, f32)],
    view: (f32, f32),
    zoom: (f32, f32),
    p: (f32, f32),
    off: (f32, f32),
) -> (f32, f32) {
    let (old, new) = zoom;
    let ratio = new / old;
    let (ax, ay) = (p.0 - off.0, p.1 - off.1);
    let top =
        |n: usize, z: f32| PAGES_TOP + pages[..n].iter().map(|p| p.1 * z + PAGE_GAP).sum::<f32>();
    let wide = |z: f32| view.0.max(column_width(pages, z));
    // the page the point is over: the last one that starts above it
    let n = (0..pages.len())
        .rev()
        .find(|&n| top(n, old) <= ay)
        .unwrap_or(0);
    let (nx, ny) = match pages.get(n) {
        Some(&(w, h)) => {
            // along the page, scaled; past its ends (a gap, the desk) the distance stays
            let along = |v: f32, start: f32, len: f32| {
                let inside = (v - start).clamp(0.0, len);
                (inside, v - start - inside)
            };
            let (ix, ex) = along(ax, (wide(old) - w * old) / 2.0, w * old);
            let (iy, ey) = along(ay, top(n, old), h * old);
            (
                (wide(new) - w * new) / 2.0 + ix * ratio + ex,
                top(n, new) + iy * ratio + ey,
            )
        }
        None => (ax * ratio, (ay - PAGES_TOP) * ratio + PAGES_TOP),
    };
    let most = (wide(new) - view.0).max(0.0);
    ((p.0 - nx).clamp(-most, 0.0), (p.1 - ny).min(0.0))
}

/// The view's new top to bring `span` (top, bottom, px) into `view` (top, height), moving as
/// little as it can; None when it is in view already (`any`: when any of it shows, else all).
fn into_view(span: (f32, f32), view: (f32, f32), any: bool) -> Option<f32> {
    let (top, bottom) = span;
    let (v, h) = view;
    let shows = if any {
        bottom > v && top < v + h
    } else {
        top >= v && bottom <= v + h
    };
    if shows {
        None
    } else if top < v || bottom - top > h - 2.0 * MARGIN {
        // above, or too tall to fit: its top near the view's top
        Some(top - MARGIN)
    } else {
        // below: its bottom near the view's bottom
        Some(bottom - h + MARGIN)
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::{MARGIN, into_view};

    #[test]
    fn the_view_moves_only_for_what_is_out_of_sight() {
        let view = (1000.0, 600.0);
        // a caret line in view: nothing moves
        assert_eq!(into_view((1200.0, 1220.0), view, false), None);
        // a tall block whose top is above the view, clicked inside: nothing moves
        assert_eq!(into_view((500.0, 1300.0), view, true), None);
        // a line just below the view: only so far that it shows
        assert_eq!(
            into_view((1610.0, 1630.0), view, false),
            Some(1630.0 - 600.0 + MARGIN)
        );
        // a line above: its top near the view's top
        assert_eq!(into_view((900.0, 920.0), view, false), Some(900.0 - MARGIN));
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::{PAGE_GAP, PAGES_TOP, zoom_offset};

    /// Four letter pages (612 × 792 at zoom 1) in a 1920 × 1080 view.
    const PAGES: [(f32, f32); 4] = [(612.0, 792.0); 4];
    const VIEW: (f32, f32) = (1920.0, 1080.0);

    /// Where the page-relative point (`fx`, `fy` of page `n`'s size) is on screen, the page
    /// column laid out as the editor does at `zoom` with scroll offset `off`.
    fn on_screen(n: usize, fx: f32, fy: f32, zoom: f32, off: (f32, f32)) -> (f32, f32) {
        let wide = VIEW.0.max(612.0 * zoom + 64.0);
        let top = PAGES_TOP + n as f32 * (792.0 * zoom + PAGE_GAP);
        (
            off.0 + (wide - 612.0 * zoom) / 2.0 + fx * 612.0 * zoom,
            off.1 + top + fy * 792.0 * zoom,
        )
    }

    /// The point at (`fx`, `fy`) of page `n` at zoom `a`, pinched to zoom `b`: where it is on
    /// screen before and after. Sideways it can only hold when the page then overflows the view
    /// (else it is centred whatever the offset).
    fn pinched(n: usize, (fx, fy): (f32, f32), a: f32, b: f32) -> ((f32, f32), (f32, f32)) {
        let top = PAGES_TOP + n as f32 * (792.0 * a + PAGE_GAP);
        let off = (0.0, (100.0 - top).min(0.0));
        let at = on_screen(n, fx, fy, a, off);
        let now = zoom_offset(&PAGES, VIEW, (a, b), at, off);
        (at, on_screen(n, fx, fy, b, now))
    }

    #[test]
    fn the_point_under_the_fingers_stays_on_its_page_spot() {
        // the fourth page's upper part, from 200% to 400%
        let ((x, y), (x2, y2)) = pinched(3, (0.3, 0.2), 2.0, 4.0);
        assert!(y > 0.0 && y < VIEW.1, "{y}");
        assert!(
            (x2 - x).abs() < 0.01 && (y2 - y).abs() < 0.01,
            "{x2},{y2} for {x},{y}"
        );
    }

    #[test]
    fn it_holds_zooming_out_and_in_at_every_page() {
        for n in 0..4 {
            for (a, b) in [(1.0, 0.5), (0.5, 1.5), (1.0, 3.0), (3.0, 1.0), (2.0, 4.0)] {
                let ((x, y), (x2, y2)) = pinched(n, (0.5, 0.1), a, b);
                // the page's top may be at the view's top already: the offset cannot go past it
                let held_y = (y2 - y).abs() < 0.01;
                let at_top = n == 0 || b < a;
                assert!(held_y || at_top, "page {n} {a}→{b}: y {y2} for {y}");
                if 612.0 * b + 64.0 > VIEW.0 {
                    assert!((x2 - x).abs() < 0.01, "page {n} {a}→{b}: x {x2} for {x}");
                }
            }
        }
    }

    #[test]
    fn gaps_between_pages_do_not_scale() {
        // 200% → 400% over the fourth page: scaling the whole column would be off by the three
        // gaps it passes (72 px) times the change of zoom
        let ((_, y), (_, y2)) = pinched(3, (0.5, 0.5), 2.0, 4.0);
        assert!((y2 - y).abs() < 0.01, "{y2} for {y}");
    }

    #[test]
    fn a_page_wider_than_the_view_scrolls_to_both_its_edges() {
        // at 400% the page is 2448 wide in a 1920 view: the offset may go 592 px to the left
        let (x, _) = zoom_offset(&PAGES, VIEW, (1.0, 4.0), (1900.0, 500.0), (0.0, 0.0));
        assert!((-592.0..=0.0).contains(&x), "{x}");
        assert_eq!(x, -592.0);
        // and the view's left edge pinned at the page's left edge keeps the page there
        let (x, _) = zoom_offset(&PAGES, VIEW, (1.0, 4.0), (0.0, 500.0), (0.0, 0.0));
        assert_eq!(x, 0.0);
    }

    #[test]
    fn no_pages_scale_about_the_point() {
        let (x, y) = zoom_offset(&[], VIEW, (1.0, 2.0), (100.0, PAGES_TOP + 50.0), (0.0, 0.0));
        // (no pages: no room sideways)
        assert_eq!((x, y), (0.0, -50.0));
    }
}
