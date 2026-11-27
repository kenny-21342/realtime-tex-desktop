// Randomised tests of the source editors (table, format, TikZ): documents built from the awkward
// bits of real TeX (comments, escapes, multi-byte text, `\multicolumn`, nested groups, junk), every
// edit tried at every place, and what must hold afterwards checked: no panic, every range lands
// on a character boundary and edits do not overlap, braces stay balanced, and the words on the
// page are the words that were there. The generator is seeded, so a failure replays.
use super::caret::balanced;
use super::format::{self, Align, Font, Size, Style as Para};
use super::table::{self, Borders, Change};
use super::tikz::{self, Shape, Style as Draw};
use std::collections::BTreeMap;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Rng(u64);

/// How many random cases a test tries (`FUZZ_N`), and the seed they start from (`FUZZ_SEED`).
fn runs(default: usize) -> (usize, u64) {
    let get = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u64>().ok());
    (
        get("FUZZ_N").map_or(default, |n| n as usize),
        get("FUZZ_SEED").unwrap_or(0),
    )
}

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n.max(1)
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// What went wrong, by kind: the first input that did it.
#[derive(Default)]
struct Failures(BTreeMap<String, String>);

impl Failures {
    fn note(&mut self, kind: impl Into<String>, input: &str) {
        self.0
            .entry(kind.into())
            .or_insert_with(|| input.to_string());
    }

    /// `f` run with a panic caught and noted.
    fn guard<T>(&mut self, what: &str, input: &str, f: impl FnOnce() -> T) -> Option<T> {
        if std::env::var("FUZZ_TRACE").is_ok() {
            eprintln!("TRACE {what}: {input:?}");
        }
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(v) => Some(v),
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                self.note(format!("panic in {what}: {msg}"), input);
                None
            }
        }
    }

    fn report(self) {
        if self.0.is_empty() {
            return;
        }
        let mut out = String::new();
        for (kind, input) in &self.0 {
            out.push_str(&format!("\n- {kind}\n  input: {input:?}\n"));
        }
        panic!("{} kinds of failure:{out}", self.0.len());
    }
}

fn on_edges(src: &str, r: &Range<usize>) -> bool {
    r.start <= r.end
        && r.end <= src.len()
        && src.is_char_boundary(r.start)
        && src.is_char_boundary(r.end)
}

/// The changes fit the source, on character boundaries, and none overlaps another.
fn changes_fit(f: &mut Failures, what: &str, src: &str, ch: &[Change]) -> bool {
    let mut ok = true;
    for c in ch {
        if !on_edges(src, &c.range) {
            f.note(
                format!("{what}: a change range is off the text or inside a character"),
                src,
            );
            ok = false;
        }
    }
    if !ok {
        return false;
    }
    let mut spans: Vec<(Range<usize>, bool)> = ch
        .iter()
        .map(|c| (c.range.clone(), c.range.is_empty()))
        .collect();
    spans.sort_by_key(|(r, _)| (r.start, r.end));
    for w in spans.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if a.0.end > b.0.start {
            f.note(format!("{what}: two changes overlap"), src);
            ok = false;
        } else if a.0.start == b.0.start && a.1 != b.1 {
            f.note(
                format!("{what}: an insert and a replace start at one place (order decides)"),
                src,
            );
            ok = false;
        }
    }
    ok
}

/// What a reader sees of `s`: its capital letters and non-ASCII text, commands left out.
fn words(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if chars.peek().is_some_and(|n| n.is_ascii_alphabetic()) {
                while chars.peek().is_some_and(|n| n.is_ascii_alphabetic()) {
                    chars.next();
                }
            } else {
                chars.next();
            }
        } else if c.is_ascii_uppercase() || !c.is_ascii() {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- tables

const CELLS: &[&str] = &[
    "A",
    "B",
    "",
    " ",
    "AB",
    "é日",
    "{A & B}",
    "A \\& B",
    "\\multicolumn{2}{c}{M}",
    "$a_{1}$",
    "\\textbf{A}",
    "\\cellcolor{yellow}A",
    "\\textcolor{red}{A}",
    "\\begin{itemize}\\item A \\item B\\end{itemize}",
    "A % note\n",
    "100\\%",
];
const RULES_: &[&str] = &[
    "",
    "",
    "\\hline",
    "\\toprule",
    "\\midrule",
    "\\hline\\hline",
    "\\cline{1-2}",
    "\\cmidrule(lr){1-2}",
];
const ENDS: &[&str] = &["\\\\", "\\\\", "\\\\[3pt]", "\\\\*", "\\\\ "];
const COLS: &[&str] = &[
    "l",
    "c",
    "r",
    "p{3cm}",
    "m{20mm}",
    ">{\\raggedleft\\arraybackslash}p{30pt}",
    "p{1.5in}",
    "@{}l",
    "b{2cm}",
];
const JUNK: &[&str] = &[
    "{",
    "}",
    "&",
    "\\\\",
    "\\",
    "%",
    "\\hline",
    "A",
    "é",
    "\n",
    " ",
    "\\begin{x}",
    "\\end{x}",
    "[",
    "]",
    "$",
    "|",
    ">{",
];

fn gen_table(r: &mut Rng) -> (String, bool) {
    let clean = r.chance(80);
    let cols = 1 + r.below(4);
    let mut spec = String::new();
    if clean {
        let bars = r.chance(30);
        if bars {
            spec.push('|');
        }
        for _ in 0..cols {
            spec.push_str(r.pick(COLS));
            if bars {
                spec.push('|');
            }
        }
    } else {
        for _ in 0..r.below(8) {
            spec.push_str(r.pick(COLS));
            spec.push_str(r.pick(JUNK));
        }
    }
    let indent = *r.pick(&["", "  ", "\t"]);
    let mut body = String::new();
    for k in 0..1 + r.below(4) {
        body.push_str(&format!("\n{indent}{}", r.pick(RULES_)));
        body.push_str(&format!("\n{indent}"));
        for c in 0..cols {
            if c > 0 {
                body.push_str(" & ");
            }
            if clean {
                // a `\multicolumn` spans the row; the other cells are single
                let cell = loop {
                    let c = *r.pick(CELLS);
                    if !c.contains("multicolumn") {
                        break c;
                    }
                    if r.chance(10) && cols >= 2 {
                        break c;
                    }
                };
                body.push_str(cell);
                if cell.contains("multicolumn") {
                    break;
                }
            } else if r.chance(70) {
                body.push_str(r.pick(CELLS));
            } else {
                body.push_str(r.pick(JUNK));
            }
        }
        // the last row may leave out its `\\`
        if !(clean && k > 2 && r.chance(30)) {
            body.push(' ');
            body.push_str(r.pick(ENDS));
        }
    }
    if r.chance(60) {
        body.push_str(&format!("\n{indent}{}", r.pick(RULES_)));
    }
    body.push('\n');
    let src = format!("Intro é\n\\begin{{tabular}}{{{spec}}}{body}\\end{{tabular}}\nOutro 日\n");
    (src, clean)
}

fn uniform(t: &table::Table) -> bool {
    let n = t.cols();
    t.rows.iter().all(|r| r.cells.len() == n)
}

fn cell_texts(src: &str, t: &table::Table) -> Vec<Vec<String>> {
    t.rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| src[c.clone()].trim().to_string())
                .collect()
        })
        .collect()
}

