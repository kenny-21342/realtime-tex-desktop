// One open file: for a .tex file, its typeset pages from `rtex serve`, where the block under the
// cursor (a paragraph, a heading, a display formula) turns into its TeX source in place and every
// keystroke goes to rtex, which re-typesets it in milliseconds. Other text files open as source.
mod complete;
mod disk;
#[cfg(test)]
mod e2e;
mod editing;
mod element_edit;
mod file_view;
mod find_bar;
mod formatting;
mod formula;
mod image_resize;
mod inserting;
mod menu;
mod navigation;
mod pages;
mod selection;
mod session;
mod source_box;
mod table_handles;
mod table_panel;
mod text_selection;
mod tikz_points;
mod typing;

use crate::document::{self, glyphmap};
use crate::engine;
use crate::render::{self, viewer};
use crate::ui::palette;
use crate::ui::settings::{AppSettings, TexView, Zoom};
pub use formatting::{FormatOp, RibbonState};
use futures::StreamExt;
use gpui_kit::component::input::{
    Copy, Cut, Editor, EditorState, InputEvent, Paste, SelectAll, highlight_characters,
};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, Size, spinner::Spinner};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use source_box::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use typing::*;

const SP_PER_BP: f64 = 65781.76;
pub const ZOOMS: [f32; 9] = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
const PAGE_GAP: f32 = 24.0;
/// Above the first page: padding, then the collapsed preamble strip and its gap.
const PAGES_TOP: f32 = 32.0 + FRAME_H + 8.0;
const FRAME_H: f32 = 26.0;
pub const MONO: &str = "Menlo";

/// Something not to miss (the status bar shows it too, where it is easily missed): the
/// workspace shows each as a notification in the window's corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Saving failed (why): the edits are only in the window; later, a save that worked again.
    SaveFailed(String),
    Saved,
    /// rtex could not start (why: not found, not runnable), or ended (its last words); later,
    /// running again.
    EngineMissing(String),
    EngineStopped(String),
    EngineRunning,
    /// The full run failed on the last change; later, one that worked again.
    TypesetFailed,
    Typeset,
    /// A PDF export ended: where (None: no PDF made) and whether LaTeX ran clean.
    Exported {
        path: Option<String>,
        ok: bool,
    },
    /// Anything else that failed: what, and why.
    Failed {
        what: String,
        why: String,
    },
}

struct PageImage {
    img: Arc<RenderImage>,
    key: (u64, (u32, Option<render::DarkPaper>)),
}

actions!(
    doc,
    [
        EditBlock,
        DeleteBlock,
        DuplicateBlock,
        MoveBlockUp,
        MoveBlockDown,
        SelectPrevBlock,
        SelectNextBlock,
        ExtendSelectionUp,
        ExtendSelectionDown,
        ClearSelection,
        CaretLeft,
        CaretRight,
        CaretUp,
        CaretDown,
        CaretWordLeft,
        CaretWordRight,
        CaretLineStart,
        CaretLineEnd,
        CaretDocStart,
        CaretDocEnd,
        Backspace,
        DeleteForward,
        NewParagraph,
        LineBreak,
        Indent,
        Outdent,
        SelectCaretBlock,
        InsertMath,
        MoveParagraphUp,
        MoveParagraphDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocStart,
        SelectDocEnd,
        FormatBold,
        FormatItalic,
        FormatUnderline,
        FontBigger,
        FontSmaller,
        Heading1,
        Heading2,
        Heading3,
        AlignLeft,
        AlignCenter,
        AlignRight,
        AlignJustify,
        PastePlain,
        OpenFind,
        OpenReplace,
        FindNext,
        FindPrev,
        CloseFind
    ]
);

