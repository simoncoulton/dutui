use rayon::prelude::*;
use std::{
    collections::HashSet,
    ffi::{CStr, CString, OsStr, OsString},
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        mpsc::{self, Receiver},
    },
    thread,
};

pub struct Node {
    pub name: Box<str>,
    /// Bytes allocated on disk (including all descendants).
    pub size: u64,
    pub is_dir: bool,
    /// Descendant file count.
    pub count: u64,
    /// Sorted by size, largest first.
    pub children: Vec<Node>,
}

impl Node {
    pub fn empty(name: &str) -> Self {
        Node { name: name.into(), size: 0, is_dir: true, count: 0, children: Vec::new() }
    }
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

pub enum Msg {
    /// Every top-level entry. Directories still being walked have `live` set.
    Partial(Vec<Entry>),
    /// A top-level directory (by index into the `Partial` list) finished.
    Done(usize, Node),
    /// Scan complete; carries the root directory's own on-disk size.
    Finished(u64),
}

pub struct Entry {
    pub node: Node,
    /// Running total of bytes found so far under an unfinished directory.
    pub live: Option<Arc<AtomicU64>>,
}

pub struct ScanHandle {
    pub progress: Arc<Progress>,
    pub rx: Receiver<Msg>,
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.progress.cancel.store(true, Relaxed);
    }
}

/// A child that is already scanned; a placeholder is emitted for it instead of rescanning.
pub struct Skip {
    pub name: OsString,
    pub size: u64,
    pub count: u64,
}

struct Ctx {
    dev: u64,
    cross: bool,
    seen: Mutex<HashSet<(u64, u64)>>,
    progress: Arc<Progress>,
}

pub fn start(path: PathBuf, cross: bool, skip: Option<Skip>) -> ScanHandle {
    let progress = Arc::new(Progress::default());
    let (tx, rx) = mpsc::channel();
    let p = progress.clone();
    thread::spawn(move || {
        // More threads than ~8 just fight over filesystem locks (measured slower on 12 cores).
        let threads = std::env::var("DUTUI_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| thread::available_parallelism().map_or(4, |n| n.get()).clamp(4, 8));
        let Ok(pool) = rayon::ThreadPoolBuilder::new().num_threads(threads).build() else {
            return;
        };
        let md = fs::symlink_metadata(&path);
        let ctx = Ctx {
            dev: md.as_ref().map_or(0, |m| m.dev()),
            cross,
            seen: Mutex::new(HashSet::new()),
            progress: p.clone(),
        };
        let own = md.as_ref().map_or(0, |m| m.blocks() * 512);
        pool.install(|| scan_top(&path, &ctx, skip.as_ref(), &tx, own));
    });
    ScanHandle { progress, rx }
}

struct Stat {
    dev: u64,
    ino: u64,
    nlink: u64,
    /// Bytes allocated on disk.
    size: u64,
    is_dir: bool,
}

struct Ent {
    name: Vec<u8>,
    st: Stat,
}

/// Portable fallback: list a directory and lstat every entry relative to the open directory handle.
/// Going through `fstatat` avoids re-resolving the full path for each entry, which
/// is what `DirEntry::metadata` does on macOS and is the dominant cost of a scan.
fn list_stat_slow(path: &Path) -> Vec<Ent> {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else { return Vec::new() };
    unsafe {
        let fd = libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC);
        if fd < 0 {
            return Vec::new();
        }
        let dp = libc::fdopendir(fd);
        if dp.is_null() {
            libc::close(fd);
            return Vec::new();
        }
        let mut names: Vec<CString> = Vec::new();
        loop {
            let ent = libc::readdir(dp);
            if ent.is_null() {
                break;
            }
            let n = CStr::from_ptr((*ent).d_name.as_ptr());
            if n.to_bytes() != b"." && n.to_bytes() != b".." {
                names.push(n.to_owned());
            }
        }
        let stat = |n: &CString| -> Option<Ent> {
            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstatat(fd, n.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) != 0 {
                return None;
            }
            Some(Ent {
                name: n.to_bytes().to_vec(),
                st: Stat {
                    dev: st.st_dev as u64,
                    ino: st.st_ino as u64,
                    nlink: st.st_nlink as u64,
                    size: st.st_blocks as u64 * 512,
                    is_dir: (st.st_mode & libc::S_IFMT) == libc::S_IFDIR,
                },
            })
        };
        let out = if names.len() > 1024 {
            names.par_iter().filter_map(stat).collect()
        } else {
            names.iter().filter_map(stat).collect()
        };
        libc::closedir(dp);
        out
    }
}