fn table_ranges_fit(f: &mut Failures, src: &str, t: &table::Table) {
    let mut all: Vec<Range<usize>> = vec![t.spec.clone(), t.body.clone()];
    for r in &t.rows {
        all.extend(r.cells.iter().cloned());
        all.extend(r.end.clone());
        all.push(r.start..r.start);
    }
    for r in all {
        if !on_edges(src, &r) {
            f.note(
                "table: a parsed range is off the text or inside a character",
                src,
            );
        }
    }
}

fn after(src: &str, ch: Vec<Change>) -> String {
    table::apply(src, ch)
}

fn fuzz_table_one(f: &mut Failures, src: &str, clean: bool) {
    let Some(t) = f
        .guard("table::find", src, || table::find(src, 0..src.len()))
        .flatten()
    else {
        return;
    };
    table_ranges_fit(f, src, &t);
    let (rows, cols) = (t.rows.len(), t.cols());
    let sound = clean && uniform(&t) && rows > 0;
    let before = cell_texts(src, &t);

    // reading things never panics, at any index
    f.guard("table reads", src, || {
        for i in 0..cols + 2 {
            let _ = (t.align(src, i), t.col_width(src, i));
        }
        for i in 0..rows + 2 {
            let _ = t.row_space(src, i);
        }
        let _ = (t.borders(src), t.header(src));
    });

    for row in 0..rows + 2 {
        for (what, res) in [
            (
                "add_row",
                f.guard("add_row", src, || t.add_row(src, row).map(|c| vec![c]))
                    .flatten(),
            ),
            (
                "del_row",
                f.guard("del_row", src, || t.del_row(src, row).map(|c| vec![c]))
                    .flatten(),
            ),
            (
                "set_cell",
                f.guard("set_cell", src, || {
                    t.set_cell(src, row, 0, "Q").map(|c| vec![c])
                })
                .flatten(),
            ),
            (
                "set_row_space",
                f.guard("set_row_space", src, || {
                    t.set_row_space(src, row, 7.0).map(|c| vec![c])
                })
                .flatten(),
            ),
        ] {
            let Some(ch) = res else { continue };
            if !changes_fit(f, what, src, &ch) {
                continue;
            }
            let out = after(src, ch);
            let Some(t2) = f
                .guard("find after edit", &out, || table::find(&out, 0..out.len()))
                .flatten()
            else {
                if sound && row < rows {
                    f.note(format!("{what}: the table is gone after the edit"), src);
                }
                continue;
            };
            if !(sound && row < rows) {
                continue;
            }
            match what {
                "add_row" if t2.rows.len() != rows + 1 => {
                    f.note("add_row: rows != before + 1", src)
                }
                "del_row" if t2.rows.len() + 1 != rows => {
                    f.note("del_row: rows != before - 1", src)
                }
                "set_cell" => {
                    let got = cell_texts(&out, &t2);
                    if got.get(row).and_then(|r| r.first()).map(String::as_str) != Some("Q") {
                        f.note("set_cell: the cell does not read what was set", src);
                    }
                }
                "set_row_space" if (t2.row_space(&out, row) - 7.0).abs() > 0.11 => {
                    f.note("set_row_space: the room is not what was set", src)
                }
                _ => {}
            }
            // the other cells keep their text
            if what == "add_row" && sound {
                let got = cell_texts(&out, &t2);
                let mut want = before.clone();
                want.insert(row + 1, vec![String::new(); cols]);
                if got != want {
                    f.note("add_row: the other cells changed", src);
                }
            }
            if what == "del_row" && sound {
                let got = cell_texts(&out, &t2);
                let mut want = before.clone();
                want.remove(row);
                if got != want {
                    f.note("del_row: the other cells changed", src);
                }
            }
        }
    }

    for col in 0..cols + 2 {
        let ops: Vec<(&str, Option<Vec<Change>>)> = vec![
            (
                "add_col",
                f.guard("add_col", src, || t.add_col(src, col)).flatten(),
            ),
            (
                "del_col",
                f.guard("del_col", src, || t.del_col(src, col)).flatten(),
            ),
            (
                "set_align",
                f.guard("set_align", src, || {
                    t.set_align(src, col, 'r').map(|c| vec![c])
                })
                .flatten(),
            ),
            (
                "set_col_width",
                f.guard("set_col_width", src, || {
                    t.set_col_width(src, col, 72.0).map(|(c, _)| vec![c])
                })
                .flatten(),
            ),
            (
                "auto_col",
                f.guard("auto_col", src, || t.auto_col(src, col).map(|c| vec![c]))
                    .flatten(),
            ),
        ];
        for (what, res) in ops {
            let Some(ch) = res else { continue };
            if !changes_fit(f, what, src, &ch) {
                continue;
            }
            let out = after(src, ch);
            let Some(t2) = f
                .guard("find after edit", &out, || table::find(&out, 0..out.len()))
                .flatten()
            else {
                if sound && col < cols {
                    f.note(format!("{what}: the table is gone after the edit"), src);
                }
                continue;
            };
            if !(sound && col < cols) {
                continue;
            }
            match what {
                "add_col" if t2.cols() != cols + 1 => f.note("add_col: cols != before + 1", src),
                "del_col" if t2.cols() + 1 != cols => f.note("del_col: cols != before - 1", src),
                "set_align" if t2.align(&out, col) != Some('r') => {
                    f.note("set_align: the column is not right-aligned after", src)
                }
                "set_col_width"
                    if t2
                        .col_width(&out, col)
                        .is_none_or(|w| (w - 72.0).abs() > 0.2) =>
                {
                    f.note("set_col_width: the width is not what was set", src)
                }
                _ => {}
            }
            if matches!(what, "set_align" | "set_col_width" | "auto_col")
                && cell_texts(&out, &t2) != before
            {
                f.note(format!("{what}: a cell changed"), src);
            }
            if what == "set_col_width" {
                // the alignment of the column survives a width
                if let Some(a) = t.align(src, col)
                    && t2.align(&out, col) != Some(a)
                {
                    f.note("set_col_width: the alignment was lost", src);
                }
            }
        }
    }

    // borders and header, on the whole table
    for style in [
        Borders::None,
        Borders::Lines,
        Borders::Grid,
        Borders::Booktabs,
    ] {
        let Some(ch) = f
            .guard("set_borders", src, || t.set_borders(src, style))
            .flatten()
        else {
            continue;
        };
        if !changes_fit(f, "set_borders", src, &ch) {
            continue;
        }
        let out = after(src, ch);
        let Some(t2) = f
            .guard("find after borders", &out, || {
                table::find(&out, 0..out.len())
            })
            .flatten()
        else {
            if sound {
                f.note(
                    format!("set_borders {style:?}: the table is gone after"),
                    src,
                );
            }
            continue;
        };
        if sound {
            if t2.borders(&out) != style {
                f.note(
                    format!(
                        "set_borders {style:?}: reads back as {:?}",
                        t2.borders(&out)
                    ),
                    src,
                );
            }
            if cell_texts(&out, &t2) != before {
                f.note(format!("set_borders {style:?}: a cell changed"), src);
            }
        }
    }
    for on in [true, false] {
        let Some(ch) = f
            .guard("set_header", src, || t.set_header(src, on))
            .flatten()
        else {
            continue;
        };
        if !changes_fit(f, "set_header", src, &ch) {
            continue;
        }
        let out = after(src, ch);
        if let Some(t2) = table::find(&out, 0..out.len())
            && sound
            && !src[t.rows[0].cells[0].clone()].contains("multicolumn")
            && t2.header(&out) != on
            && t.rows[0]
                .cells
                .iter()
                .any(|c| !src[c.clone()].trim().is_empty())
        {
            f.note(format!("set_header {on}: reads back as {}", !on), src);
        }
    }

    // a cell's look: each tool twice returns the cell, and none loses its words
    for cell in CELLS.iter().map(|c| c.trim()) {
        for cmd in ["textbf", "textit"] {
            let once = table::toggle_cell(cell, cmd);
            f.guard("toggle_cell", cell, || table::toggle_cell(&once, cmd));
            if !balanced(cell) {
                continue;
            }
            if words(&once) != words(cell) {
                f.note(format!("toggle_cell {cmd}: the words changed"), cell);
            }
            let twice = table::toggle_cell(&once, cmd);
            if twice.trim() != cell.trim()
                && !cell.starts_with("\\cellcolor")
                && !cell.starts_with("\\textbf")
                && !cell.starts_with("\\textit")
            {
                f.note(
                    format!("toggle_cell {cmd}: twice is not the cell again ({twice:?})"),
                    cell,
                );
            }
        }
        for c in [Some("yellow!30"), None] {
            let a = table::shade_cell(cell, c);
            let b = table::colour_cell(&a, Some("red"));
            let d = table::colour_cell(&b, None);
            f.guard("cell_look", &b, || table::cell_look(&b));
            if balanced(cell) && words(&d) != words(cell) {
                f.note("shade/colour: the words changed", cell);
            }
        }
    }
}