/// The fixed editing keys on the pages, while no source box is open: for the caret ("Caret"), or
/// for the selected blocks ("Pages"); commands' keys are in ui::keymap, which you can change. Inside a box the editor's own bindings win, so ⌘A, ⇧↑ and the rest act on
/// its text.
pub fn bind_keys(cx: &mut App) {
    crate::ui::terminal::bind_keys(cx);
    let caret = Some("Caret");
    cx.bind_keys([
        KeyBinding::new(crate::ui::keymap::fixed_key("left"), CaretLeft, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("right"), CaretRight, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("up"), CaretUp, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("down"), CaretDown, caret),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("alt-left"),
            CaretWordLeft,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("alt-right"),
            CaretWordRight,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-left"),
            CaretLineStart,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-right"),
            CaretLineEnd,
            caret,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("home"), CaretLineStart, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("end"), CaretLineEnd, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-up"), CaretDocStart, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-down"), CaretDocEnd, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("backspace"), Backspace, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("delete"), DeleteForward, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("enter"), NewParagraph, caret),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-enter"),
            LineBreak,
            caret,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("tab"), Indent, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("shift-tab"), Outdent, caret),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("escape"),
            SelectCaretBlock,
            caret,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-v"), Paste, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-c"), Copy, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-x"), Cut, caret),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-a"), SelectAll, caret),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-left"),
            SelectLeft,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-right"),
            SelectRight,
            caret,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("shift-up"), SelectUp, caret),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-down"),
            SelectDown,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("alt-shift-left"),
            SelectWordLeft,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("alt-shift-right"),
            SelectWordRight,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-shift-left"),
            SelectLineStart,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-shift-right"),
            SelectLineEnd,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-home"),
            SelectLineStart,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-end"),
            SelectLineEnd,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-shift-up"),
            SelectDocStart,
            caret,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-shift-down"),
            SelectDocEnd,
            caret,
        ),
    ]);
    cx.bind_keys([
        KeyBinding::new(
            crate::ui::keymap::fixed_key("escape"),
            CloseFind,
            Some("FindBar"),
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-g"),
            FindNext,
            Some("FindBar"),
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("cmd-shift-g"),
            FindPrev,
            Some("FindBar"),
        ),
    ]);
    let pages = Some("Pages");
    cx.bind_keys([
        KeyBinding::new(crate::ui::keymap::fixed_key("enter"), EditBlock, pages),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("backspace"),
            DeleteBlock,
            pages,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("delete"), DeleteBlock, pages),
        KeyBinding::new(crate::ui::keymap::fixed_key("up"), SelectPrevBlock, pages),
        KeyBinding::new(crate::ui::keymap::fixed_key("down"), SelectNextBlock, pages),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-up"),
            ExtendSelectionUp,
            pages,
        ),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("shift-down"),
            ExtendSelectionDown,
            pages,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-a"), SelectAll, pages),
        KeyBinding::new(
            crate::ui::keymap::fixed_key("escape"),
            ClearSelection,
            pages,
        ),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-c"), Copy, pages),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-x"), Cut, pages),
        KeyBinding::new(crate::ui::keymap::fixed_key("cmd-v"), Paste, pages),
    ]);
}

/// A page, a box on it in sp, and the font of the text there.
type BoxSpot = (u32, [f64; 4], Option<engine::FontDesc>);
/// A page and where it was last drawn (window coordinates).
type PageBounds = (u32, Bounds<Pixels>);
/// A right-click: page, y (sp) and the start of the block there.
type MenuTarget = (u32, f64, Option<usize>);

/// What a panel under the page edits.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    /// A formula's TeX, with symbols to click (`range` is inside its delimiters).
    Formula,
    /// A table's cells, in their own inputs.
    Table,
    /// A block's TeX (a TikZ picture) in an editor, the picture above it.
    Source,
}

/// The block being edited as source.
struct Active {
    span: u64,
    /// Its bytes in `source`, kept up to date as the user types.
    range: Range<usize>,
    /// The preamble or the trailer, which are edited in a strip off the page.
    frame: bool,
    /// New text typed into empty space on a page, until it typesets as a block of its own.
    gap: Option<Gap>,
    /// Edited in a panel under the page, which stays typeset, rather than a box in place of
    /// its lines.
    panel: Option<Panel>,
    /// Where the box was last drawn (page, box in sp): it stays there while the text has no
    /// typeset lines (a lone `\`, an error). A box that is not drawn loses focus and closes.
    last: Cell<Option<BoxSpot>>,
}

impl Active {
    fn new(span: u64, range: Range<usize>, frame: bool, gap: Option<Gap>) -> Self {
        Self {
            span,
            range,
            frame,
            gap,
            panel: None,
            last: Cell::new(None),
        }
    }

    /// Edited in a panel under the page rather than a box in place of its lines.
    fn in_panel(&self) -> bool {
        self.panel.is_some()
    }
}

/// Where the source box is drawn: a page, its box in sp, and whether that is a block's lines
/// (else a spot without lines yet).
struct BoxPlace {
    page: u32,
    rect: [f64; 4],
    /// The font the text is typeset in: the source is shown in it, at its size.
    font: Option<engine::FontDesc>,
    /// How many typeset lines the box replaces on its page.
    count: usize,
    lines: bool,
    /// It holds several blocks' source (a multi-block selection being edited).
    multi: bool,
}

/// How far below the block it follows new text's box opens, in sp.
const GAP_PAD: f64 = 3.0 * 65536.0;

/// Where the right-click menu puts new text.
#[derive(Clone, Copy)]
enum Spot {
    Above(usize),
    Below(usize),
    /// Empty space at y (sp) on a page.
    Gap(u32, f64),
}

