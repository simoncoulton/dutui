mod app;
mod config;
mod scan;
mod theme;
mod treemap;
mod ui;

use app::App;
use ratatui::crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
};
use std::sync::atomic::Ordering::Relaxed;
use std::{io, path::PathBuf, time::Duration};

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let arg = args.next();
    if arg.as_deref() == Some("--bench") {
        // Scan without the UI and report timing; used to profile the scanner.
        let path = args
            .next()
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let t = std::time::Instant::now();
        let h = scan::start(path, false, None);
        while let Ok(m) = h.rx.recv() {
            if matches!(m, scan::Msg::Finished(_)) {
                break;
            }
        }
        let (f, b) = (
            h.progress.files.load(Relaxed),
            h.progress.bytes.load(Relaxed),
        );
        println!("{:.2?}  {f} files  {b} bytes", t.elapsed());
        return Ok(());
    }
    if matches!(arg.as_deref(), Some("-V" | "--version")) {
        println!("dutui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if matches!(arg.as_deref(), Some("-h" | "--help")) {
        println!(
            "dutui [PATH]\n\nDisk usage treemap. Scans PATH (default: current directory).\nConfig: ~/.config/dutui/dutui.yml"
        );
        return Ok(());
    }
    let start = match arg {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir()?,
    };
    let start = start.canonicalize().unwrap_or(start);
    if !start.is_dir() {
        eprintln!("dutui: {} is not a directory", start.display());
        std::process::exit(1);
    }

    let mut app = App::new(config::Config::load(), start);
    let mut term = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let res = run(&mut term, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    res
}

fn run(term: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut dirty = true;
    while !app.quit {
        dirty |= app.poll_scan();
        dirty |= app.poll_delete();
        if dirty || app.scanning() || app.deleting() {
            term.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }
        let wait = if app.scanning() || app.deleting() {
            Duration::from_millis(60)
        } else {
            Duration::from_millis(500)
        };
        if app.edit_config {
            app.edit_config = false;
            edit_config(term, app)?;
            dirty = true;
        }
        if event::poll(wait)? {
            // Drain everything queued so held keys / fast scrolling don't lag.
            loop {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    _ => {}
                }
                dirty = true;
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Hand the terminal to `$VISUAL` / `$EDITOR` on the config file, then reload it.
fn edit_config(term: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let Some(path) = config::config_path() else {
        app.status = "no config path (HOME is unset)".into();
        return Ok(());
    };
    if !path.exists() {
        app.cfg.save();
    }
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "vi".into());
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    // Through the shell so editors set with arguments (e.g. "code -w") work.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("dutui")
        .arg(&path)
        .status();
    *term = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    match status {
        Ok(s) if s.success() => match std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_yaml_ng::from_str(&t).ok())
        {
            Some(cfg) => app.apply_config(cfg),
            None => app.status = "config is not valid YAML; keeping current settings".into(),
        },
        Ok(_) => app.status = "editor exited with an error; config not reloaded".into(),
        Err(e) => app.status = format!("could not run '{editor}': {e}"),
    }
    Ok(())
}