#[test]
fn fuzz_tables() {
    let mut f = Failures::default();
    let (n, seed) = runs(4000);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x9E37_79B9));
    for _ in 0..n {
        let (src, clean) = gen_table(&mut r);
        fuzz_table_one(&mut f, &src, clean);
    }
    f.report();
}

// ---------------------------------------------------------------- formatting

const WORDS: &[&str] = &["AB", "CD", "E", "é日", "ÉCOLE", "X Y"];

/// The source built so far, and the places in it a caret or selection edge can be: the page shows
/// only text, so they are the starts and ends of its words, never inside a command.
struct Gen {
    s: String,
    spots: Vec<usize>,
}

impl Gen {
    fn text(&mut self, w: &str) {
        self.spots.push(self.s.len());
        self.s.push_str(w);
        self.spots.push(self.s.len());
    }
}

fn gen_inline(r: &mut Rng, g: &mut Gen, depth: usize) {
    for _ in 0..1 + r.below(4) {
        let n = if depth >= 3 { 0 } else { r.below(13) };
        let wrap = |g: &mut Gen, r: &mut Rng, open: &str| {
            g.s.push_str(open);
            gen_inline(r, g, depth + 1);
            g.s.push('}');
        };
        match n {
            0..=3 => {
                let w = format!("{} ", r.pick(WORDS));
                g.text(&w);
            }
            4 => wrap(g, r, "\\textbf{"),
            5 => wrap(g, r, "\\emph{"),
            6 => wrap(g, r, "{\\large "),
            7 => wrap(g, r, "\\textcolor{red}{"),
            8 => wrap(g, r, "\\texttt{"),
            9 => g.text("$x_{1}$ "),
            10 => g.text("100\\% "),
            11 => g.s.push_str("% note ZZ\n"),
            _ => wrap(g, r, "\\uline{"),
        }
    }
}

fn gen_block(r: &mut Rng, g: &mut Gen) {
    match r.below(8) {
        0 => {
            g.s.push_str("\\begin{itemize}\n\\item ");
            gen_inline(r, g, 1);
            g.s.push_str("\n\\item ");
            gen_inline(r, g, 1);
            g.s.push_str("\n\\end{itemize}");
        }
        1 => {
            g.s.push_str("\\begin{center}\n");
            gen_inline(r, g, 1);
            g.s.push_str("\n\\end{center}");
        }
        2 => {
            g.s.push_str("\\section{");
            gen_inline(r, g, 2);
            g.s.push('}');
        }
        3 => {
            g.s.push_str("\\centering ");
            gen_inline(r, g, 1);
        }
        4 => {
            g.s.push_str("\\begin{flushright}\n");
            gen_inline(r, g, 1);
            g.s.push_str("\n\\end{flushright}");
        }
        _ => gen_inline(r, g, 0),
    }
}