struct Gap {
    page: u32,
    /// Where the box opens, in sp from the page top.
    y: f64,
    /// The blank line that makes it a paragraph goes before it (after the previous block) or,
    /// above the first block, after it.
    sep_before: bool,
}

/// Where the PDF exported as the app quits is.
#[derive(Clone, Copy, PartialEq)]
enum QuitExport {
    No,
    /// To be asked of rtex once the quit's compile runs are done.
    Wanted,
    /// Asked: the answer is awaited.
    Asked,
}

pub struct Doc {
    /// A PDF or image shown in the viewer instead of text.
    pub view: Option<viewer::View>,
    /// The viewer's rendered pages: page to (zoom key, image).
    view_images: HashMap<usize, (u64, Arc<RenderImage>)>,
    root: PathBuf,
    /// Relative to `root`.
    pub path: String,
    /// Typeset with rtex (a .tex file); otherwise edited as plain source.
    pub tex: bool,
    pub source: String,
    pub dirty: bool,
    /// The file as last read or written here, to tell a change someone else made.
    disk: disk::Disk,
    /// The file's text after a change made on disk while there were unsaved edits here: the
    /// user has not chosen yet which version stays.
    pub conflict: Option<String>,
    _watch: Task<()>,
    pub status: String,
    /// A full compile is under way for an edit the live path could not take.
    pub compiling: bool,
    /// Full compile runs still to make before the app quits (see `compile_for_quit`).
    quit_runs: u32,
    /// The PDF exported as the app quits (see `export_for_quit`).
    quit_export: QuitExport,
    /// rtex's events and stderr, oldest first.
    pub log: String,
    /// The last full run's errors and warnings.
    pub problems: Vec<engine::Diagnostic>,
    /// What the workspace is to show as notifications (it takes them).
    pub notices: Vec<Notice>,
    /// The last full run failed (a later good one says so), and the wait before saying it.
    typeset_failed: bool,
    /// A save failed, rtex is not running: told, so the next save that works, the next rtex
    /// that starts, takes the notification back (it would say what is no longer so).
    save_failed: bool,
    engine_failed: bool,
    typeset_notice: Option<Task<()>>,
    rtex: Option<engine::Rtex>,
    spans: Vec<engine::Span>,
    spans_pending: usize,
    preview: document::Preview,
    blocks: Vec<document::Block>,
    pub outline: Vec<document::Heading>,
    resources: Arc<Mutex<render::Resources>>,
    layout_version: u64,
    images: HashMap<u32, PageImage>,
    versions: HashMap<u32, u64>,
    inflight: HashSet<(u32, u64, (u32, Option<render::DarkPaper>))>,
    /// With the region it shows and the paper it is on.
    live: Option<(Arc<RenderImage>, [f64; 4], Option<render::DarkPaper>)>,
    pub zoom: f32,
    /// Rasterizing waits until a pinch has paused; the stretched old images show meanwhile.
    zoomed_at: Instant,
    scroll: ScrollHandle,
    page_bounds: Rc<Cell<Vec<PageBounds>>>,
    /// The source box's bounds (window coordinates) and the page and height of the lines it
    /// replaces: the page below it is shifted down by the difference.
    box_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    box_cut: Cell<Option<(u32, f32)>>,
    /// What the source box was last sized with.
    box_fit: Rc<Cell<BoxFit>>,
    /// The formula editor's symbol tab.
    formula_tab: usize,
    /// The table editor's cells, while it is open.
    table: Option<table_panel::TableEdit>,
    /// Document fonts registered for the source box: file to family (None: unusable).
    doc_fonts: std::cell::RefCell<HashMap<String, Option<Font>>>,
    /// The insert line under the pointer: its page, height (sp) and where a new block goes.
    hover_gap: Option<(u32, f64, Spot)>,
    /// The last single click's page, point (sp) and block: the first half of a double-click.
    first_click: Option<(u32, f64, f64, Option<usize>)>,
    /// The source before each edit made on the pages (a source box, a menu action), for ⌘Z
    /// outside the box; the box and Source mode have their own undo while focused.
    undo: Vec<String>,
    redo: Vec<String>,
    /// The pages', so actions reach the workspace while no editor is focused.
    focus: FocusHandle,
    /// What the last right-click hit: page, y (sp) and the start of the block there, for the
    /// context menu. Its actions find the block again by that offset when they run.
    menu_target: Rc<Cell<Option<MenuTarget>>>,
    /// The selected blocks (outlined) while no box is open: the anchor and head blocks, by the
    /// source offsets of their starts. Right-click selects one; Shift extends; ⌘A takes all.
    selected: Option<(usize, usize)>,
    input: Entity<EditorState>,
    /// The whole file as source in a code editor: a .tex file's Source mode, any other text file.
    code: Entity<EditorState>,
    /// A .tex file shown as its source instead of its pages.
    pub source_mode: bool,
    /// Line numbers and soft wrap as last applied to `code`.
    code_opts: (bool, bool),
    autosave: Option<Task<()>>,
    active: Option<Active>,
    /// Visual mode's text caret, while no source box is open.
    caret: Option<Caret>,
    /// Text typed since the pages last caught up, drawn at the caret until they do.
    overlay: Option<Overlay>,
    /// The undo step typing is in.
    typing: Option<Typing>,
    /// Caret maps by (block start, page), until the blocks or the source change.
    maps: RefCell<HashMap<(usize, u32), Rc<document::caret::CharMap>>>,
    /// Each page's text, for VoiceOver (the page itself is an image), until the blocks or the
    /// source change.
    page_text: RefCell<HashMap<u32, SharedString>>,
    /// The caret's blink: shown now, and the timer that flips it.
    caret_on: bool,
    blink: Option<Task<()>>,
    /// Where the caret was last drawn (window coordinates), for an input method's window.
    caret_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// A drag that began on text is under way: moving the pointer selects.
    dragging: bool,
    /// A drag of the selected figure's picture handle under way.
    resizing: Option<image_resize::Resizing>,
    /// A line of the selected table's, held to size its column or row.
    line_drag: Option<table_handles::LineDrag>,
    /// A dot of the TikZ editor's canvas held, being dragged.
    pointing: Option<tikz_points::Pointing>,
    tikz_ui: tikz_points::Ui,
    /// The Find and Replace bar, while open.
    find: Option<find_bar::Find>,
    /// Edits sent to rtex, and the source revision it began at (what its first reply had): the
    /// revision of the last edit is their sum.
    edits_sent: u64,
    first_revision: Option<u64>,
    /// When the last edit was made.
    edited_at: Instant,
    /// What the last edit made: the bytes it wrote.
    last_edit: Range<usize>,
    _tasks: Vec<Task<()>>,
    _subs: [Subscription; 4],
}

