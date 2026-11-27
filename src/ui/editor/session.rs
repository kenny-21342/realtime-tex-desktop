// Keeping a Doc in step with its rtex session: layouts and live paragraphs in, page images and
// the formula preview out.
use super::*;

/// How long a failed full run stays failed before the notification says so.
const TYPESET_NOTICE_AFTER: Duration = Duration::from_secs(2);

/// The most of rtex's log a Doc keeps (bytes); past it the older half goes.
const LOG_MAX: usize = 256 * 1024;

/// `line` added to `log`; a log past `max` bytes keeps only its newer half, from a line start.
fn push_log(log: &mut String, line: &str, max: usize) {
    log.push_str(line);
    log.push('\n');
    if log.len() > max {
        let cut = log.ceil_char_boundary(log.len() - max / 2);
        let cut = log[cut..].find('\n').map_or(cut, |i| cut + i + 1);
        log.drain(..cut);
    }
}

impl Doc {
    pub(super) fn on_msg(&mut self, msg: engine::Msg, window: &mut Window, cx: &mut Context<Self>) {
        match msg {
            // these change nothing on the pages: kept, without redoing the blocks below
            engine::Msg::Log(l) => {
                push_log(&mut self.log, &l, LOG_MAX);
                cx.notify();
                return;
            }
            // underfull boxes and the like (info) would drown the errors
            engine::Msg::Diagnostics(d) => {
                self.problems = d.into_iter().filter(|d| d.severity != "info").collect();
                cx.notify();
                return;
            }
            engine::Msg::Layout {
                pages,
                pages_total,
                placements,
                stale_since,
                layout_version,
                source_revision,
            } => {
                self.note_revision(source_revision);
                self.layout_version = layout_version;
                let first = self.preview.pages.is_empty();
                let dirty = self
                    .preview
                    .apply_layout(pages, pages_total, placements, stale_since);
                if first
                    && !self.preview.pages.is_empty()
                    && AppSettings::get(cx).default_zoom == Zoom::FitWidth
                {
                    self.fit_width(window, cx);
                }
                self.bump(dirty);
                self.status.clear();
                self.compiling = stale_since.is_some();
                // a full run that caught up with the source
                if stale_since.is_none() {
                    self.quit_run_done();
                }
                if std::mem::take(&mut self.typeset_failed) {
                    self.typeset_notice = None;
                    self.notices.push(Notice::Typeset);
                }
                // behind the source, it has not the typed text yet
                if stale_since.is_none() {
                    self.overlay_seen(u64::MAX);
                }
            }
            engine::Msg::Paragraph {
                par_id,
                removed,
                dl,
                fragments,
                source_revision,
            } => {
                let dirty = self.preview.apply_paragraph(
                    par_id,
                    dl,
                    if removed { vec![] } else { fragments },
                    source_revision,
                );
                self.bump(dirty);
                self.status.clear();
                self.note_revision(source_revision);
                self.overlay_seen(source_revision);
            }
            engine::Msg::Spans(spans) => {
                self.spans_pending = self.spans_pending.saturating_sub(1);
                // a reply older than the last edit has stale ranges; the last request will answer
                if self.spans_pending == 0 {
                    self.spans = spans;
                }
            }
            engine::Msg::Status(s) => self.status = s,
            engine::Msg::Failed => {
                self.status = "LaTeX couldn’t typeset your last change: the pages still show the document from before it. Undo (⌘Z) or fix the TeX".into();
                self.typeset_failed = true;
                // told once typing pauses: a run that fails mid-edit and works a moment later
                // (each failure restarts the wait) never flashes a warning beside the text
                self.typeset_notice = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(TYPESET_NOTICE_AFTER).await;
                    let _ = this.update(cx, |d, cx| {
                        if d.typeset_failed {
                            d.notices.push(Notice::TypesetFailed);
                            cx.notify();
                        }
                    });
                }));
                self.compiling = false;
                self.quit_runs = 0;
                self.quit_export = QuitExport::No;
            }
            engine::Msg::Compiling => self.compiling = true,
            engine::Msg::Exported { path, ok } => {
                let name = |p: &str| {
                    Path::new(p)
                        .file_name()
                        .map_or(p.to_string(), |n| n.to_string_lossy().to_string())
                };
                // the export a quit asked for is no time to open a viewer
                let quitting =
                    std::mem::replace(&mut self.quit_export, QuitExport::No) == QuitExport::Asked;
                if let Some(p) = path
                    .as_ref()
                    .filter(|_| !quitting && AppSettings::get(cx).open_after_export)
                {
                    cx.open_with_system(Path::new(p));
                }
                self.notices.push(Notice::Exported {
                    path: path.clone(),
                    ok,
                });
                self.status = match (path, ok) {
                    (Some(p), true) => format!("Exported {}", name(&p)),
                    (Some(p), false) => format!(
                        "Exported {}, with LaTeX errors: parts may be wrong (see Problems)",
                        name(&p)
                    ),
                    (None, _) => {
                        "Couldn’t export the PDF: LaTeX made none. Fix the errors in Problems, then export again".into()
                    }
                }
            }
            engine::Msg::Exited(tail) => {
                // rtex rarely says why (its stderr is usually empty)
                let why = match tail.trim() {
                    "" => String::new(),
                    t => format!(" ({t})"),
                };
                self.status = format!(
                    "rtex stopped{why}: the pages won’t update until it restarts (Recompile, ⌘⇧R)"
                );
                self.engine_failed = true;
                self.notices.push(Notice::EngineStopped(tail));
                self.rtex = None;
                self.compiling = false;
                self.quit_runs = 0;
                self.quit_export = QuitExport::No;
            }
        }
        self.blocks = document::blocks(&self.spans, &self.source, &self.preview);
        self.maps.borrow_mut().clear();
        self.page_text.borrow_mut().clear();
        self.render_live(window, cx);
        cx.notify();
    }

    /// rtex copies each installed layout to bg/layout-<version>.pdf; one still being written
    /// does not end with %%EOF yet.
    pub(super) fn layout_pdf(&self) -> Option<PathBuf> {
        let p = engine::build_dir(&self.root, &self.path)
            .join(format!("bg/layout-{}.pdf", self.layout_version));
        let data = std::fs::read(&p).ok()?;
        let tail = &data[data.len().saturating_sub(1024)..];
        tail.windows(5).any(|w| w == b"%%EOF").then_some(p)
    }

    pub(super) fn bump(&mut self, pages: BTreeSet<u32>) {
        for p in pages {
            *self.versions.entry(p).or_default() += 1;
        }
    }

    /// Rasterize pages near the viewport whose image is missing, outdated or at another zoom.
    pub(super) fn ensure_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settle = Duration::from_millis(150);
        let since = self.zoomed_at.elapsed();
        if since < settle {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(settle - since).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
            return;
        }
        let scale = self.zoom as f64 * window.scale_factor() as f64 / SP_PER_BP;
        let dark = self.dark_pages(cx).then(|| palette::dark_paper(cx));
        // the key holds the paper too, so a theme switch or a colour changed redraws every page
        let zkey = ((scale * 1e6) as u32, dark);
        if self.live.as_ref().is_some_and(|l| l.2 != dark) {
            if let Some((old, ..)) = self.live.take() {
                let _ = window.drop_image(old);
            }
            self.render_live(window, cx);
        }
        let view_h = f32::from(self.scroll.bounds().size.height).max(1000.0);
        let view_top = -f32::from(self.scroll.offset().y);
        let mut y = PAGES_TOP;
        for (&n, dl) in &self.preview.pages {
            let h = page_px(dl.page_height.unwrap_or(0.0), self.zoom);
            let near = y + h > view_top - view_h && y < view_top + 2.0 * view_h;
            y += h + PAGE_GAP;
            let v = self.versions.get(&n).copied().unwrap_or(0);
            if !near
                || self.images.get(&n).is_some_and(|im| im.key == (v, zkey))
                || !self.inflight.insert((n, v, zkey))
            {
                continue;
            }
            let (mut rows, live, clear) = self.preview.page_parts(n);
            // a degraded page (TikZ…) comes from the layout's PDF, with live rows drawn over it
            let pdf = self
                .preview
                .inexact
                .contains(&n)
                .then(|| self.layout_pdf())
                .flatten();
            if pdf.is_some() {
                rows.clear();
            }
            rows.extend(live);
            let job = render::Job {
                rows,
                other: Some(dl.clone()),
                region: [
                    0.0,
                    0.0,
                    dl.page_width.unwrap_or(40258437.0),
                    dl.page_height.unwrap_or(52099153.0),
                ],
                scale,
                clear: if pdf.is_some() { clear } else { vec![] },
                pdf: pdf.map(|p| (p, n as usize - 1)),
                dark,
            };
            let fonts = self.resources.clone();
            let task = cx.background_executor().spawn(async move {
                render::render(&mut fonts.lock().unwrap_or_else(|e| e.into_inner()), &job)
            });
            cx.spawn_in(window, async move |this, cx| {
                let (w, h, data) = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.inflight.remove(&(n, v, zkey));
                    if this.versions.get(&n).copied().unwrap_or(0) != v {
                        // outdated: redraw, so the next frame asks for the current version
                        cx.notify();
                        return;
                    }
                    let img = to_image(w, h, data);
                    if let Some(old) = this.images.insert(
                        n,
                        PageImage {
                            img,
                            key: (v, zkey),
                        },
                    ) {
                        let _ = window.drop_image(old.img);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// The live typeset result of the block being edited, shown under its source.
    pub(super) fn render_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // the formula editor has no result under it: the page itself shows it
        if self.active.as_ref().is_some_and(|a| a.in_panel()) {
            return;
        }
        let Some(b) = self.active_block() else { return };
        let rows: Vec<render::Row> = b.rows.iter().map(|r| r.row.clone()).collect();
        let Some(bbox) = union(b.rows.iter().map(|r| r.bbox)) else {
            return;
        };
        let pad = 2.0 * 65536.0;
        let region = [
            bbox[0] - pad,
            bbox[1] - pad,
            bbox[2] - bbox[0] + 2.0 * pad,
            bbox[3] - bbox[1] + 2.0 * pad,
        ];
        let dark = self.dark_pages(cx).then(|| palette::dark_paper(cx));
        let job = render::Job {
            rows,
            other: None,
            region,
            scale: self.zoom as f64 * window.scale_factor() as f64 / SP_PER_BP,
            pdf: None,
            clear: vec![],
            dark,
        };
        let fonts = self.resources.clone();
        let task = cx.background_executor().spawn(async move {
            render::render(&mut fonts.lock().unwrap_or_else(|e| e.into_inner()), &job)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (w, h, data) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some((old, ..)) = this.live.replace((to_image(w, h, data), region, dark)) {
                    let _ = window.drop_image(old);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Pages on dark paper: the dark theme, unless the settings keep pages white.
    pub(super) fn dark_pages(&self, cx: &App) -> bool {
        palette::dark_pages(cx)
    }

    /// The page at the top third of the view, and how many there are.
    pub fn page_position(&self) -> (u32, u32) {
        let mark =
            -f32::from(self.scroll.offset().y) + f32::from(self.scroll.bounds().size.height) / 3.0;
        let mut y = PAGES_TOP;
        for (&n, dl) in &self.preview.pages {
            y += page_px(dl.page_height.unwrap_or(0.0), self.zoom) + PAGE_GAP;
            if y > mark {
                return (n, self.preview.pages_total);
            }
        }
        (self.preview.pages_total, self.preview.pages_total)
    }
}

#[cfg(test)]
mod tests {
    use super::push_log;

    #[test]
    fn the_log_drops_its_older_half_at_whole_lines() {
        let mut log = String::new();
        for i in 0..10 {
            push_log(&mut log, &format!("line {i} é"), 40);
        }
        assert!(log.len() <= 40, "{log:?}");
        assert!(log.ends_with("line 9 é\n"));
        assert!(log.starts_with("line "), "cut at a line start: {log:?}");
    }
}