fn gen_doc(r: &mut Rng) -> (String, Vec<Range<usize>>, Vec<usize>) {
    let mut g = Gen {
        s: String::new(),
        spots: vec![],
    };
    let mut blocks = vec![];
    for k in 0..1 + r.below(4) {
        if k > 0 {
            g.s.push_str("\n\n");
        }
        let start = g.s.len();
        gen_block(r, &mut g);
        let end = start + g.s[start..].trim_end().len();
        blocks.push(start..end);
    }
    (g.s, blocks, g.spots)
}

const OPEN: char = '⟦';
const CLOSE: char = '⟧';

/// One paragraph and the selection in it.
#[derive(Clone)]
struct Case {
    src: String,
    sel: Range<usize>,
}

impl Case {
    /// The paragraph with the selection's edges marked.
    fn marked(&self) -> String {
        format!(
            "{}{OPEN}{}{CLOSE}{}",
            &self.src[..self.sel.start],
            &self.src[self.sel.clone()],
            &self.src[self.sel.end..]
        )
    }

    fn parse(m: &str) -> Option<Case> {
        let a = m.find(OPEN)?;
        let b = m.find(CLOSE)?;
        if a > b {
            return None;
        }
        let src = m.replace([OPEN, CLOSE], "");
        let sel = a..b - OPEN.len_utf8();
        Some(Case { src, sel })
    }
}

type FmtFn = Box<dyn Fn(&str, Range<usize>, Range<usize>) -> Option<format::Fmt>>;

fn fmt_ops() -> Vec<(String, FmtFn)> {
    let mut v: Vec<(String, FmtFn)> = vec![];
    for cmd in ["textbf", "textit", "uline", "sout", "texttt"] {
        v.push((
            format!("toggle {cmd}"),
            Box::new(move |s, b, sel| format::toggle_inline(s, b, sel, cmd)),
        ));
    }
    v.push((
        "size large".into(),
        Box::new(|s, b, sel| format::set_size(s, b, sel, Size::Named("large"))),
    ));
    v.push((
        "size normal".into(),
        Box::new(|s, b, sel| format::set_size(s, b, sel, Size::Named("normalsize"))),
    ));
    v.push((
        "size 13.5pt".into(),
        Box::new(|s, b, sel| format::set_size(s, b, sel, Size::Pt(13.5))),
    ));
    v.push((
        "size up".into(),
        Box::new(|s, b, sel| format::step_size(s, b, sel, 1)),
    ));
    v.push((
        "size down".into(),
        Box::new(|s, b, sel| format::step_size(s, b, sel, -2)),
    ));
    for (n, f) in [
        ("serif", Font::Serif),
        ("sans", Font::Sans),
        ("mono", Font::Mono),
        ("named", Font::Named("lato".into())),
    ] {
        v.push((
            format!("font {n}"),
            Box::new(move |s, b, sel| format::set_font(s, b, sel, &f)),
        ));
    }
    for c in ["red", "blue"] {
        v.push((
            format!("colour {c}"),
            Box::new(move |s, b, sel| format::set_color(s, b, sel, c)),
        ));
    }
    for env in ["itemize", "enumerate"] {
        v.push((
            format!("list {env}"),
            Box::new(move |s, b, sel| format::toggle_list(s, b, sel.start, env)),
        ));
    }
    for (n, a) in [
        ("left", Align::Left),
        ("centre", Align::Center),
        ("right", Align::Right),
        ("justify", Align::Justify),
    ] {
        v.push((
            format!("align {n}"),
            Box::new(move |s, b, _| format::align(s, b, a)),
        ));
    }
    for (n, st) in [
        ("normal", Para::Normal),
        ("section", Para::Heading("section")),
        ("quote", Para::Quote),
        ("title", Para::Title),
    ] {
        v.push((
            format!("style {n}"),
            Box::new(move |s, b, _| format::set_style(s, b, st)),
        ));
    }
    // the caret's edits, as the same kind of change (the selection after is where the caret goes)
    let fmt = |e: super::caret::Edit| format::Fmt {
        range: e.range,
        text: e.text,
        select: e.caret..e.caret,
        packages: vec![],
    };
    v.push((
        "delete range".into(),
        Box::new(move |s, _, sel| Some(fmt(super::caret::delete_range(s, sel.start, sel.end)))),
    ));
    v.push((
        "enter".into(),
        Box::new(move |s, b, sel| {
            super::caret::enter_in_paragraph(s, b, sel.start, false).map(fmt)
        }),
    ));
    v.push((
        "soft enter".into(),
        Box::new(move |s, b, sel| super::caret::enter_in_paragraph(s, b, sel.start, true).map(fmt)),
    ));
    v.push((
        "line break".into(),
        Box::new(move |s, b, sel| super::caret::line_break(s, b, sel.start).map(fmt)),
    ));
    v.push((
        "insert block".into(),
        Box::new(move |s, b, sel| {
            Some(fmt(super::caret::insert_block(
                s,
                b,
                sel.start,
                "\\begin{center}Q\\end{center}",
            )))
        }),
    ));
    v.push((
        "item enter".into(),
        Box::new(move |s, _, sel| {
            super::caret::item_at(s, sel.start)
                .and_then(|it| super::caret::enter_in_item(s, &it, sel.start))
                .map(fmt)
        }),
    ));
    v.push((
        "item indent".into(),
        Box::new(move |s, _, sel| {
            super::caret::item_at(s, sel.start)
                .and_then(|it| super::caret::indent_item(s, &it, sel.start))
                .map(fmt)
        }),
    ));
    v.push((
        "item outdent".into(),
        Box::new(move |s, _, sel| {
            super::caret::item_at(s, sel.start)
                .and_then(|it| super::caret::outdent_item(s, &it, sel.start))
                .map(fmt)
        }),
    ));
    v.push((
        "item backspace".into(),
        Box::new(move |s, _, sel| {
            super::caret::item_at(s, sel.start)
                .and_then(|it| super::caret::backspace_item(s, &it, sel.start))
                .map(fmt)
        }),
    ));
    v
}