impl Doc {
    pub fn new(
        root: PathBuf,
        path: String,
        resources: Arc<Mutex<render::Resources>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tex = path.ends_with(".tex");
        let source = std::fs::read_to_string(root.join(&path));
        // an editor (not a textarea) for its completion menu; it never scrolls: the box grows to
        // its text and the page below moves
        let input = cx.new(|cx| box_state(tex, window, cx));
        // syntect names LaTeX "tex" and BibTeX "bib"
        let language = Path::new(&path)
            .extension()
            .map_or("text".into(), |e| e.to_string_lossy().to_string());
        let code = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language)
                .line_number(true)
                .soft_wrap(true)
        });
        let latex = Rc::new(complete::Latex {
            root: root.clone(),
            doc: cx.entity().downgrade(),
        });
        if tex {
            for state in [&input, &code] {
                state.update(cx, |s, _| {
                    let lsp = s.lsp_mut();
                    lsp.completion_provider = Some(latex.clone());
                    lsp.set_completion_highlighter(Rc::new(highlight_characters));
                });
            }
        }
        let code_sub = cx.subscribe_in(&code, window, |this, code, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) && !place_caret(code, window, cx) {
                this.on_code(cx);
            }
        });
        // Source mode: the caret on the pages follows the code editor's cursor
        let code_moves = cx.observe(&code, |this, _, cx| this.follow_code(cx));
        let sub = cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change if !place_caret(input, window, cx) => this.on_input(window, cx),
                // a click anywhere else (project panel, tabs) closes the source box; switching apps does not
                InputEvent::Blur if this.tex && window.is_window_active() => this.leave(window, cx),
                _ => {}
            },
        );
        // key bindings run before key listeners, and the textarea binds the arrows: intercept first
        let weak = cx.entity().downgrade();
        let keys = cx.intercept_keystrokes(move |ev, window, cx| {
            let _ = weak.update(cx, |this, cx| this.on_box_key(&ev.keystroke, window, cx));
        });
        let mut this = Self {
            view: None,
            view_images: HashMap::new(),
            root,
            path,
            tex,
            source: String::new(),
            dirty: false,
            disk: Default::default(),
            conflict: None,
            _watch: Task::ready(()),
            status: String::new(),
            compiling: false,
            quit_runs: 0,
            quit_export: QuitExport::No,
            log: String::new(),
            problems: vec![],
            notices: vec![],
            typeset_failed: false,
            save_failed: false,
            engine_failed: false,
            typeset_notice: None,
            rtex: None,
            spans: vec![],
            spans_pending: 0,
            preview: Default::default(),
            blocks: vec![],
            outline: vec![],
            resources,
            layout_version: 0,
            images: HashMap::new(),
            versions: HashMap::new(),
            inflight: HashSet::new(),
            live: None,
            // fit width waits for the first pages
            zoom: match AppSettings::get(cx).default_zoom {
                Zoom::Percent(p) => (p / 100.0) as f32,
                Zoom::FitWidth => 1.0,
            },
            zoomed_at: Instant::now(),
            scroll: ScrollHandle::new(),
            page_bounds: Default::default(),
            box_bounds: Default::default(),
            box_cut: Cell::new(None),
            first_click: None,
            hover_gap: None,
            doc_fonts: Default::default(),
            box_fit: Default::default(),
            formula_tab: 0,
            edits_sent: 0,
            first_revision: None,
            edited_at: Instant::now(),
            table: None,
            menu_target: Default::default(),
            selected: None,
            undo: vec![],
            redo: vec![],
            focus: cx.focus_handle(),
            input,
            code,
            source_mode: tex && AppSettings::get(cx).open_tex_in == TexView::Source,
            code_opts: (true, true),
            autosave: None,
            active: None,
            caret: None,
            overlay: None,
            typing: None,
            maps: Default::default(),
            page_text: Default::default(),
            caret_on: true,
            blink: None,
            caret_bounds: Default::default(),
            dragging: false,
            resizing: None,
            line_drag: None,
            pointing: None,
            tikz_ui: Default::default(),
            find: None,
            last_edit: 0..0,
            _tasks: vec![],
            _subs: [sub, keys, code_sub, code_moves],
        };
        if viewer::can_view(&this.path) {
            match viewer::open(&this.root.join(&this.path)) {
                Ok(v) => this.view = Some(v),
                Err(e) => {
                    this.status = format!("Couldn’t show {}: {e}", this.path);
                    this.notices.push(Notice::Failed {
                        what: format!("Couldn’t show {}", this.path),
                        why: e.to_string(),
                    });
                }
            }
            return this;
        }
        match source {
            Ok(s) => this.source = s,
            Err(e) => {
                this.status = format!("Couldn’t open {}: {e}", this.path);
                this.notices.push(Notice::Failed {
                    what: format!("Couldn’t open {}", this.path),
                    why: crate::ui::plain_error(&e),
                });
                return this;
            }
        }
        this.outline = document::outline(&this.source);
        this.disk = disk::Disk::of(&this.source, &this.root, &this.path);
        this._watch = Self::watch_disk(window, cx);
        let text = this.source.clone();
        this.code.update(cx, |s, cx| s.set_value(text, window, cx));
        if !tex {
            return this;
        }
        this.start_engine(window, cx);
        this
    }

    /// rtex started on the file (as saved) and listened to.
    fn start_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        match engine::Rtex::start(&self.root, &self.path, tx) {
            Ok(mut r) => {
                r.request_spans();
                self.spans_pending = 1;
                (self.edits_sent, self.first_revision) = (0, None);
                self.rtex = Some(r);
                self.status = "Typesetting…".into();
                if std::mem::take(&mut self.engine_failed) {
                    self.notices.push(Notice::EngineRunning);
                }
            }
            Err(e) => {
                self.status = e.clone();
                self.engine_failed = true;
                self.notices.push(Notice::EngineMissing(e));
            }
        }
        // the last rtex's messages end here: its exit must not stop this one
        self._tasks = vec![cx.spawn_in(window, async move |this, cx| {
            while let Some(msg) = rx.next().await {
                if this
                    .update_in(cx, |this, window, cx| this.on_msg(msg, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })];
    }

    /// Free the page images in the window's texture atlas (when the tab closes).
    pub fn release_images(&mut self, window: &mut Window) {
        for (_, im) in self.images.drain() {
            let _ = window.drop_image(im.img);
        }
        for (_, (_, img)) in self.view_images.drain() {
            let _ = window.drop_image(img);
        }
        if let Some((img, ..)) = self.live.take() {
            let _ = window.drop_image(img);
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        // the file changed on disk and which version stays is not decided: nothing is
        // overwritten unseen
        if self.conflict.is_some() {
            self.status = format!(
                "{} changed on disk: choose which version to keep first",
                self.path
            );
            cx.notify();
            return;
        }
        if self.disk_moved_on() {
            cx.notify();
            return;
        }
        match std::fs::write(self.root.join(&self.path), &self.source) {
            Ok(()) => {
                self.dirty = false;
                self.written();
                if std::mem::take(&mut self.save_failed) {
                    self.notices.push(Notice::Saved);
                }
            }
            Err(e) => {
                self.status = format!("Couldn’t save {}: {e}", self.path);
                self.save_failed = true;
                self.notices
                    .push(Notice::SaveFailed(crate::ui::plain_error(&e)));
            }
        }
        cx.notify();
    }

    /// A full LaTeX run now (references, a changed .bib or included file); the typesetting
    /// engine started again (on the file, saved first) if it had stopped.
    pub fn recompile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.rtex.as_mut() {
            Some(r) => {
                r.request_layout();
                self.compiling = true;
            }
            None if self.tex => {
                self.save(cx);
                self.start_engine(window, cx);
            }
            None => {}
        }
        cx.notify();
    }

    /// The app is quitting: the file saved, then `runs` full LaTeX runs made one after another
    /// (a failed run ends them). False when there is no engine to run them.
    pub fn compile_for_quit(&mut self, runs: u32, cx: &mut Context<Self>) -> bool {
        if !self.tex || self.rtex.is_none() {
            return false;
        }
        self.save(cx);
        if let Some(r) = self.rtex.as_mut() {
            r.request_layout();
        }
        self.compiling = true;
        self.quit_runs = runs;
        self.status = "Compiling before quitting… (⌘Q again quits now)".into();
        cx.notify();
        true
    }

    /// The app is quitting: this document's PDF exported beside its file, after the quit's
    /// compile runs if there are any. False for what is not a whole TeX document (an included
    /// chapter) or has no engine to typeset it.
    pub fn export_for_quit(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.tex || self.rtex.is_none() || !self.source.contains("\\begin{document}") {
            return false;
        }
        self.save(cx);
        self.quit_export = QuitExport::Wanted;
        if self.quit_runs == 0 {
            self.export_now_for_quit();
        }
        cx.notify();
        true
    }

    /// The export wanted for the quit asked of rtex, the compile runs being over.
    fn export_now_for_quit(&mut self) {
        if self.quit_export != QuitExport::Wanted {
            return;
        }
        let Some(r) = self.rtex.as_mut() else {
            self.quit_export = QuitExport::No;
            return;
        };
        let out = self.root.join(&self.path).with_extension("pdf");
        r.export_pdf(&out);
        self.quit_export = QuitExport::Asked;
        let name = out
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());
        self.status = format!("Exporting {name} before quitting… (⌘Q again quits now)");
    }

    /// Whether what the quit asked for (`compile_for_quit`'s runs, `export_for_quit`'s PDF) is
    /// not all done yet.
    pub fn quit_compiling(&self) -> bool {
        self.quit_runs > 0 || self.quit_export != QuitExport::No
    }

    /// A full run came back: the next of the quit's runs asked for, if there is one.
    fn quit_run_done(&mut self) {
        if self.quit_runs == 0 {
            return;
        }
        self.quit_runs -= 1;
        match self.rtex.as_mut() {
            Some(r) if self.quit_runs > 0 => {
                r.request_layout();
                self.compiling = true;
            }
            _ => self.quit_runs = 0,
        }
        if self.quit_runs == 0 {
            self.export_now_for_quit();
        }
    }

    /// rtex stopped and started again (a TeX file only), on the file saved first: rtex reads
    /// it from disk.
    pub fn restart_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tex {
            return;
        }
        self.save(cx);
        self.rtex = None;
        self.start_if_stopped(window, cx);
    }

    /// rtex started if it is not running (a TeX file only).
    pub fn start_if_stopped(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rtex.is_none() && self.tex {
            self.start_engine(window, cx);
            cx.notify();
        }
    }

    /// Ask where (from `into`, else the file's folder), then have rtex typeset the whole
    /// document into a PDF there.
    pub fn export(&mut self, into: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.rtex.is_none() {
            return;
        }
        let stem = Path::new(&self.path)
            .file_stem()
            .map_or("document".into(), |s| s.to_string_lossy().to_string());
        let dir = into.unwrap_or_else(|| {
            self.root
                .join(&self.path)
                .parent()
                .map_or(self.root.clone(), Path::to_path_buf)
        });
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{stem}.pdf")));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(out))) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(r) = this.rtex.as_mut() {
                        r.export_pdf(&out);
                        let name = out
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().to_string());
                        this.status = format!("Exporting {name}…");
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}

