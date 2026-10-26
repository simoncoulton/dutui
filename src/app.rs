use crate::{
    config::Config,
    scan::{self, Entry, Msg, Node, ScanHandle, Skip},
    theme::{self, THEMES},
    treemap::Cell,
};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    thread,
    time::{Duration, Instant},
};

pub enum JobKind {
    Initial,
    /// Rescan the same directory, restoring the cursor afterwards.
    Rescan {
        names: Vec<Box<str>>,
        sel_name: Option<Box<str>>,
    },
    /// Scan the parent of the current root and graft the old root back in.
    Up,
}

/// A top-level entry of the root, aligned with `root.children` while scanning.
pub struct Slot {
    id: usize,
    /// Set while the directory is still being walked.
    live: Option<Arc<AtomicU64>>,
}

pub struct Job {
    pub handle: ScanHandle,
    pub kind: JobKind,
    pub target: PathBuf,
    pub began: Instant,
    slots: Vec<Slot>,
}

pub struct Confirm {
    pub path: PathBuf,
    pub name: Box<str>,
    pub size: u64,
    pub count: u64,
    pub is_dir: bool,
    /// Delete for good instead of moving to the Trash.
    pub permanent: bool,
}

struct Deleting {
    rx: Receiver<io::Result<()>>,
    path: PathBuf,
    name: Box<str>,
    permanent: bool,
}

pub struct Modal {
    pub sel: usize,
    orig: usize,
}

pub struct App {
    pub cfg: Config,
    pub theme_idx: usize,
    pub root: Node,
    pub root_path: PathBuf,
    /// Child indices leading from `root` to the directory being shown.
    pub path: Vec<usize>,
    pub sel: usize,
    pub scroll: usize,
    pub job: Option<Job>,
    pub quit: bool,
    pub help: bool,
    pub modal: Option<Modal>,
    pub confirm: Option<Confirm>,
    deleting: Option<Deleting>,
    pub modal_area: Rect,
    pub modal_rows: u16,
    pub status: String,
    /// How long the last completed scan took.
    pub scan_time: Option<Duration>,
    pub started: Instant,
    // Filled in by the renderer for mouse hit-testing.
    pub list_area: Rect,
    pub map_hits: Vec<(Cell, Vec<usize>)>,
    last_click: Option<(Instant, usize)>,
}

fn trash_item(path: &Path) -> io::Result<()> {
    let mut ctx = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        // NSFileManager is much faster than scripting Finder and needs no permission prompt.
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete(path)
        .map_err(|e| io::Error::other(e.to_string()))
}

pub fn cur<'a>(root: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(root, |n, &i| &n.children[i])
}

impl App {
    pub fn new(cfg: Config, root_path: PathBuf) -> Self {
        crate::ui::set_iec(cfg.units.eq_ignore_ascii_case("iec"));
        let theme_idx = theme::index_of(&cfg.theme);
        let mut app = App {
            cfg,
            theme_idx,
            root: Node::empty(""),
            root_path: root_path.clone(),
            path: Vec::new(),
            sel: 0,
            scroll: 0,
            job: None,
            quit: false,
            help: false,
            modal: None,
            confirm: None,
            deleting: None,
            modal_area: Rect::default(),
            modal_rows: 1,
            status: String::new(),
            scan_time: None,
            started: Instant::now(),
            list_area: Rect::default(),
            map_hits: Vec::new(),
            last_click: None,
        };
        app.spawn(root_path, JobKind::Initial, None);
        app
    }