/// What the edit gets wrong, if anything: a short, stable name for it.
fn judge(c: &Case, name: &str, op: &FmtFn) -> Option<String> {
    let block = 0..c.src.len();
    let res = match catch_unwind(AssertUnwindSafe(|| {
        op(&c.src, block.clone(), c.sel.clone())
    })) {
        Ok(r) => r,
        Err(_) => return Some(format!("{name}: panics")),
    };
    let fm = res?;
    if !on_edges(&c.src, &fm.range) {
        return Some(format!(
            "{name}: range is off the text or inside a character"
        ));
    }
    let out = format!(
        "{}{}{}",
        &c.src[..fm.range.start],
        fm.text,
        &c.src[fm.range.end..]
    );
    if !on_edges(&out, &fm.select) {
        return Some(format!(
            "{name}: the selection after is off the new text or inside a character"
        ));
    }
    if balanced(&c.src) && !balanced(&out) {
        return Some(format!("{name}: braces or environments no longer balance"));
    }
    if name == "delete range" {
        // the text around the selection stays, in order
        let (head, tail) = (words(&c.src[..c.sel.start]), words(&c.src[c.sel.end..]));
        let w = words(&out);
        if !(w.starts_with(&head) && w.ends_with(&tail) && w.len() >= head.len() + tail.len()) {
            return Some(format!("{name}: the text around the selection changed"));
        }
    } else if name != "insert block" && words(&c.src) != words(&out) {
        return Some(format!("{name}: the words changed"));
    }
    None
}

/// Where the group (`{…}`) opened at `open` ends.
fn group_to(m: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut chars = m[open..].char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Smaller versions of a marked paragraph: a group unwrapped or gone, a word or a comment gone.
fn smaller(m: &str) -> Vec<String> {
    let mut out = vec![];
    let marks = |t: &str| t.contains(OPEN) || t.contains(CLOSE);
    for (i, ch) in m.char_indices() {
        if ch == '{' {
            // `\cmd{…}` or a bare `{…}`
            let head = m[..i].trim_end_matches(|c: char| {
                c != '\\' && c != '}' && c != '{' && c != ' ' && c != '\n'
            });
            let from = if head.ends_with('\\') {
                head.len() - 1
            } else {
                i
            };
            let from = if m[..i].ends_with(['}', ']']) {
                i
            } else {
                from
            };
            if let Some(j) = group_to(m, i) {
                let inner = &m[i + 1..j];
                if !marks(&m[from..=j]) {
                    out.push(format!("{}{}", &m[..from], &m[j + 1..]));
                }
                out.push(format!("{}{}{}", &m[..from], inner, &m[j + 1..]));
            }
        }
    }
    for w in WORDS.iter().map(|w| format!("{w} ")).chain([
        "$x_{1}$ ".into(),
        "100\\% ".into(),
        "% note ZZ\n".into(),
    ]) {
        let mut at = 0;
        while let Some(k) = m[at..].find(&w) {
            let i = at + k;
            out.push(format!("{}{}", &m[..i], &m[i + w.len()..]));
            at = i + w.len();
        }
    }
    out.retain(|t| t.contains(OPEN) && t.contains(CLOSE) && t.len() < m.len());
    out
}

/// The smallest paragraph that still goes wrong the same way.
fn shrink(c: Case, name: &str, op: &FmtFn, kind: &str) -> Case {
    let mut best = c;
    'again: loop {
        for t in smaller(&best.marked()) {
            let Some(cand) = Case::parse(&t) else {
                continue;
            };
            if !balanced(&cand.src) {
                continue;
            }
            if judge(&cand, name, op).as_deref() == Some(kind) {
                best = cand;
                continue 'again;
            }
        }
        return best;
    }
}

#[test]
fn fuzz_formatting() {
    let mut f = Failures::default();
    let (n, seed) = runs(4000);
    let mut r = Rng(0xD1B5_4A32_D192_ED03 ^ seed.wrapping_mul(0x9E37_79B9));
    let ops = fmt_ops();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..n {
        let (src, blocks, spots) = gen_doc(&mut r);
        let block = blocks[r.below(blocks.len())].clone();
        let mine: Vec<usize> = spots
            .iter()
            .copied()
            .filter(|s| block.contains(s) || *s == block.end)
            .map(|s| s - block.start)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let text = src[block.clone()].to_string();
        let (a, b) = (*r.pick(&mine), *r.pick(&mine));
        let sel = match r.below(4) {
            0 => a..a,
            1 => 0..text.len(),
            _ => a.min(b)..a.max(b),
        };
        let case = Case { src: text, sel };
        for (name, op) in &ops {
            let Some(kind) = judge(&case, name, op) else {
                // done again over what it left selected (a toggle turns off, a size moves on): the
                // paragraph must still be sound
                let Some(fm) = op(&case.src, 0..case.src.len(), case.sel.clone()) else {
                    continue;
                };
                let out = format!(
                    "{}{}{}",
                    &case.src[..fm.range.start],
                    fm.text,
                    &case.src[fm.range.end..]
                );
                let next = Case {
                    src: out,
                    sel: fm.select,
                };
                if let Some(kind) = judge(&next, name, op) {
                    let kind = format!("{kind} (the second time)");
                    if seen.insert(kind.clone()) {
                        f.note(kind, &next.marked());
                    }
                }
                continue;
            };
            if !seen.insert(kind.clone()) {
                continue;
            }
            let small = shrink(case.clone(), name, op, &kind);
            f.note(kind, &small.marked());
        }
        f.guard("reads", &case.src, || {
            let _ = format::formats_at(&case.src, 0..case.src.len(), case.sel.start);
            let _ = format::font_and_color_at(&case.src, 0..case.src.len(), case.sel.start);
            let _ = format::size_at(&case.src, 0..case.src.len(), case.sel.start);
            let _ = format::style_of(&case.src, 0..case.src.len());
        });
    }
    f.report();
}

// ---------------------------------------------------------------- TikZ

const STMTS: &[&str] = &[
    "\\draw (0,0) -- (1.5, -2);",
    "\\draw[red, thick] (0,0) rectangle (2,1);",
    "\\fill[blue] (1,1) circle (0.3);",
    "\\filldraw [draw=red] (3,3) circle [radius=1.5cm];",
    "\\node at (2,2) {label};",
    "\\node[above] (a) at (1,1) {f(0,1) é};",
    "\\node at (4,4) {50\\% (7,8)}; % (9,9)",
    "% a comment (3,3) \\node {no}",
    "\\draw (a) -- (b) node[midway] {t};",
    "\\draw[->, dashed] (0,0) -- (1,1) node {\\textbf{x}};",
    "\\draw (1,2,3) -- ({\\x},1) -- (30:2) -- ++(1,0) -- +(0,1);",
    "\\path[fill=green!20] (0,0) -- (0,1) -- (1,0) -- cycle;",
    "\\draw (0,0) -- (1,1) % c\n -- (2,2);",
    "\\foreach \\i in {1,2} { \\draw (\\i,0) circle (0.2); }",
    "\\node {éé} ;",
    "% é日 comment (5,5) \\node {gone}\n  \\draw (6,6) circle (0.5);",
    "\\draw (0,0) -- (1,1); % ünï (7,7)",
];