impl Render for Doc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.view.is_some() {
            return self.render_view(window, cx);
        }
        let theme = cx.theme().clone();
        let st = AppSettings::get(cx).clone();
        let fs = st.source_font_size as f32;
        if !self.tex || self.source_mode {
            let family = st.source_family();
            // follow the line-number and wrap settings (set only on change: setting them redraws)
            if self.code_opts != (st.line_numbers, st.soft_wrap) {
                self.code_opts = (st.line_numbers, st.soft_wrap);
                self.code.update(cx, |s, cx| {
                    s.set_line_number(st.line_numbers, window, cx);
                    s.set_soft_wrap(st.soft_wrap, window, cx);
                });
            }
            let code = div()
                .size_full()
                .bg(theme.background)
                .font_family(family.clone())
                .child(
                    Editor::new(&self.code)
                        .h_full()
                        .appearance(false)
                        .bordered(false)
                        .font_family(family)
                        .text_size(px(fs)),
                );
            if !self.tex {
                return code.into_any_element();
            }
            // Source mode: the source on the left, its pages beside it
            return div()
                .size_full()
                .flex()
                .child(code.flex_1().min_w_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .border_l_1()
                        .border_color(theme.border)
                        .child(self.render_pages(window, cx)),
                )
                .into_any_element();
        }
        self.render_pages(window, cx)
    }
}

