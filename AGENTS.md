# AGENTS.md

Guidance for AI agents (and people) changing dutui. Read this before touching
the UI, keys, themes, config or the scanner. The README is the user-facing doc;
this file is about how the code is meant to be shaped.

## What this is

A terminal disk-usage browser in the spirit of DaisyDisk: `ratatui` + `crossterm`
UI, a parallel scanner, a squarified treemap. Performance is the main feature.

| Module | Role |
| --- | --- |
| `main.rs` | CLI flags, terminal setup, the event loop, hand-off to `$EDITOR`. |
| `app.rs` | UI state and **all key handling**; owns the scan and delete jobs. |
| `ui.rs` | Rendering. Reads `App`; the only writes are hit-test rects (`map_hits`, `list_area`, `modal_area`) that mouse handling needs. |
| `scan.rs` | The scanner: two phases, progressive results, cancel-on-drop. |
| `treemap.rs` | Pure squarified layout in terminal cells. |
| `theme.rs` | Built-in presets, theme files, name matching. The only file with colour literals. |
| `config.rs` | YAML config, comment-preserving edits, validation. |

Data flow: key -> `App::on_key` -> state change (or a job is spawned) ->
`poll_scan` / `poll_delete` drain channels each tick -> `ui::draw`. Never block
in `app.rs` or `ui.rs`; filesystem work happens on scan/delete threads.

## Screen layout

```
┌ dutui ───────────────────────────────────────┐  header: 3 rows, bordered
│ /path           28.5 GB · 604,946 files · ◷ 1.9 s │  left: directory; right: size, file count, time
└──────────────────────────────────────────────┘
┌ Files ──────────┐┌ name · size · % ─────────┐
│ list            ││ treemap (nested, 3 deep) │   body: left list, right treemap
└─────────────────┘└──────────────────────────┘
 j/k move  l open  h up  d trash ...             footer: 1 row, no border
```

- The **footer** shows scan/delete progress, else a transient **status
  message**, else the **key hints**. The footer is deliberately short: less-used
  keys (theme, transparent, units, config editor) live in the help popup only.
  **Do not add footer hints for new keys unless asked**; add them to help and
  the README.
- Popups are drawn last, centred, via `popup()`, which clears the area and
  paints the page style so the background is right in opaque and transparent
  modes.
- **Nothing may panic at any terminal size, including 0x0.** `put()` clips to
  the buffer; use `saturating_sub`, and `.max(1)` / `.min(..)` instead of
  `clamp(min, max)` when `max` can fall below `min` (it panics). The tiny
  terminal test must keep passing.

## Keybinds

- `App::on_key` is the single place for keys. Modals (`confirm`, `modal`,
  `help`) are checked first and swallow input.
- Lower-case for everyday actions (`d` trash, `r` rescan, `t` theme, `u`
  units); **capital for the heavier or rarer variant** (`D` permanent delete,
  `E` edit config). `j k g G` and arrows navigate, `Esc` closes, `?` is help,
  `q` quits.
- Destructive actions ask first; only `y` confirms, anything else cancels.
  Permanent deletion has its own, more explicit (red) dialog.
- When you add or change a key, update **all** of: `on_key`, the rows in
  `ui::draw_help`, the README key table, and the tests. Check the key is free
  (`grep "KeyCode::Char('x')" src/app.rs`).

## Colour rules

- **No colour literals outside `theme.rs`** (`Color::Rgb`, named colours). Use the
  fields on `Theme`: `fg`, `muted`, `accent`, `border`, `selection_bg`, `success`,
  `info`, `warning`, `error`, `special`, `bg`, plus the treemap's derived `palette`
  and `dark`. Background goes through `Theme::page_bg(transparent)`; never paint
  your own. Highlight the selected row with `Theme::selection()` (it falls back to
  reverse video when `selection_bg` is `Reset`).