fn gen_pic(r: &mut Rng) -> String {
    let mut s = String::from("\\begin{tikzpicture}[scale=1]\n");
    for _ in 0..1 + r.below(5) {
        s.push_str("  ");
        s.push_str(r.pick(STMTS));
        s.push('\n');
    }
    if r.chance(10) {
        s.push_str(r.pick(&["(", ")", "{", "}", "\\node [", ";"]));
    }
    s.push_str("\\end{tikzpicture}");
    s
}

const LABELS: &[&str] = &[
    "", "Q", "a b", "é日", "x_{1}", "f(0,1)", "{", "}", "a}b", "\\", "50%", "a\\%b",
];

#[test]
fn fuzz_tikz() {
    let mut f = Failures::default();
    let (n, seed) = runs(4000);
    let mut r = Rng(0xA5A5_1234_DEAD_BEEF ^ seed.wrapping_mul(0x9E37_79B9));
    for _ in 0..n {
        let src = gen_pic(&mut r);
        let pts = f
            .guard("points", &src, || tikz::points(&src))
            .unwrap_or_default();
        for p in &pts {
            if !on_edges(&src, &p.x_at) || !on_edges(&src, &p.y_at) {
                f.note("points: a number's range is off the text", &src);
            }
        }
        let labels = f
            .guard("labels", &src, || tikz::labels(&src))
            .unwrap_or_default();
        let circles = f
            .guard("circles", &src, || tikz::circles(&src))
            .unwrap_or_default();
        f.guard("rects", &src, || tikz::rects(&src));
        let stmts = f
            .guard("statements", &src, || tikz::statements(&src))
            .unwrap_or_default();
        for st in &stmts {
            if !on_edges(&src, &st.range) || st.head > st.range.end {
                f.note("statements: a range is off the text", &src);
            }
        }
        for i in 0..pts.len() + 1 {
            f.guard("stmt_of", &src, || tikz::stmt_of(&src, i));
            if let Some(out) = f
                .guard("moved", &src, || tikz::moved(&src, i, 1.25, -3.5))
                .flatten()
            {
                let p2 = tikz::points(&out);
                if p2.len() != pts.len() {
                    f.note("moved: the number of points changed", &src);
                } else if (p2[i].x - 1.25).abs() > 1e-9 || (p2[i].y + 3.5).abs() > 1e-9 {
                    f.note("moved: the point is not where it was put", &src);
                }
            }
        }
        for i in 0..labels.len() + 1 {
            for text in LABELS {
                if let Some(out) = f
                    .guard("set_label", &src, || tikz::set_label(&src, i, text))
                    .flatten()
                {
                    // a label of any text keeps the picture's braces
                    if balanced(&src) && balanced(text) && !balanced(&out) {
                        f.note("set_label: braces no longer balance", &src);
                    }
                    if !balanced(text) && balanced(&src) && !balanced(&out) {
                        f.note(
                            "set_label: a label with a lone brace breaks the picture",
                            &src,
                        );
                    }
                    if balanced(text) && tikz::labels(&out).len() != labels.len() {
                        f.note(
                            format!("set_label {text:?}: the number of labels changed"),
                            &src,
                        );
                    }
                    if tikz::points(&out).len() != pts.len() && !text.contains('(') {
                        f.note("set_label: the number of points changed", &src);
                    }
                }
            }
        }
        for i in 0..circles.len() + 1 {
            for rad in [0.25, 1.0, 12.5] {
                if let Some(out) = f
                    .guard("set_radius", &src, || tikz::set_radius(&src, i, rad))
                    .flatten()
                {
                    let c2 = tikz::circles(&out);
                    if c2.len() != circles.len() || (c2[i].r - rad).abs() > 1e-9 {
                        f.note("set_radius: the radius is not what was set", &src);
                    }
                }
            }
        }
        for (k, st) in stmts.iter().enumerate() {
            let before = tikz::look(&src, st);
            let edits = [
                Draw::Stroke(Some("red")),
                Draw::Stroke(None),
                Draw::Fill(Some("blue!20")),
                Draw::Fill(None),
                Draw::Width(Some("thick")),
                Draw::Width(None),
                Draw::Dashed,
                Draw::Arrow,
            ];
            for e in &edits {
                let Some(out) = f
                    .guard("styled", &src, || tikz::styled(&src, st, e))
                    .flatten()
                else {
                    continue;
                };
                if balanced(&src) && !balanced(&out) {
                    f.note(format!("styled {e:?}: braces no longer balance"), &src);
                }
                if tikz::points(&out).len() != pts.len() {
                    f.note(format!("styled {e:?}: the number of points changed"), &src);
                }
                let sts = tikz::statements(&out);
                if sts.len() != stmts.len() {
                    f.note(
                        format!("styled {e:?}: the number of statements changed"),
                        &src,
                    );
                    continue;
                }
                let now = tikz::look(&out, &sts[k]);
                let ok = match e {
                    Draw::Stroke(Some(c)) => now.stroke.as_deref() == Some(*c),
                    Draw::Stroke(None) => now.stroke.is_none(),
                    Draw::Fill(Some(c)) => now.fill.as_deref() == Some(*c),
                    Draw::Fill(None) => now.fill.is_none(),
                    Draw::Width(Some(w)) => now.width.as_deref() == Some(*w),
                    Draw::Width(None) => now.width.is_none(),
                    Draw::Dashed => now.dashed != before.dashed,
                    Draw::Arrow => now.arrow != before.arrow,
                };
                if !ok {
                    f.note(
                        format!("styled {e:?}: the look reads back as {now:?} (was {before:?})"),
                        &format!("{src}\n=> {out}"),
                    );
                }
            }
        }
        for shape in [Shape::Line, Shape::Rectangle, Shape::Circle, Shape::Text] {
            if let Some(out) = f
                .guard("added", &src, || tikz::added(&src, shape, 1.0, 2.0))
                .flatten()
                && tikz::statements(&out).len() != stmts.len() + 1
                && !stmts
                    .iter()
                    .any(|s| src[s.range.clone()].contains("\\end{tikzpicture}"))
            {
                f.note(format!("added {shape:?}: not one more statement"), &src);
            }
        }
    }
    f.report();
}

// ---------------------------------------------------------------- the sidebar's fields

