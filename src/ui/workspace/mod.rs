// The window: title bar, project panel (files and outline), tabs of open files and status bar,
// laid out like Zed.
mod chrome;
mod files;
mod insert_tab;
mod inspector;
mod layout;
mod notices;
mod page_setup;
pub use layout::MIN_WINDOW;
pub use notices::place_notifications;
mod project_panel;
mod ribbon;
pub mod tour;
mod widgets;

use files::*;

use crate::render;
use crate::render::viewer;
use crate::ui::editor::{Doc, ZOOMS};
use crate::ui::settings;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Redo, Undo};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::tree::{TreeEvent, TreeItem, TreeState, tree};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, TitleBar, h_resizable, resizable_panel,
    v_resizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

actions!(
    realtime_tex,
    [
        Save,
        Quit,
        CloseTab,
        Export,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleSidebar,
        ToggleSource,
        Recompile,
        OpenSettings,
        ToggleInspector,
        ToggleTerminal,
        InsertLink,
        InsertPicture,
        InsertTable,
        InsertEquation,
        InsertFootnote,
        InsertCrossReference,
        InsertCitation,
        InsertPageBreak,
        InsertContents,
        FitWidth,
        MinimizeWindow,
        ZoomWindow,
        ToggleFullScreen
    ]
);

/// Taller than gpui-kit's 34px so the toolbar buttons have room; the traffic lights are centred in it.
pub const TITLE_BAR_H: f32 = 40.0;

