// End-to-end tests of the pages with a real rtex: typing, the ribbon, the Insert tab, the
// sidebar, each typesetting a document and checking what the pages and the source do. They are
// #[ignore]d (they need a TeX installation): `cargo test --release <name> -- --ignored
// --nocapture`, with RTD_TYPE=/abs/copy.tex for those that take a document (use a copy: rtex
// writes beside it).
mod disk;
mod formula;
mod image;
mod insert;
mod page_setup;
mod perf;
mod quit;
mod sidebar;
mod source_mode;
mod stress;
mod table;
mod typing;
mod zoom;

use super::Doc;
use crate::engine;
use crate::ui::settings::AppSettings;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, Window};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The document RTD_TYPE names: its folder and file name.
pub(super) fn rtd_type() -> (PathBuf, String) {
    let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
    let path = PathBuf::from(&file);
    (
        path.parent().unwrap().to_path_buf(),
        path.file_name().unwrap().to_string_lossy().to_string(),
    )
}

/// A fresh folder for a test's document `tex` (and pic.png beside it).
pub(super) fn scratch(name: &str, tex: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rtd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), tex).unwrap();
    std::fs::write(dir.join("pic.png"), PNG).unwrap();
    dir
}

/// `main` in `root` opened in a window of its own, with rtex, laid out (`blocks` of them on the
/// pages at least) and settled.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    root: PathBuf,
    main: &str,
    blocks: usize,
) -> (Entity<Doc>, &'a mut VisualTestContext) {
    // rtex wakes the test from its own thread
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(AppSettings {
            enter_no_indent: false,
            ..AppSettings::default()
        });
    });
    engine::configure("", "/Library/TeX/texbin");
    let main = main.to_string();
    let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
        Doc::new(root, main, Default::default(), window, cx)
    });
    wait(cx, &doc, "the first layout", |d| {
        d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > blocks
    });
    calm(cx, &doc);
    (doc, cx)
}

/// Run the app until `done` holds for the doc (rtex answers from its own process).
pub(super) fn wait(
    cx: &mut VisualTestContext,
    doc: &Entity<Doc>,
    what: &str,
    done: impl Fn(&Doc) -> bool,
) {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| done(doc.read(cx))) {
            return;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Run until rtex has typeset everything and stays so for a while: LaTeX may run again
/// (references) just after a layout comes in.
pub(super) fn calm(cx: &mut VisualTestContext, doc: &Entity<Doc>) {
    let settled = |d: &Doc| d.spans_pending == 0 && !d.compiling;
    let mut since = Instant::now();
    loop {
        wait(cx, doc, "the edit typeset", settled);
        std::thread::sleep(Duration::from_millis(30));
        cx.run_until_parked();
        if !cx.read(|cx| settled(doc.read(cx))) {
            since = Instant::now();
        } else if since.elapsed() > Duration::from_millis(300) {
            return;
        }
    }
}

fn src(cx: &mut VisualTestContext, doc: &Entity<Doc>) -> String {
    cx.read(|cx| doc.read(cx).source.clone())
}

/// The edit typesets without a LaTeX error, and one undo takes it back.
pub(super) fn typesets_and_undoes(
    cx: &mut VisualTestContext,
    doc: &Entity<Doc>,
    base: &str,
    what: &str,
) {
    calm(cx, doc);
    let status = cx.read(|cx| doc.read(cx).status.clone());
    assert!(!status.contains("LaTeX"), "{what}: {status}");
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx, doc), base, "{what}: one undo step");
    calm(cx, doc);
}

/// rtex started again (new engine settings, a TeX installed): an unsaved edit is saved first,
/// so the new rtex has what the pages show, and the old rtex's exit does not stop the new one.
/// `cargo test --release a_restart_keeps_the_new_rtex -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn a_restart_keeps_the_new_rtex(cx: &mut TestAppContext) {
    let tex = "\\documentclass{article}\n\\begin{document}\nOne.\n\\end{document}\n";
    let dir = scratch("restart", tex);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    doc.update_in(cx, |d, window, cx| {
        let at = d.source.find("One.").unwrap();
        d.replace(at..at + 4, "Two.", cx);
        d.restart_engine(window, cx);
    });
    calm(cx, &doc);
    // time for the old rtex's last words to arrive
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(20));
        cx.run_until_parked();
    }
    let saved = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(saved.contains("Two."), "saved before the restart: {saved}");
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(d.rtex.is_some(), "the new rtex runs: {}", d.status);
        let stopped = d
            .notices
            .iter()
            .any(|n| matches!(n, super::Notice::EngineStopped(_)));
        assert!(!stopped, "{:?}", d.notices);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// Files set to open at fit width: the first pages to come in fill the column (its width less