const PIECES: &[&str] = &[
    "\\documentclass[11pt]{article}\n",
    "\\documentclass{article}\n",
    "\\usepackage[a4paper,margin=2cm]{geometry}\n",
    "\\usepackage{graphicx,amsmath}\n",
    "\\usepackage{booktabs}\n",
    "\\begin{document}\n",
    "\\section{Intro}\n",
    "\\section*{No number}\n\\label{sec:a}\n",
    "Some text with \\textbf{bold}, 50\\% and é日.\n\n",
    "\\begin{figure}[ht]\n\\centering\n\\includegraphics[width=0.5\\linewidth]{pic.png}\n\\caption{A caption}\n\\label{fig:a}\n\\end{figure}\n\n",
    "\\includegraphics[width=3cm,angle=90]{pic.png}\n\n",
    "\\begin{figure}\n\\includegraphics{pic.png}\n\\end{figure}\n\n",
    "\\begin{table}[h]\n\\caption{T}\n\\begin{tabular}{ll}\nA & B \\\\\n\\end{tabular}\n\\end{table}\n\n",
    "\\begin{equation}\nx^2 % note\n\\end{equation}\n\n",
    "\\begin{align}\na &= b \\\\\nc &= d\n\\end{align}\n\n",
    "\\begin{itemize}\n\\item One\n\\item Two\n\\end{itemize}\n\n",
    "\\begin{enumerate}[label=\\alph*)]\n\\item One\n\\end{enumerate}\n\n",
    "\\end{document}\n",
    "% a comment \\caption{x}\n",
];

const VALUES: &[&str] = &[
    "",
    "0",
    "5",
    "50",
    "100",
    "1.5",
    "-3",
    "true",
    "false",
    "a4paper",
    "letterpaper",
    "2cm",
    "12mm",
    "left",
    "center",
    "x",
    "A new caption",
    "é日",
    "50%",
    "a_b",
    "a & b",
    "}",
    "{",
    "{x}",
    "\\",
    "\\textbf{x}",
    "fig:new",
    "[",
    "]",
    "[x]",
];

#[test]
fn fuzz_element_fields() {
    use super::element;
    let mut f = Failures::default();
    let (n, seed) = runs(1500);
    let mut r = Rng(0xFEED_FACE_CAFE_BEEF ^ seed.wrapping_mul(0x9E37_79B9));
    for _ in 0..n {
        let mut src = String::new();
        for _ in 0..3 + r.below(8) {
            src.push_str(r.pick(PIECES));
        }
        let edges: Vec<usize> = src
            .char_indices()
            .map(|(i, _)| i)
            .chain([src.len()])
            .collect();
        // a block: from one line start to the next blank line (or the end), now and then none
        let at = *r.pick(&edges);
        let start = src[..at].rfind("\n\n").map_or(0, |i| i + 2);
        // a block ends at the first blank line after its start (a position inside a blank line
        // is no block's: it takes the one before)
        let end = src[start..].find("\n\n").map_or(src.len(), |i| start + i);
        let block = r.chance(70).then_some(start..end.max(start));
        let el = f.guard("element_at", &src, || {
            element::element_at(&src, block.clone(), at)
        });
        let Some(el) = el else { continue };
        if !on_edges(&src, &el.range) {
            f.note("element_at: the range is off the text", &src);
            continue;
        }
        let fields = f
            .guard("fields", &src, || element::fields(&src, &el))
            .unwrap_or_default();
        f.guard("image_width_percent", &src, || {
            element::image_width_percent(&src, &el)
        });
        for fld in &fields {
            // what each kind of field can be given: the sidebar offers a choice, a switch or a number
            // box only what it holds, and any text to a text box
            let values: Vec<String> = match &fld.input {
                element::Input::Text => VALUES.iter().map(|v| v.to_string()).collect(),
                element::Input::Number => ["0", "5", "50", "100", "1.5", "12"]
                    .map(String::from)
                    .to_vec(),
                element::Input::Toggle => ["true", "false"].map(String::from).to_vec(),
                element::Input::Choice(opts) => opts.iter().map(|(v, _)| v.to_string()).collect(),
            };
            for v in values.iter().map(String::as_str) {
                let Some(ch) = f
                    .guard("set", &src, || element::set(&src, &el, fld.key, v))
                    .flatten()
                else {
                    continue;
                };
                if !on_edges(&src, &ch.range) {
                    f.note(format!("set {}: the range is off the text", fld.key), &src);
                    continue;
                }
                for p in &ch.packages {
                    if !super::preamble::is_package_name(p) {
                        f.note(format!("set {}: a bad package name {p:?}", fld.key), &src);
                    }
                }
                let out = format!(
                    "{}{}{}",
                    &src[..ch.range.start],
                    ch.text,
                    &src[ch.range.end..]
                );
                if balanced(&src) && balanced(v) && !balanced(&out) {
                    f.note(
                        format!(
                            "set {} to {v:?}: braces or environments no longer balance",
                            fld.key
                        ),
                        &format!("{src}\n=> {out}"),
                    );
                }
                if balanced(&src) && !balanced(v) && !balanced(&out) {
                    f.note(
                        format!("set {} to {v:?}: a lone brace breaks the document", fld.key),
                        &format!("{src}\n=> {out}"),
                    );
                }
                // what was set reads back, for a value that is plain
                let plain =
                    !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
                if plain && !matches!(fld.input, element::Input::Text) {
                    let mut at2 = at.min(out.len());
                    while !out.is_char_boundary(at2) {
                        at2 -= 1;
                    }
                    let Some((el2, again)) = f.guard("read back", &out, || {
                        let el2 = element::element_at(&out, None, at2);
                        let again = element::fields(&out, &el2);
                        (el2, again)
                    }) else {
                        continue;
                    };
                    if let Some(g) = again.iter().find(|g| g.key == fld.key)
                        && g.value.trim() != v
                        && !(g.value.is_empty() && v == "false")
                        && fld.key != "width"
                        && el2.kind == el.kind
                    {
                        f.note(
                            format!(
                                "set {} ({:?}) to {v:?}: it reads back as {:?}",
                                fld.key, fld.input, g.value
                            ),
                            &format!("{src}\n=> {out}"),
                        );
                    }
                }
            }
        }
    }
    f.report();
}

// ---------------------------------------------------------------- parsers fed anything