/// List a directory with sizes. On macOS, `getattrlistbulk` returns names, types and
/// allocated sizes for many entries per syscall, which is far cheaper than one stat per
/// file. Falls back to `fstatat` where it is unsupported.
fn list_stat(path: &Path) -> Vec<Ent> {
    #[cfg(target_os = "macos")]
    if let Some(v) = bulk::list(path) {
        return v;
    }
    list_stat_slow(path)
}

#[cfg(target_os = "macos")]
mod bulk {
    use super::{Ent, Stat};
    use std::{cell::RefCell, ffi::CString, os::unix::ffi::OsStrExt, path::Path};

    #[repr(C)]
    struct AttrList {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }

    unsafe extern "C" {
        fn getattrlistbulk(dirfd: libc::c_int, alist: *mut AttrList, buf: *mut libc::c_void, size: usize, options: u64) -> libc::c_int;
    }

    const ATTR_BIT_MAP_COUNT: u16 = 5;
    const ATTR_CMN_NAME: u32 = 0x0000_0001;
    const ATTR_CMN_DEVID: u32 = 0x0000_0002;
    const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
    const ATTR_CMN_FILEID: u32 = 0x0200_0000;
    const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
    const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
    const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
    const FSOPT_PACK_INVAL_ATTRS: u64 = 0x8;
    const VDIR: u32 = 2;

    thread_local! {
        static BUF: RefCell<Vec<u8>> = RefCell::new(vec![0; 256 * 1024]);
    }

    fn rd<T: Copy>(base: *const u8, off: usize) -> T {
        unsafe { std::ptr::read_unaligned(base.add(off) as *const T) }
    }

    /// `None` means bulk listing is unavailable here; the caller falls back.
    pub fn list(path: &Path) -> Option<Vec<Ent>> {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Some(Vec::new()); // unreadable directory: nothing to list
        }
        let mut al = AttrList {
            bitmapcount: ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_DEVID | ATTR_CMN_OBJTYPE | ATTR_CMN_FILEID,
            volattr: 0,
            dirattr: 0,
            fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE,
            forkattr: 0,
        };
        let mut out = Vec::new();
        let mut supported = true;
        BUF.with(|b| {
            let mut buf = b.borrow_mut();
            loop {
                let n = unsafe { getattrlistbulk(fd, &mut al, buf.as_mut_ptr().cast(), buf.len(), FSOPT_PACK_INVAL_ATTRS) };
                if n < 0 {
                    // Unsupported on this filesystem: let the caller use the portable path.
                    supported = !out.is_empty();
                    return;
                }
                if n == 0 {
                    return;
                }
                let mut p = buf.as_ptr();
                for _ in 0..n {
                    // Layout with FSOPT_PACK_INVAL_ATTRS: length, returned set (5 x u32),
                    // name ref, devid, objtype, fileid, linkcount, allocsize.
                    let len: u32 = rd(p, 0);
                    let name_off: i32 = rd(p, 24);
                    let name_len: u32 = rd(p, 28);
                    let dev: u32 = rd(p, 32);
                    let objtype: u32 = rd(p, 36);
                    let ino: u64 = rd(p, 40);
                    let nlink: u32 = rd(p, 48);
                    let alloc: i64 = rd(p, 52);
                    let name = unsafe {
                        std::slice::from_raw_parts(p.add(24).offset(name_off as isize), (name_len as usize).saturating_sub(1))
                    };
                    let is_dir = objtype == VDIR;
                    out.push(Ent {
                        name: name.to_vec(),
                        st: Stat { dev: dev as u64, ino, nlink: nlink as u64, size: if is_dir { 0 } else { alloc.max(0) as u64 }, is_dir },
                    });
                    p = unsafe { p.add(len as usize) };
                }
            }
        });
        unsafe { libc::close(fd) };
        if supported { Some(out) } else { None }
    }
}