- `accent` = titles, **key hints** and modal borders. Keys are bold accent, their
  descriptions `dim`. Use `hint_line(th, "Enter save  Esc cancel")` (items split
  on two spaces, first word is the key) and `key_style(th)`; do not hand-style
  hints.
- Treemap blocks take colours from `palette`; text drawn on them uses `dark`.

## Theming

- Themes are YAML files in `~/.config/dutui/themes/`. On start each built-in
  without a file is written out with every colour spelled out (a deleted
  built-in comes back); any other `.yml` there is a new theme named after the
  file. A file may set `base: <built-in>`; without it, it inherits from the
  built-in of the same name, else from `default`. Unknown keys, bad colours and
  unknown bases are errors naming the file.
- The set is the same as torrentui's. Keep the preset table in `theme.rs` in
  step with it rather than inventing colours.
- Themes are discovered dynamically: `App::reload_themes` re-reads the directory
  when the picker opens and when the config is reloaded after `E`.
- Match names with `theme::position` / `normalize` (case, accents and punctuation
  ignored; `catppuccin` means Mocha for old configs). Never compare names with `==`.
- Adding a built-in: a `preset(...)` entry in `builtin_themes`, its page colour in
  `preset_bg`, the README list, and the lookup test.
- The picker previews live and scrolls when there are more themes than rows; `Esc`
  restores, `Enter` saves, `b` toggles transparency.

## Config

- `~/.config/dutui/dutui.yml` (also on macOS). Keys: `theme`, `transparent`,
  `cross_filesystems`, `units`. Unknown keys, unknown themes and bad `units` are
  **errors** naming the file (the theme name is checked against the loaded
  themes); startup exits with the message and the `E` reload keeps the old
  settings and shows it. Never fall back silently.
- **Edit in place, never rewrite.** `Config::save` replaces only the
  `key: value` lines, so the user's comments survive; it refuses to touch a
  file that is already broken and refuses to write anything that would not
  parse back identically.
- Adding a key: `Config`, `Default`, `DEFAULT_CONFIG` (commented), `apply_to`,
  the README, and the `default_template_equals_defaults` test.

## Scanner and performance

- Size is **allocated** bytes (`st_blocks * 512`), hard links counted once per
  `(dev, ino)`, symlinks never followed, other filesystems skipped unless
  `cross_filesystems`. Changing any of this changes every number users compare
  against `du` and `dua`; do it deliberately.
- macOS uses `getattrlistbulk` (`scan::bulk`) with a portable `fstatat` fallback.
  Per-file `lstat` is the bottleneck; do not reintroduce it on the hot path.
  Benchmark with `dutui --bench PATH` before and after, and check the file and
  byte totals did not move.
- Threads default to 4..=8 (`DUTUI_THREADS` overrides): more contends on
  filesystem locks and gets slower.
- Dropping a `ScanHandle` cancels its scan. Anything that navigates or rescans
  must drop the old job first.

## Tests and checks

CI runs `cargo fmt --all --check`, `cargo clippy --locked --all-targets
-- -D warnings` and `cargo test --locked` on Linux and macOS, plus an MSRV build.
Run all three.

- UI tests render with `TestBackend` and assert on text and cell colours. Keep
  these invariants tested: popups match the page background in both modes, modal
  hints use `accent`, nothing panics on tiny terminals, rare keys stay out of the
  footer.
- **Tests must never touch the real config, home or filesystem outside
  `std::env::temp_dir()`.** Anything that calls `Config::save` (keys `u`, `b`, `x`,
  theme confirm) writes `$HOME/.config`; test the pure parts (`apply_to`) instead.
- `ui::set_iec` is a process-wide global; tests that depend on it hold the
  `UNITS` mutex.
- Test deletes only on throwaway data.
- When verifying in a real terminal, use an isolated `HOME`.

## Style

Match the surrounding code: naming, comment density, error handling. Comments
explain *why*, briefly. Do not reformat unrelated code (`cargo fmt`, then check
the diff is only yours). Update the README for anything user-visible, and keep
this file true when the layout, keys or theming rules change.