const HTML: &[&str] = &[
    "AB",
    " CD ",
    "é日",
    "&amp;",
    "&lt;",
    "&gt;",
    "&nbsp;",
    "&#233;",
    "&#x65e5;",
    "&#99999999;",
    "&#;",
    "&bogus;",
    "&",
    "<b>",
    "</b>",
    "<i>",
    "</i>",
    "<u>",
    "</u>",
    "<p>",
    "</p>",
    "<br>",
    "<br/>",
    "<h1>",
    "</h1>",
    "<h3>",
    "</h3>",
    "<ul>",
    "</ul>",
    "<ol>",
    "</ol>",
    "<li>",
    "</li>",
    "<a href=\"http://x.y/a_b%20c#d\">",
    "<a href='{x}'>",
    "</a>",
    "<code>",
    "</code>",
    "<pre>",
    "</pre>",
    "<sup>",
    "</sup>",
    "<sub>",
    "</sub>",
    "<span style=\"font-weight:bold\">",
    "<span style=\"font-style: italic; text-decoration: underline\">",
    "</span>",
    "<table><tr><td>",
    "</td></tr></table>",
    "<!-- c -->",
    "<!--",
    "<script>x</script>",
    "<style>p{}</style>",
    "<img src=\"a.png\">",
    "<",
    ">",
    "\\",
    "{",
    "}",
    "%",
    "$",
    "#",
    "_",
    "^",
    "~",
    "<div>",
    "</div>",
    "<font color=red>",
    "</font>",
    "<blockquote>",
    "</blockquote>",
    "\n",
    "\r\n",
    "\t",
];

fn soup(r: &mut Rng, pool: &[&str], max: usize) -> String {
    (0..r.below(max)).map(|_| *r.pick(pool)).collect()
}

#[test]
fn fuzz_parsers() {
    use super::{find as finder, html, insert, math, outline};
    let mut f = Failures::default();
    let (n, seed) = runs(6000);
    let mut r = Rng(0xBADC_0FFE_E0DD_F00D ^ seed.wrapping_mul(0x9E37_79B9));
    for _ in 0..n {
        // pasted HTML: no panic, TeX that holds together, and the words that were there
        let h = soup(&mut r, HTML, 40);
        if let Some((tex, pk)) = f.guard("html_to_tex", &h, || html::html_to_tex(&h)) {
            // (what stands in a verbatim block is shown as it is: braces, `%` and `#` are its own)
            let mut tex = tex;
            while let Some(a) = tex.find("\\begin{verbatim}") {
                let b = tex[a..]
                    .find("\\end{verbatim}")
                    .map_or(tex.len(), |e| a + e + "\\end{verbatim}".len());
                tex.replace_range(a..b, "");
            }
            if !balanced(&tex) {
                f.note(
                    "html_to_tex: the TeX does not balance",
                    &format!("{h}\n=> {tex}"),
                );
            }
            for p in pk {
                if !super::preamble::is_package_name(p) {
                    f.note(format!("html_to_tex: a bad package name {p:?}"), &h);
                }
            }
            // a `%`, `#`, `&`, `_` or `^` typed in the page never reaches the TeX bare
            let bare = |c: char| {
                let cs: Vec<char> = tex.chars().collect();
                (0..cs.len()).any(|i| cs[i] == c && (i == 0 || cs[i - 1] != '\\'))
            };
            for c in ['%', '#'] {
                if h.contains(c) && !h.contains("<!--") && !h.contains("href") && bare(c) {
                    f.note(
                        format!("html_to_tex: a bare {c:?} in the TeX"),
                        &format!("{h}\n=> {tex}"),
                    );
                }
            }
        }

        // source soups: whatever the readers make of them, nothing panics
        let t = soup(
            &mut r,
            &[
                "\\section{",
                "\\subsection*{",
                "\\paragraph{",
                "}",
                "{",
                "AB ",
                "é日",
                "$",
                "\\[",
                "\\]",
                "$$",
                "\\(",
                "\\)",
                "\\begin{equation}",
                "\\end{equation}",
                "%",
                "\n",
                "\\label{a}",
                "\\label{",
                "@article{k,",
                "title = {T},",
                "}",
                "\\\\",
                "\\",
                "\\textbf{",
                "\\item ",
            ],
            40,
        );
        f.guard("outline", &t, || outline::outline(&t));
        f.guard("labels", &t, || insert::labels(&t));
        f.guard("bib_entries", &t, || insert::bib_entries(&t));
        f.guard("new_label", &t, || insert::new_label(&t, "fig", "a b"));
        f.guard("escape", &t, || super::caret::escape(&t));
        f.guard("parse_opts", &t, || super::element::parse_opts(&t));
        f.guard("visible_chars", &t, || {
            (
                super::glyphmap::visible_chars(&t),
                super::glyphmap::plain_text(&t),
                super::glyphmap::has_visible(&t),
            )
        });
        let edges: Vec<usize> = t.char_indices().map(|(i, _)| i).chain([t.len()]).collect();
        let at = *r.pick(&edges);
        if let Some(Some(range)) = f.guard("formula_at", &t, || math::formula_at(&t, at)) {
            if !on_edges(&t, &range) {
                f.note("formula_at: the range is off the text", &t);
            } else {
                f.guard("math::body", &t, || math::body(&t, range));
            }
        }
        f.guard("split_display_math", &t, || {
            super::blocks::split_display_math(&t, 0..t.len())
        });
        f.guard("is_display_math", &t, || super::blocks::is_display_math(&t));
        // find and replace: the matches are on the text, and replacing one keeps the TeX whole
        let q = *r.pick(&["AB", "ab", "é", "日", "x", " ", "}", "\\", "section"]);
        let whole = 0..t.len();
        let blocks = std::slice::from_ref(&whole);
        if let Some(ms) = f.guard("find", &t, || finder::find(&t, blocks, q, r.chance(50))) {
            for m in ms.iter().take(3) {
                if !on_edges(&t, m) {
                    f.note("find: a match is off the text", &t);
                    continue;
                }
                if let Some((range, text)) =
                    f.guard("replace", &t, || finder::replace(&t, m.clone(), "Z"))
                {
                    if !on_edges(&t, &range) {
                        f.note("replace: the range is off the text", &t);
                        continue;
                    }
                    let out = format!("{}{}{}", &t[..range.start], text, &t[range.end..]);
                    if balanced(&t) && !balanced(&out) {
                        f.note(
                            "replace: the TeX no longer balances",
                            &format!("{t}\n=> {out}"),
                        );
                    }
                }
            }
        }
    }
    f.report();
}