    pub fn theme(&self) -> &'static theme::Theme {
        &THEMES[self.theme_idx]
    }

    pub fn current(&self) -> &Node {
        cur(&self.root, &self.path)
    }

    /// Absolute filesystem path of the directory being shown.
    pub fn current_path(&self) -> PathBuf {
        let mut p = self.root_path.clone();
        let mut n = &self.root;
        for &i in &self.path {
            n = &n.children[i];
            p.push(&*n.name);
        }
        p
    }

    fn spawn(&mut self, target: PathBuf, kind: JobKind, skip: Option<Skip>) {
        let handle = scan::start(target.clone(), self.cfg.cross_filesystems, skip);
        self.job = Some(Job {
            handle,
            kind,
            target,
            began: Instant::now(),
            slots: Vec::new(),
        });
    }

    /// Apply scan messages and refresh the live view. Returns true when the view changed.
    pub fn poll_scan(&mut self) -> bool {
        let Some(mut job) = self.job.take() else {
            return false;
        };
        let mut finished = None;
        let mut installed = !job.slots.is_empty();
        while let Ok(msg) = job.handle.rx.try_recv() {
            match msg {
                Msg::Partial(entries) => {
                    self.install(&mut job, entries);
                    installed = true;
                }
                Msg::Done(id, node) => {
                    if let Some(pos) = job.slots.iter().position(|s| s.id == id) {
                        self.root.children[pos] = node;
                        job.slots[pos].live = None;
                    }
                }
                Msg::Finished(own) => {
                    finished = Some(own);
                    break;
                }
            }
        }
        if installed {
            let (names, sel_name) = self.capture();
            self.resort(&mut job, names, sel_name);
        }
        if let Some(own) = finished {
            self.root.size += own;
            self.scan_time = Some(job.began.elapsed());
        } else {
            self.job = Some(job);
        }
        installed
    }

    /// Names along the current path plus the selected entry's name.
    fn capture(&self) -> (Vec<Box<str>>, Option<Box<str>>) {
        let mut names = Vec::new();
        let mut n = &self.root;
        for &i in &self.path {
            n = &n.children[i];
            names.push(n.name.clone());
        }
        (names, n.children.get(self.sel).map(|c| c.name.clone()))
    }

    /// Replace the view with the freshly listed top-level entries of the scan target.
    fn install(&mut self, job: &mut Job, entries: Vec<Entry>) {
        let (names, mut sel_name) = match &job.kind {
            JobKind::Initial => (Vec::new(), None),
            JobKind::Rescan { names, sel_name } => (names.clone(), sel_name.clone()),
            JobKind::Up => {
                let n = self
                    .root_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned());
                (Vec::new(), n.map(Box::<str>::from))
            }
        };
        let old = std::mem::replace(&mut self.root, Node::empty(""));
        job.slots.clear();
        let mut children = Vec::with_capacity(entries.len());
        for (id, e) in entries.into_iter().enumerate() {
            job.slots.push(Slot { id, live: e.live });
            children.push(e.node);
        }
        if matches!(job.kind, JobKind::Up) {
            // Graft the already-scanned directory back in place of its placeholder.
            match sel_name
                .as_ref()
                .and_then(|n| children.iter().position(|c| c.is_dir && c.name == *n))
            {
                Some(pos) => {
                    let mut old = old;
                    old.name = children[pos].name.clone();
                    children[pos] = old;
                }
                None => sel_name = None,
            }
        }
        self.root = Node {
            name: job.target.to_string_lossy().into_owned().into(),
            size: 0,
            is_dir: true,
            count: 0,
            children,
        };
        self.root_path = job.target.clone();
        self.path.clear();
        self.scroll = 0;
        self.resort(job, names, sel_name);
    }

    /// Refresh sizes of unfinished directories, re-sort, and restore cursor/path by name.
    fn resort(&mut self, job: &mut Job, names: Vec<Box<str>>, sel_name: Option<Box<str>>) {
        for (c, s) in self.root.children.iter_mut().zip(&job.slots) {
            if let Some(l) = &s.live {
                c.size = l.load(Relaxed);
            }
        }
        self.root.size = self.root.children.iter().map(|c| c.size).sum();
        self.root.count = self.root.children.iter().map(|c| c.count).sum();

        let mut order: Vec<usize> = (0..job.slots.len()).collect();
        order.sort_by(|&a, &b| self.root.children[b].size.cmp(&self.root.children[a].size));
        if order.iter().enumerate().any(|(i, &o)| i != o) {
            let mut kids: Vec<Option<Node>> = std::mem::take(&mut self.root.children)
                .into_iter()
                .map(Some)
                .collect();
            let mut slots: Vec<Option<Slot>> = std::mem::take(&mut job.slots)
                .into_iter()
                .map(Some)
                .collect();
            self.root.children = order.iter().map(|&i| kids[i].take().unwrap()).collect();
            job.slots = order.iter().map(|&i| slots[i].take().unwrap()).collect();
        }

        self.restore(names, sel_name);
    }

    /// Re-resolve the shown directory and cursor from names after indices shifted.
    fn restore(&mut self, names: Vec<Box<str>>, sel_name: Option<Box<str>>) {
        self.path.clear();
        let mut n = &self.root;
        for name in &names {
            let Some(i) = n.children.iter().position(|c| c.is_dir && c.name == *name) else {
                break;
            };
            self.path.push(i);
            n = &n.children[i];
        }
        let last = n.children.len().saturating_sub(1);
        self.sel = match sel_name.and_then(|s| n.children.iter().position(|c| c.name == s)) {
            Some(i) => i,
            None => self.sel.min(last),
        };
    }

    pub fn deleting(&self) -> bool {
        self.deleting.is_some()
    }

    fn ask_delete(&mut self, permanent: bool) {
        if self.job.is_some() {
            self.status = "wait for the scan to finish before deleting".into();
            return;
        }
        if self.deleting.is_some() {
            return;
        }
        let Some(c) = self.current().children.get(self.sel) else {
            return;
        };
        self.confirm = Some(Confirm {
            path: self.current_path().join(&*c.name),
            name: c.name.clone(),
            size: c.size,
            count: c.count,
            is_dir: c.is_dir,
            permanent,
        });
    }

    fn start_delete(&mut self) {
        let Some(c) = self.confirm.take() else { return };
        let (tx, rx) = mpsc::channel();
        let target = c.path.clone();
        let permanent = c.permanent;
        thread::spawn(move || {
            let res = if permanent {
                // lstat: a symlink to a directory is unlinked, never followed.
                match fs::symlink_metadata(&target) {
                    Ok(m) if m.is_dir() => fs::remove_dir_all(&target),
                    Ok(_) => fs::remove_file(&target),
                    Err(e) => Err(e),
                }
            } else {
                trash_item(&target)
            };
            let _ = tx.send(res);
        });
        self.deleting = Some(Deleting {
            rx,
            path: c.path,
            name: c.name,
            permanent,
        });
    }

    /// Finish a background delete. Returns true when the view changed.
    pub fn poll_delete(&mut self) -> bool {
        let Some(d) = &self.deleting else {
            return false;
        };
        let Ok(res) = d.rx.try_recv() else {
            return false;
        };
        let d = self.deleting.take().unwrap();
        match res {
            Ok(()) => {
                self.remove_from_tree(&d.path);
                self.status = if d.permanent {
                    format!("deleted {}", d.name)
                } else {
                    format!("moved {} to the Trash", d.name)
                };
            }
            Err(e) => {
                self.status = format!(
                    "{} failed: {e}",
                    if d.permanent { "delete" } else { "trash" }
                );
                // A partial delete leaves the tree stale.
                if self.job.is_none() {
                    self.rescan();
                }
            }
        }
        true
    }

    fn remove_from_tree(&mut self, abs: &Path) {
        fn rec(n: &mut Node, comps: &[String]) -> Option<(u64, u64)> {
            let (first, rest) = comps.split_first()?;
            let idx = n.children.iter().position(|c| &*c.name == first.as_str())?;
            let (sz, cnt) = if rest.is_empty() {
                let r = n.children.remove(idx);
                (r.size, r.count)
            } else {
                rec(&mut n.children[idx], rest)?
            };
            n.size = n.size.saturating_sub(sz);
            n.count = n.count.saturating_sub(cnt);
            n.children.sort_by_key(|c| std::cmp::Reverse(c.size));
            Some((sz, cnt))
        }
        let Ok(rel) = abs.strip_prefix(&self.root_path) else {
            return;
        };
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let (names, sel_name) = self.capture();
        rec(&mut self.root, &comps);
        self.restore(names, sel_name);
    }

    /// Elapsed time of the running scan, or the duration of the last finished one.
    pub fn scan_elapsed(&self) -> Option<Duration> {
        self.job
            .as_ref()
            .map(|j| j.began.elapsed())
            .or(self.scan_time)
    }

    pub fn scanning(&self) -> bool {
        self.job.is_some()
    }

    pub fn scan_stats(&self) -> (u64, u64) {
        self.job.as_ref().map_or((0, 0), |j| {
            (
                j.handle.progress.files.load(Relaxed),
                j.handle.progress.bytes.load(Relaxed),
            )
        })
    }

    fn len(&self) -> usize {
        self.current().children.len()
    }

    fn move_sel(&mut self, delta: isize) {
        let max = self.len().saturating_sub(1) as isize;
        self.sel = (self.sel as isize + delta).clamp(0, max) as usize;
    }

    fn enter(&mut self) {
        let Some(child) = self.current().children.get(self.sel) else {
            return;
        };
        if child.is_dir {
            self.descend(&[self.sel]);
        }
    }

    /// Move into the directory reached by following `idxs` from the current one.
    /// If a scan is still running it is cancelled: the destination becomes the new
    /// scan root, reusing its subtree when that has already been fully scanned.
    fn descend(&mut self, idxs: &[usize]) {
        if idxs.is_empty() {
            return;
        }
        let Some(job) = &self.job else {
            self.path.extend_from_slice(idxs);
            self.sel = 0;
            self.scroll = 0;
            return;
        };
        let mut full = self.path.clone();
        full.extend_from_slice(idxs);
        // Everything under a finished top-level directory is complete.
        let done = job.slots.get(full[0]).is_some_and(|s| s.live.is_none());
        let mut target = self.root_path.clone();
        let mut n = &self.root;
        for &i in &full {
            n = &n.children[i];
            target.push(&*n.name);
        }
        self.job = None; // dropping the handle cancels the scan
        self.scan_time = None;
        self.path.clear();
        self.sel = 0;
        self.scroll = 0;
        if done {
            let mut n = &mut self.root;
            for &i in &full {
                n = &mut n.children[i];
            }
            let mut taken = std::mem::replace(n, Node::empty(""));
            taken.name = target.to_string_lossy().into_owned().into();
            self.root = taken;
            self.root_path = target;
        } else {
            self.root = Node::empty("");
            self.root_path = target.clone();
            self.spawn(target, JobKind::Initial, None);
        }
        self.status = "scan cancelled".into();
    }

    /// Open the folder at `trail` (child indices from the current dir). A file
    /// resolves to its parent folder.
    fn enter_trail(&mut self, trail: &[usize]) {
        let mut n = self.current();
        let mut dirs = Vec::new();
        for &i in trail {
            let Some(c) = n.children.get(i) else { break };
            if !c.is_dir {
                break;
            }
            dirs.push(i);
            n = c;
        }
        self.descend(&dirs);
    }

    fn up(&mut self) {
        if let Some(i) = self.path.pop() {
            self.sel = i;
            self.scroll = 0;
        } else if self.job.is_some() {
            self.status = "wait for the scan to finish before going up".into();
        } else {
            match self.root_path.parent() {
                Some(parent) => {
                    let skip = Skip {
                        name: self.root_path.file_name().unwrap_or_default().to_owned(),
                        size: self.root.size,
                        count: self.root.count,
                    };
                    self.spawn(parent.to_path_buf(), JobKind::Up, Some(skip));
                }
                None => self.status = "already at the filesystem root".into(),
            }
        }
    }

    fn rescan(&mut self) {
        let (names, sel_name) = self.capture();
        let kind = JobKind::Rescan { names, sel_name };
        self.spawn(self.root_path.clone(), kind, None);
    }

    fn open_modal(&mut self) {
        self.modal = Some(Modal {
            sel: self.theme_idx,
            orig: self.theme_idx,
        });
    }

    fn modal_move(&mut self, delta: isize) {
        if let Some(m) = &mut self.modal {
            m.sel = (m.sel as isize + delta).rem_euclid(THEMES.len() as isize) as usize;
            self.theme_idx = m.sel; // live preview
        }
    }

    fn modal_confirm(&mut self) {
        if let Some(m) = self.modal.take() {
            self.theme_idx = m.sel;
            self.cfg.theme = THEMES[m.sel].id.into();
            self.cfg.save();
        }
    }

    fn modal_cancel(&mut self) {
        if let Some(m) = self.modal.take() {
            self.theme_idx = m.orig;
        }
    }

    fn toggle_transparent(&mut self) {
        self.cfg.transparent = !self.cfg.transparent;
        self.cfg.save();
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        self.status.clear();
        if self.confirm.is_some() {
            match k.code {
                KeyCode::Char('y' | 'Y') => self.start_delete(),
                KeyCode::Char('n' | 'N' | 'q') | KeyCode::Esc => self.confirm = None,
                _ => {}
            }
            return;
        }
        if self.modal.is_some() {
            match k.code {
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => self.modal_move(1),
                KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => self.modal_move(-1),
                KeyCode::Enter | KeyCode::Char(' ') => self.modal_confirm(),
                KeyCode::Char('b') => self.toggle_transparent(),
                KeyCode::Esc | KeyCode::Char('q' | 't') => self.modal_cancel(),
                _ => {}
            }
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        let page = self.list_area.height.max(2) as isize - 1;
        match (k.code, k.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
            | (KeyCode::Char('q'), _)
            | (KeyCode::Esc, _) => self.quit = true,
            (KeyCode::Char('j') | KeyCode::Down, _) => self.move_sel(1),
            (KeyCode::Char('k') | KeyCode::Up, _) => self.move_sel(-1),
            (KeyCode::PageDown, _) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.move_sel(page)
            }
            (KeyCode::PageUp, _) | (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.move_sel(-page)
            }
            (KeyCode::Char('g') | KeyCode::Home, _) => self.sel = 0,
            (KeyCode::Char('G') | KeyCode::End, _) => self.sel = self.len().saturating_sub(1),
            (KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter, _) => self.enter(),
            (KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace, _) => self.up(),
            (KeyCode::Char('d'), KeyModifiers::NONE) => self.ask_delete(false),
            (KeyCode::Char('D'), _) => self.ask_delete(true),
            (KeyCode::Char('u'), _) => {
                let iec = !self.cfg.units.eq_ignore_ascii_case("iec");
                self.cfg.units = if iec { "iec" } else { "si" }.into();
                crate::ui::set_iec(iec);
                self.cfg.save();
                self.status = format!(
                    "units: {}",
                    if iec {
                        "KiB / MiB / GiB (1024)"
                    } else {
                        "kB / MB / GB (1000)"
                    }
                );
            }
            (KeyCode::Char('r'), _) => self.rescan(),
            (KeyCode::Char('t'), _) => self.open_modal(),
            (KeyCode::Char('b'), _) => self.toggle_transparent(),
            (KeyCode::Char('x'), _) => {
                self.cfg.cross_filesystems = !self.cfg.cross_filesystems;
                self.cfg.save();
                self.status = format!(
                    "other drives/mounts: {}",
                    if self.cfg.cross_filesystems {
                        "included"
                    } else {
                        "skipped"
                    }
                );
                self.rescan();
            }
            (KeyCode::Char('?'), _) => self.help = true,
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        if self.confirm.is_some() {
            return;
        }
        if self.modal.is_some() {
            let a = self.modal_area;
            match m.kind {
                MouseEventKind::ScrollDown => self.modal_move(1),
                MouseEventKind::ScrollUp => self.modal_move(-1),
                MouseEventKind::Down(MouseButton::Left) => {
                    let inside = m.column > a.x && m.column + 1 < a.x + a.width;
                    let row = (m.row.wrapping_sub(a.y + 1) / self.modal_rows.max(1)) as usize;
                    if inside && m.row > a.y && row < THEMES.len() {
                        if let Some(md) = &mut self.modal {
                            md.sel = row;
                        }
                        self.theme_idx = row;
                        self.modal_confirm();
                    } else if !a.contains((m.column, m.row).into()) {
                        self.modal_cancel();
                    }
                }
                _ => {}
            }
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.move_sel(1),
            MouseEventKind::ScrollUp => self.move_sel(-1),
            MouseEventKind::Down(MouseButton::Right) => self.up(),
            MouseEventKind::Down(MouseButton::Left) => {
                let (x, y) = (m.column, m.row);
                let la = self.list_area;
                let list_hit =
                    x >= la.x && x < la.x + la.width && y >= la.y && y < la.y + la.height;
                // Blocks are recorded outer-first, so the last match is the deepest.
                let deep: Vec<usize> = if list_hit {
                    Some(self.scroll + (y - la.y) as usize)
                        .filter(|&i| i < self.len())
                        .into_iter()
                        .collect()
                } else {
                    self.map_hits
                        .iter()
                        .rfind(|(c, _)| c.contains(x, y))
                        .map(|(_, t)| t.clone())
                        .unwrap_or_default()
                };
                let Some(&i) = deep.first() else { return };
                let double = matches!(self.last_click, Some((t, j)) if j == i && t.elapsed() < Duration::from_millis(400));
                self.sel = i;
                if double {
                    self.last_click = None;
                    self.enter_trail(&deep);
                } else {
                    self.last_click = Some((Instant::now(), i));
                }
            }
            _ => {}
        }
    }
}
