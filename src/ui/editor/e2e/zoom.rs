// Pinch zoom keeps the spot under the fingers where it is, measured on the laid-out pages:
// `cargo test --release zoom_ -- --ignored --nocapture`.
use super::{calm, open, scratch};
use gpui_kit::{Point, TestAppContext, VisualTestContext, point, px};

const DOC: &str = "\\documentclass{article}\n\\begin{document}\nPage one.\n\n\\newpage\nPage two.\n\n\\newpage\nPage three.\n\n\\newpage\nPage four.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn zoom_holds_the_spot_under_the_pointer(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("zoom-spot", DOC), "main.tex", 3);
    calm(cx, &doc);
    // page `n`'s box on screen now (window px)
    let bounds = |cx: &mut VisualTestContext, n: u32| {
        cx.read(|cx| {
            let d = doc.read(cx);
            let b = d.page_bounds.take();
            let r = b.iter().find(|(p, _)| *p == n).map(|(_, b)| *b);
            d.page_bounds.set(b);
            r.unwrap_or_else(|| panic!("page {n} not laid out"))
        })
    };
    // the fraction of page `n` under the point `p`
    let frac = |cx: &mut VisualTestContext, n: u32, p: Point<gpui_kit::Pixels>| {
        let b = bounds(cx, n);
        (
            f32::from(p.x - b.origin.x) / f32::from(b.size.width),
            f32::from(p.y - b.origin.y) / f32::from(b.size.height),
        )
    };
    doc.update_in(cx, |d, window, cx| d.set_zoom(1.0, None, window, cx));
    cx.run_until_parked();
    for (n, from, to) in [
        (3u32, 1.0f32, 2.0f32),
        (4, 2.0, 4.0),
        (2, 4.0, 1.0),
        (4, 1.0, 3.0),
    ] {
        // bring the page's upper part to the middle of the view, then pinch on it
        doc.update_in(cx, |d, window, cx| {
            d.set_zoom(from, None, window, cx);
            let top = d.page_y(n);
            let view = d.scroll.bounds();
            d.scroll
                .set_offset(point(d.scroll.offset().x, px(-(top - 200.0)).min(px(0.0))));
            let _ = view;
            cx.notify();
        });
        cx.run_until_parked();
        let b = bounds(cx, n);
        let at = point(
            b.origin.x + b.size.width * 0.3,
            b.origin.y + b.size.height * 0.1,
        );
        let before = frac(cx, n, at);
        doc.update_in(cx, |d, window, cx| d.set_zoom(to, Some(at), window, cx));
        cx.run_until_parked();
        let after = frac(cx, n, at);
        // sideways it holds only when the page then fills the view
        let overflows = cx.read(|cx| doc.read(cx).scroll.max_offset().x > px(0.0));
        let tag = format!("page {n} {from}→{to}: {before:?} → {after:?}");
        assert!((after.1 - before.1).abs() < 0.004, "{tag}");
        if overflows {
            assert!((after.0 - before.0).abs() < 0.004, "{tag}");
        }
    }
}

#[gpui_kit::test]
#[ignore]
fn zoom_far_in_scrolls_to_both_edges(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("zoom-edges", DOC), "main.tex", 3);
    calm(cx, &doc);
    doc.update_in(cx, |d, window, cx| d.set_zoom(4.0, None, window, cx));
    cx.run_until_parked();
    let (page, view, max) = cx.read(|cx| {
        let d = doc.read(cx);
        let b = d.page_bounds.take();
        let page = b.iter().find(|(n, _)| *n == 1).map(|(_, b)| *b).unwrap();
        d.page_bounds.set(b);
        (page, d.scroll.bounds(), d.scroll.max_offset())
    });
    // wider than the view: there is room to scroll, and the left edge is not out of reach
    assert!(page.size.width > view.size.width, "{page:?} {view:?}");
    assert!(max.x > px(0.0), "{max:?}");
    doc.update_in(cx, |d, window, cx| {
        d.scroll.set_offset(point(px(0.0), d.scroll.offset().y));
        window.refresh();
        cx.notify();
    });
    cx.run_until_parked();
    let left = cx.read(|cx| {
        let d = doc.read(cx);
        let b = d.page_bounds.take();
        let r = b
            .iter()
            .find(|(n, _)| *n == 1)
            .map(|(_, b)| b.origin.x)
            .unwrap();
        d.page_bounds.set(b);
        r
    });
    assert!(
        left >= view.origin.x,
        "the page's left edge is out of reach: {left:?}"
    );
}