impl Doc {
    /// The typeset pages, scrolled; in Source mode only a preview of the code editor's text.
    fn render_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let st = AppSettings::get(cx).clone();
        self.ensure_images(window, cx);
        self.settle_overlay();
        // the source box's page, or the last one if the document got shorter
        let at = self.box_place();
        let box_page = at.as_ref().map(|at| {
            let last = self
                .preview
                .pages
                .keys()
                .next_back()
                .copied()
                .unwrap_or(at.page);
            if self.preview.pages.contains_key(&at.page) {
                at.page
            } else {
                last
            }
        });
        let mut pages: Vec<AnyElement> = self
            .preview
            .pages
            .iter()
            .map(|(&n, dl)| {
                let here = at.as_ref().filter(|_| box_page == Some(n));
                self.render_page(n, dl, here, window, cx)
            })
            .collect();
        let width = |dl: Option<&Arc<engine::DisplayList>>| {
            page_px(
                dl.and_then(|d| d.page_width).unwrap_or(40258437.0),
                self.zoom,
            )
        };
        // the open source box, over its page: the column's top padding, the preamble strip and a
        // gap come before the first page
        if !pages.is_empty() && !self.source_mode {
            let (w0, w1) = (
                width(self.preview.pages.values().next()),
                width(self.preview.pages.values().last()),
            );
            pages.insert(0, self.render_frame("Preamble", w0, cx));
            pages.push(self.render_frame("Trailer", w1, cx));
        } else if self.tex {
            // no pages yet: what is happening, rather than an empty desk
            let name = Path::new(&self.path)
                .file_name()
                .map_or(self.path.clone(), |n| n.to_string_lossy().to_string());
            let (title, detail) = if self.rtex.is_some() {
                (
                    format!("Typesetting {name}…"),
                    "The first time can take a minute, while LaTeX builds its font list.",
                )
            } else {
                (
                    "rtex isn’t running".to_string(),
                    "See the status bar for why. Recompile (⌘⇧R) tries again.",
                )
            };
            let muted = theme.muted_foreground;
            pages.push(
                div()
                    .mt(px(120.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .when(self.rtex.is_some(), |d| {
                        d.child(
                            Spinner::new()
                                .icon(Icon::default().path("icons/loader-circle.svg"))
                                .with_size(Size::Medium)
                                .color(muted),
                        )
                    })
                    .child(div().text_sm().child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(crate::ui::keys_in(detail)),
                    )
                    .into_any_element(),
            );
        }
        // the selected blocks' keys; with no selection (or a box open) the key goes on
        type SelAct = fn(&mut Doc, usize, usize, &mut Window, &mut Context<Doc>);
        let on_sel = |f: SelAct| {
            move |this: &mut Doc, window: &mut Window, cx: &mut Context<Doc>| match this
                .selection()
                .filter(|_| this.active.is_none())
            {
                Some((lo, hi)) => f(this, lo, hi, window, cx),
                None => cx.propagate(),
            }
        };
        let typing = self.caret.is_some() && self.active.is_none();
        let view = caret_actions(div().id("pages"), cx)
            .on_action(cx.listener(|d, _: &OpenFind, w, cx| d.open_find(false, w, cx)))
            .on_action(cx.listener(|d, _: &PastePlain, _, cx| {
                if d.caret.is_some() && d.active.is_none() {
                    d.paste_at_caret(true, cx)
                }
            }))
            .on_action(cx.listener(|d, _: &OpenReplace, w, cx| d.open_find(true, w, cx)))
            .on_action(cx.listener(|d, _: &FindNext, w, cx| d.find_step(1, w, cx)))
            .on_action(cx.listener(|d, _: &FindPrev, w, cx| d.find_step(-1, w, cx)));
        let view = view
            .key_context(if typing { "Caret" } else { "Pages" })
            .track_focus(&self.focus)
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| d.activate_range(lo, hi, w, cx));
                move |this, _: &EditBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| d.delete_blocks(lo, hi, w, cx));
                move |this, _: &DeleteBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.duplicate_blocks(lo, hi, cx));
                move |this, _: &DuplicateBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.move_blocks(lo, hi, true, cx));
                move |this, _: &MoveBlockUp, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.move_blocks(lo, hi, false, cx));
                move |this, _: &MoveBlockDown, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, _, _, cx| {
                    if lo > 0 {
                        d.set_selection(lo - 1, lo - 1, cx)
                    }
                });
                move |this, _: &SelectPrevBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, hi, _, cx| d.set_selection(hi + 1, hi + 1, cx));
                move |this, _: &SelectNextBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.extend_selection(-1, cx));
                move |this, _: &ExtendSelectionUp, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.extend_selection(1, cx));
                move |this, _: &ExtendSelectionDown, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.clear_selection(cx));
                move |this, _: &ClearSelection, w, cx| f(this, w, cx)
            }))
            // ⌘A on the pages: every block (in a box, the box's text)
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                if this.caret.is_some() && this.active.is_none() {
                    return this.select_all_text(cx);
                }
                if this.active.is_some() || this.blocks.is_empty() {
                    cx.propagate();
                    return;
                }
                let last = this.blocks.len() - 1;
                this.selected = Some((this.blocks[0].range.start, this.blocks[last].range.start));
                cx.notify();
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.copy_blocks(lo, hi, cx));
                move |this, _: &Copy, w, cx| {
                    if !(this.active.is_none() && this.copy_selection(cx)) {
                        f(this, w, cx)
                    }
                }
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| {
                    d.copy_blocks(lo, hi, cx);
                    d.delete_blocks(lo, hi, w, cx);
                });
                move |this, _: &Cut, w, cx| {
                    if this.active.is_none() && this.copy_selection(cx) {
                        this.delete_selection(cx);
                    } else {
                        f(this, w, cx)
                    }
                }
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, hi, w, cx| {
                    if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        d.insert_at(Spot::Below(hi), &t, w, cx);
                    }
                });
                move |this, _: &Paste, w, cx| {
                    if this.caret.is_some() && this.active.is_none() {
                        this.paste_at_caret(false, cx)
                    } else {
                        f(this, w, cx)
                    }
                }
            }))
            .size_full()
            // typing and input methods reach the caret through the pages' input handler
            .when(typing, |v| {
                let (focus, doc) = (self.focus.clone(), cx.entity());
                v.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, doc.clone()),
                                cx,
                            )
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            // the desk, from the theme: in the dark, between the panel and a black page
            .bg(palette::desk(&theme))
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            // a click beside the pages closes the source box
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.leave(window, cx)),
            )
            .when(st.pinch_zoom, |v| {
                v.on_pinch(cx.listener(|this, ev: &PinchEvent, window, cx| {
                    this.set_zoom(this.zoom * (1.0 + ev.delta), Some(ev.position), window, cx)
                }))
            })
            .child(
                div()
                    .id("page-column")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(PAGE_GAP))
                    // page_y counts the preamble strip, which Source mode does not show
                    .pt(px(if self.source_mode {
                        PAGES_TOP
                    } else {
                        PAGES_TOP - FRAME_H - PAGE_GAP
                    }))
                    .pb(px(48.0))
                    .px_8()
                    .relative()
                    .min_w(px(self.column_min_width()))
                    .children(pages)
                    .context_menu(self.page_menu(cx)),
            );
        // the scrollbars sit over the scroll area as a sibling, so they do not scroll with the
        // pages; always shown, like an editor's
        div()
            .relative()
            .size_full()
            .child(view)
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&self.scroll)
                        .axis(ScrollbarAxis::Both)
                        .viewport_from_layout()
                        .mode(ScrollbarMode::Always),
                ),
            )
            .children(self.render_find_bar(cx))
            .into_any_element()
    }
}

fn page_px(sp: f64, zoom: f32) -> f32 {
    (sp / SP_PER_BP) as f32 * zoom
}

fn union(boxes: impl Iterator<Item = [f64; 4]>) -> Option<[f64; 4]> {
    boxes.reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}

fn to_image(w: u32, h: u32, bgra: Vec<u8>) -> Arc<RenderImage> {
    let buf = image::RgbaImage::from_raw(w, h, bgra).expect("buffer size matches");
    Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
}
