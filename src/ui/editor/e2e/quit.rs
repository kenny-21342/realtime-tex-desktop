// What quitting does with "Compile when quitting": `cargo test --release quitting_compiles --
// --ignored --nocapture`.
use super::{calm, open, scratch, wait};
use gpui_kit::TestAppContext;

const DOC: &str = "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn quitting_compiles(cx: &mut TestAppContext) {
    let dir = scratch("quit", DOC);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    // an edit not yet on disk: saved, then the chosen number of full runs, each a new layout
    doc.update(cx, |d, cx| {
        let end = d.source.find("Hello.").unwrap();
        d.replace(end..end, "Unsaved. ", cx);
    });
    calm(cx, &doc);
    let before = cx.read(|cx| doc.read(cx).layout_version);
    assert!(doc.update(cx, |d, cx| d.compile_for_quit(3, cx)));
    assert!(cx.read(|cx| doc.read(cx).quit_compiling()));
    wait(cx, &doc, "the three runs", |d| !d.quit_compiling());
    let after = cx.read(|cx| doc.read(cx).layout_version);
    assert!(after >= before + 3, "three full runs: {before} → {after}");
    let disk = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(disk.contains("Unsaved. Hello."), "saved first");

    // a run that fails ends them
    doc.update(cx, |d, cx| {
        let end = d.source.find("Hello.").unwrap();
        d.replace(end..end, "\\notamacro ", cx);
    });
    assert!(doc.update(cx, |d, cx| d.compile_for_quit(5, cx)));
    wait(cx, &doc, "the failed run to end them", |d| {
        !d.quit_compiling()
    });
}

/// "Export the PDF when quitting": the document's PDF is written beside its file, after the
/// compile runs when those are on too, and quitting waits for it. `cargo test --release
/// quitting_exports -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn quitting_exports(cx: &mut TestAppContext) {
    let dir = scratch("quit-export", DOC);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    calm(cx, &doc);
    let pdf = dir.join("main.pdf");
    assert!(!pdf.exists());
    // an edit not yet on disk: saved first, and in the PDF
    doc.update(cx, |d, cx| {
        let at = d.source.find("Hello.").unwrap();
        d.replace(at..at, "Unsaved. ", cx);
    });
    calm(cx, &doc);
    // two compile runs, then the export
    assert!(doc.update(cx, |d, cx| d.compile_for_quit(2, cx)));
    assert!(doc.update(cx, |d, cx| d.export_for_quit(cx)));
    assert!(cx.read(|cx| doc.read(cx).quit_compiling()));
    wait(cx, &doc, "the runs and the export", |d| !d.quit_compiling());
    let bytes = std::fs::read(&pdf).expect("the PDF beside main.tex");
    assert!(bytes.starts_with(b"%PDF"), "not a PDF");
    let disk = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(disk.contains("Unsaved. Hello."), "saved first");
    // the export alone, with no compile runs asked for
    std::fs::remove_file(&pdf).unwrap();
    assert!(doc.update(cx, |d, cx| d.export_for_quit(cx)));
    wait(cx, &doc, "the export alone", |d| !d.quit_compiling());
    assert!(pdf.exists(), "the PDF again");
}

/// A file that is no whole document (an included chapter) has no PDF of its own to export.
#[gpui_kit::test]
#[ignore]
fn quitting_exports_no_fragment(cx: &mut TestAppContext) {
    let dir = scratch("quit-export-part", DOC);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    calm(cx, &doc);
    // what is left has no `\begin{document}`
    doc.update(cx, |d, cx| {
        let at = d.source.find("\\begin{document}").unwrap();
        d.replace(at..at + "\\begin{document}".len(), "", cx);
    });
    assert!(!doc.update(cx, |d, cx| d.export_for_quit(cx)));
    assert!(!cx.read(|cx| doc.read(cx).quit_compiling()));
    assert!(!dir.join("main.pdf").exists());
}