pub struct Workspace {
    root: PathBuf,
    tree: Entity<TreeState>,
    tabs: Vec<Entity<Doc>>,
    active: usize,
    resources: Arc<Mutex<render::Resources>>,
    sidebar: bool,
    /// The sidebar on the right, for the element at the caret.
    inspector: inspector::Inspector,
    /// The ribbon's open tab.
    ribbon_tab: ribbon::RibbonTab,
    /// What the window's width allows: which panels show, how wide the ribbon is.
    fit: layout::Fit,
    /// The colour palette's hex field.
    color_hex: Entity<InputState>,
    /// The colour the text colour button puts on (the last one picked), as Word's does.
    last_color: String,
    /// The table grid's size fields, for sizing a table from the keyboard.
    table_rows: Entity<InputState>,
    table_cols: Entity<InputState>,
    status: String,
    /// What shows under the pages (the problems or the rtex log); the log's scroll, and how
    /// long it was when last drawn (it follows new lines to the bottom).
    bottom: Option<chrome::Bottom>,
    /// The terminal in the panel under the pages, kept (its shell too) while the panel is hidden.
    terminal: Option<Entity<crate::ui::terminal::Terminal>>,
    log_scroll: ScrollHandle,
    log_seen: usize,
    /// Folders open in the project panel (paths): only they are read from disk, so a large
    /// folder (node_modules) costs nothing until it is opened.
    expanded: std::collections::HashSet<String>,
    /// The project panel's file tree and outline, scrolled sideways to names too long for it.
    files_scroll: ScrollHandle,
    outline_scroll: ScrollHandle,
    /// Which of the project panel's two sections are open (a closed one shows its title only).
    files_open: bool,
    outline_open: bool,
    /// The heading the outline last marked as the one the pages are under (its offset).
    outline_seen: Option<usize>,
    /// The tree entry being renamed in place (its path) and its name field, and why the name
    /// typed cannot be (shown under the field, which stays open).
    rename: Option<(String, Entity<InputState>)>,
    rename_error: Option<String>,
    /// Files whose failed save was asked about already (later failures only notify).
    save_alerted: std::collections::HashSet<String>,
    /// The tour, in its scratch project's window.
    tour: Option<tour::Tour>,
    _subs: Vec<Subscription>,
    _rename_sub: Option<Subscription>,
    _tree_watch: Task<()>,
    _dialog_subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(
        root: PathBuf,
        main: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let resources: Arc<Mutex<render::Resources>> = Default::default();
        resources.lock().unwrap().root = root.clone();
        let mut this = Self {
            tree: cx.new(|cx| TreeState::new(cx)),
            root,
            tabs: vec![],
            active: 0,
            resources,
            sidebar: true,
            inspector: Default::default(),
            ribbon_tab: Default::default(),
            fit: Default::default(),
            color_hex: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 1A2B3C")),
            last_color: "red".into(),
            table_rows: cx.new(|cx| InputState::new(window, cx).default_value("3")),
            table_cols: cx.new(|cx| InputState::new(window, cx).default_value("3")),
            status: String::new(),
            bottom: None,
            terminal: None,
            log_scroll: ScrollHandle::new(),
            log_seen: 0,
            rename: None,
            rename_error: None,
            save_alerted: Default::default(),
            tour: None,
            expanded: Default::default(),
            files_scroll: ScrollHandle::new(),
            outline_scroll: ScrollHandle::new(),
            files_open: true,
            outline_open: true,
            outline_seen: None,
            _subs: vec![],
            _rename_sub: None,
            _tree_watch: Task::ready(()),
            _dialog_subs: vec![],
        };
        crate::ui::warm_fonts(cx);
        this._tree_watch = Self::watch_tree(window, cx);
        // a colour typed in the palette's hex field, on Enter
        let hex = this.color_hex.clone();
        this._subs.push(cx.subscribe_in(
            &hex,
            window,
            |this, input, ev: &InputEvent, window, cx| {
                if !matches!(ev, InputEvent::PressEnter { .. }) {
                    return;
                }
                let v = input
                    .read(cx)
                    .value()
                    .trim()
                    .trim_start_matches('#')
                    .to_string();
                if v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                    this.pick_color(v.to_ascii_uppercase(), window, cx);
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                }
            },
        ));
        let sub = cx.subscribe(&this.tree, |this, _, ev: &TreeEvent, cx| {
            match ev {
                TreeEvent::Expanded(id) => this.expanded.insert(id.to_string()),
                TreeEvent::Collapsed(id) => this.expanded.remove(id.as_ref()),
            };
            // an opened folder is read now
            if matches!(ev, TreeEvent::Expanded(_)) {
                this.refresh_tree(cx);
            }
        });
        this._subs.push(sub);
        // the tree again when the settings change what it hides
        let filter = |cx: &App| {
            let s = settings::AppSettings::get(cx);
            (s.show_hidden_files, s.hide_files.clone())
        };
        let mut shown = filter(cx);
        this._subs
            .push(cx.observe_global::<settings::AppSettings>(move |this, cx| {
                if filter(cx) != shown {
                    shown = filter(cx);
                    this.refresh_tree(cx);
                }
            }));
        this.refresh_tree(cx);
        match main.or_else(|| crate::engine::find_main(&this.root)) {
            Some(m) => this.open(m, window, cx),
            None => {
                this.status =
                    "No document here: this folder has no .tex file with \\documentclass. Open one from the project panel".into()
            }
        }
        if this.root == tour::dir() {
            this.tour_start(window, cx);
        }
        this
    }

    /// The file shown, relative to the folder.
    pub fn shown_file(&self, cx: &App) -> Option<String> {
        self.doc().map(|d| d.read(cx).path.clone())
    }

    fn doc(&self) -> Option<&Entity<Doc>> {
        self.tabs.get(self.active)
    }

    /// rtex started in the documents that could not start it (it was downloaded since).
    pub fn start_stopped(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for d in &self.tabs {
            d.update(cx, |d, cx| d.start_if_stopped(window, cx));
        }
    }

    /// rtex started again in every document (a TeX installed since: its PATH has it).
    pub fn restart_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for d in &self.tabs {
            d.update(cx, |d, cx| d.restart_engine(window, cx));
        }
    }

    /// The app is quitting: every TeX document saved and compiled `runs` times (none when 0),
    /// then its PDF exported if `export`; whether any has work to wait for.
    pub fn prepare_quit(&mut self, runs: u32, export: bool, cx: &mut Context<Self>) -> bool {
        let mut any = false;
        for d in &self.tabs {
            any |= d.update(cx, |d, cx| {
                let mut busy = runs > 0 && d.compile_for_quit(runs, cx);
                if export {
                    busy |= d.export_for_quit(cx);
                }
                busy
            });
        }
        any
    }

    /// Whether a document is still making the compile runs of a quit.
    pub fn quit_compiling(&self, cx: &App) -> bool {
        self.tabs.iter().any(|d| d.read(cx).quit_compiling())
    }

    /// The open documents' build folders.
    pub fn build_dirs(&self, cx: &App) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .map(|d| crate::engine::build_dir(&self.root, &d.read(cx).path))
            .collect()
    }

    /// The PDF exported (the tour's into Documents: its folder goes when its window closes).
    fn export(&mut self, cx: &mut Context<Self>) {
        let into = (self.root == tour::dir()).then(tour::export_dir);
        self.with_doc(cx, |d, cx| d.export(into, cx));
    }

    /// The full LaTeX run started now (the tour waits for it).
    fn recompile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = &mut self.tour {
            t.since.recompiled = true;
        }
        self.with_doc(cx, |d, cx| d.recompile(window, cx));
    }

    fn with_doc(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut Doc, &mut Context<Doc>)) {
        if let Some(d) = self.doc() {
            d.update(cx, f);
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tour_check(window, cx);
        let theme = cx.theme().clone();
        window.set_window_edited(self.tabs.iter().any(|d| d.read(cx).dirty));
        // a click outside the page view (project panel, tabs, status bar) closes the source box
        let leave =
            |this: &mut Self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>| {
                this.with_doc(cx, |d, cx| d.leave(window, cx))
            };
        // the window's width decides which panels have room
        self.fit_panels(window, cx);
        let inspector = if self.fit.right {
            div()
                .relative()
                .size_full()
                .child(self.render_inspector(window, cx))
                .children(self.tour_mark("sidebar"))
                .child(layout::measure(self.fit.right_w.clone()))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let bottom = match self.bottom {
            Some(chrome::Bottom::Logs) => self.render_logs(cx).into_any_element(),
            Some(chrome::Bottom::Problems) => self.render_problems(cx).into_any_element(),
            Some(chrome::Bottom::Terminal) => match &self.terminal {
                Some(t) => div().size_full().child(t.clone()).into_any_element(),
                None => div().into_any_element(),
            },
            None => div().into_any_element(),
        };
        let bottom = div()
            .relative()
            .size_full()
            .child(bottom)
            .child(layout::measure_height(self.fit.bottom_h.clone()))
            .children(self.tour_mark("bottom"));
        // the notifications in the pages' corner, clear of the panel under them and the sidebar
        // (the window in front sets it: the setting is the app's, not each window's)
        if window.is_window_active() {
            let below = if self.bottom.is_some() {
                self.fit.bottom_h.get()
            } else {
                0.0
            };
            let beside = if self.fit.right {
                self.fit.right_w.get()
            } else {
                0.0
            };
            let width = f32::from(window.viewport_size().width);
            notices::fit_notifications(below, beside, width, cx);
        }
        let pages = div()
            .relative()
            .size_full()
            .children(self.tour_mark("pages"))
            .children(self.doc().cloned())
            .children(self.render_conflict(cx))
            .when(self.tabs.is_empty(), |d| {
                d.flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground)
                    .child("Open a file from the project panel")
            });
        // the tabs and the ribbon across the whole width (the styles have room), the sidebar
        // beside the pages under them
        let center = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, cx.listener(leave))
                    .child(self.render_tabs(cx)),
            )
            .child(
                div()
                    .relative()
                    .children(self.render_ribbon(cx))
                    .children(self.tour_mark("ribbon")),
            )
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("pages-sidebar")
                        .child(resizable_panel().child(pages))
                        .child(
                            resizable_panel()
                                .size(px(280.0))
                                .size_range(px(220.0)..px(520.0))
                                .visible(self.fit.right)
                                .child(inspector),
                        ),
                ),
            );
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &Save, _, cx| this.with_doc(cx, |d, cx| d.save(cx))))
            .on_action(cx.listener(|this, _: &Undo, window, cx| {
                this.with_doc(cx, |d, cx| d.undo(window, cx))
            }))
            .on_action(cx.listener(|this, _: &Redo, window, cx| {
                this.with_doc(cx, |d, cx| d.redo(window, cx))
            }))
            .on_action(
                cx.listener(|this, _: &CloseTab, window, cx| this.close(this.active, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Export, _, cx| this.export(cx)))
            .on_action(cx.listener(|this, _: &Recompile, window, cx| this.recompile(window, cx)))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| {
                this.with_doc(cx, |d, cx| d.zoom_step(1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| {
                this.with_doc(cx, |d, cx| d.zoom_step(-1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomReset, window, cx| {
                this.with_doc(cx, |d, cx| d.set_zoom(1.0, None, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleSource, window, cx| {
                this.with_doc(cx, |d, cx| {
                    let on = !d.source_mode;
                    d.set_source_mode(on, window, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_left(cx)))
            .on_action(cx.listener(|this, _: &FitWidth, window, cx| {
                this.with_doc(cx, |d, cx| d.fit_width(window, cx))
            }))
            .on_action(
                cx.listener(|this, _: &InsertLink, window, cx| this.open_link_dialog(window, cx)),
            )
            // the Insert menu: what the Insert tab does, for the keyboard and the menu bar
            .on_action(cx.listener(|this, _: &InsertPicture, _, cx| this.choose_picture(cx)))
            .on_action(cx.listener(|this, _: &InsertTable, _, cx| {
                this.with_doc(cx, |d, cx| d.insert_table(3, 3, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertEquation, window, cx| {
                this.with_doc(cx, |d, cx| d.insert_display_math(window, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertFootnote, window, cx| {
                this.with_doc(cx, |d, cx| d.insert_footnote(window, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertCrossReference, window, cx| {
                this.open_reference_picker(window, cx)
            }))
            .on_action(cx.listener(|this, _: &InsertCitation, window, cx| {
                this.open_citation_picker(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &InsertPageBreak, _, cx| this.insert_block("\\newpage", cx)),
            )
            .on_action(cx.listener(|this, _: &InsertContents, _, cx| {
                this.insert_block("\\tableofcontents", cx)
            }))
            .on_action(|_: &MinimizeWindow, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(cx.listener(|this, _: &ToggleInspector, _, cx| this.toggle_right(cx)))
            .on_action(
                cx.listener(|this, _: &ToggleTerminal, window, cx| {
                    this.toggle_terminal(window, cx)
                }),
            )
            // the title bar keeps its mouse to itself: it moves the window
            .child(self.render_title_bar(cx))
            .child(
                div().flex_1().min_h_0().child(
                    v_resizable("workspace-logs")
                        .child(
                            resizable_panel().child(
                                h_resizable("workspace")
                                    .child(
                                        resizable_panel()
                                            .size(px(260.0))
                                            .size_range(px(160.0)..px(640.0))
                                            .visible(self.fit.left)
                                            .child(
                                                div()
                                                    .relative()
                                                    .size_full()
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(leave),
                                                    )
                                                    .child(self.render_sidebar(cx))
                                                    .children(self.tour_mark("files")),
                                            ),
                                    )
                                    .child(resizable_panel().child(center)),
                            ),
                        )
                        .child(
                            resizable_panel()
                                .size(px(200.0))
                                .size_range(px(80.0)..px(800.0))
                                .visible(self.bottom.is_some())
                                .child(bottom),
                        ),
                ),
            )
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, cx.listener(leave))
                    .child(self.render_status_bar(cx)),
            )
            .children(self.render_tour(window, cx))
    }
}

/// The Settings window.
pub fn open_settings(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(860.0), px(620.0)), cx)),
        ..TitleBar::window_options()
    };
    let _ = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| settings::SettingsView::new(window, cx))
    });
}