fn lossy(name: &[u8]) -> Box<str> {
    String::from_utf8_lossy(name).into_owned().into()
}

/// Scan the root directory in two phases so the UI can show something immediately:
/// first stat every top-level entry, then walk each sub-directory in parallel.
fn scan_top(path: &Path, ctx: &Ctx, skip: Option<&Skip>, tx: &mpsc::Sender<Msg>, own: u64) {
    let mut entries = Vec::new();
    let mut todo = Vec::new();
    for e in list_stat(path) {
        if let Some(s) = skip {
            if e.name == s.name.as_bytes() {
                let node = Node { name: lossy(&e.name), size: s.size, is_dir: true, count: s.count, children: Vec::new() };
                entries.push(Entry { node, live: None });
                continue;
            }
        }
        let name = lossy(&e.name);
        if e.st.is_dir {
            if !ctx.cross && e.st.dev != ctx.dev {
                continue;
            }
            let live = Arc::new(AtomicU64::new(e.st.size));
            todo.push((entries.len(), path.join(OsStr::from_bytes(&e.name)), e.st.size, live.clone()));
            let node = Node { name, size: e.st.size, is_dir: true, count: 0, children: Vec::new() };
            entries.push(Entry { node, live: Some(live) });
        } else {
            if e.st.nlink > 1 && !ctx.seen.lock().unwrap().insert((e.st.dev, e.st.ino)) {
                continue;
            }
            ctx.progress.files.fetch_add(1, Relaxed);
            ctx.progress.bytes.fetch_add(e.st.size, Relaxed);
            let node = Node { name, size: e.st.size, is_dir: false, count: 1, children: Vec::new() };
            entries.push(Entry { node, live: None });
        }
    }
    if tx.send(Msg::Partial(entries)).is_err() {
        return;
    }
    todo.par_iter().for_each(|(i, p, own, live)| {
        let children = read_children(p, ctx, live);
        if ctx.progress.cancel.load(Relaxed) {
            return;
        }
        let name = lossy(p.file_name().unwrap_or_default().as_bytes());
        let node = Node {
            name,
            size: own + children.iter().map(|c| c.size).sum::<u64>(),
            is_dir: true,
            count: children.iter().map(|c| c.count).sum(),
            children,
        };
        let _ = tx.send(Msg::Done(*i, node));
    });
    if !ctx.progress.cancel.load(Relaxed) {
        let _ = tx.send(Msg::Finished(own));
    }
}

fn read_children(path: &Path, ctx: &Ctx, live: &AtomicU64) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut dirs = Vec::new();
    for e in list_stat(path) {
        if e.st.is_dir {
            if ctx.cross || e.st.dev == ctx.dev {
                dirs.push(e);
            }
        } else {
            // Count hard-linked files once.
            if e.st.nlink > 1 && !ctx.seen.lock().unwrap().insert((e.st.dev, e.st.ino)) {
                continue;
            }
            live.fetch_add(e.st.size, Relaxed);
            ctx.progress.files.fetch_add(1, Relaxed);
            ctx.progress.bytes.fetch_add(e.st.size, Relaxed);
            nodes.push(Node { name: lossy(&e.name), size: e.st.size, is_dir: false, count: 1, children: Vec::new() });
        }
    }
    let visit = |e: &Ent| -> Option<Node> {
        if ctx.progress.cancel.load(Relaxed) {
            return None;
        }
        live.fetch_add(e.st.size, Relaxed);
        let children = read_children(&path.join(OsStr::from_bytes(&e.name)), ctx, live);
        Some(Node {
            name: lossy(&e.name),
            size: e.st.size + children.iter().map(|c| c.size).sum::<u64>(),
            is_dir: true,
            count: children.iter().map(|c| c.count).sum(),
            children,
        })
    };
    if dirs.len() > 1 {
        nodes.par_extend(dirs.par_iter().filter_map(visit));
    } else {
        nodes.extend(dirs.iter().filter_map(visit));
    }
    nodes.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    nodes
}