/// a page gap each side). `cargo test --release fit_width_waits_for_the_first_pages -- --ignored`.
#[gpui_kit::test]
#[ignore]
fn fit_width_waits_for_the_first_pages(cx: &mut TestAppContext) {
    let tex = "\\documentclass{article}\n\\begin{document}\nOne.\n\\end{document}\n";
    let dir = scratch("fit", tex);
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(AppSettings {
            default_zoom: crate::ui::settings::Zoom::FitWidth,
            ..AppSettings::default()
        });
    });
    engine::configure("", "/Library/TeX/texbin");
    let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
        Doc::new(
            dir.clone(),
            "main.tex".into(),
            Default::default(),
            window,
            cx,
        )
    });
    cx.simulate_resize(gpui_kit::size(gpui_kit::px(1000.0), gpui_kit::px(800.0)));
    wait(cx, &doc, "the first layout", |d| {
        !d.preview.pages.is_empty()
    });
    let (zoom, width) = cx.read(|cx| {
        let d = doc.read(cx);
        let w = d.preview.pages.values().next().unwrap().page_width.unwrap();
        (d.zoom, super::page_px(w, d.zoom))
    });
    assert!(
        (width - (1000.0 - 2.0 * super::PAGE_GAP)).abs() < 2.0,
        "{width} at {zoom}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The pages' paper changed in Settings redraws every page on it: the page images are keyed by
/// paper and ink. `cargo test --release a_page_colour_redraws_the_pages -- --ignored`.
#[gpui_kit::test]
#[ignore]
fn a_page_colour_redraws_the_pages(cx: &mut TestAppContext) {
    use crate::render::{DARK, DarkPaper};
    use crate::ui::themes;
    let tex = "\\documentclass{article}\n\\begin{document}\nOne.\n\\end{document}\n";
    let dir = scratch("paper", tex);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    cx.update(|_, cx| {
        themes::init(cx);
        AppSettings::set(cx, |s| s.theme = "dark".into());
    });
    let on = |paper: DarkPaper| {
        move |d: &Doc| !d.images.is_empty() && d.images.values().all(|im| im.key.1.1 == Some(paper))
    };
    wait(cx, &doc, "the pages on the dark paper", on(DARK));
    cx.update(|_, cx| themes::set_color("pages.paper", Some("#101830".into()), cx));
    let navy = DarkPaper {
        paper: [16, 24, 48],
        ..DARK
    };
    wait(cx, &doc, "the pages on the navy paper", on(navy));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 1×1 PNG.
pub(super) const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0, 3, 1,
    1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Every kind of element the sidebar knows, in the ways people write them.
pub(super) const DOC: &str = r"\documentclass[11pt]{article}
\usepackage{graphicx}
\usepackage{amsmath}
\usepackage{enumitem}
\begin{document}
\section{Intro}\label{sec:intro}
Some text pointing at Figure~\ref{fig:a}.
\subsection*{Unnumbered}
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.5\linewidth]{pic.png}
  \caption{A picture}
  \label{fig:a}
\end{figure}
\begin{figure}[h]
\centering
\includegraphics{pic}
\end{figure}
\begin{table}[t]
  \centering
  \begin{tabular}{|l|r|}
\hline
a & b \\
\hline
  \end{tabular}
  \caption{Numbers}
\end{table}
\begin{equation}
  E = mc^2
  \label{eq:e}
\end{equation}
\begin{equation*}
  a = b
\end{equation*}
\begin{align}
  x &= 1 \\
  y &= 2
\end{align}
\[ z = 3 \]
\begin{itemize}
  \item one
  \item two
\end{itemize}
\begin{enumerate}[label=(\alph*)]
  \item first
\end{enumerate}
\includegraphics[height=2cm]{pic.png}

\begin{minipage}{0.4\linewidth}
Inside a minipage.
\end{minipage}

\begin{center}
Centred text.
\end{center}

Last paragraph.
\end{document}
";
