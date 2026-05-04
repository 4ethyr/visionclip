# VisionClip Search Subsystem

This is the first integrated slice of the Spotlight/Finder-like search system. The search runtime lives inside `visionclip-daemon`; `visionclip` and the GTK overlay are lightweight clients over the existing Unix socket and bincode IPC.

## Current Scope

- `crates/search` provides the local search core.
- SQLite stores roots, file metadata, chunks, jobs, usage counters and audit data.
- The first index pass catalogs filename, path, extension, mime, kind, size and timestamps, prioritizing application `.desktop` roots before large user directories.
- TXT, Markdown, PDF text via `pdftotext`, and desktop-entry metadata are extracted into SQLite chunks for `search`/`grep` until Tantivy lands.
- Startup indexing runs in a background task with a separate SQLite/WAL connection so the first CLI/UI query does not hold the daemon mutex during a filesystem crawl. It catalogs all metadata first and extracts only cheap text sources (`.desktop`, TXT/CSV/log/Markdown); full PDF text extraction is kept for explicit rebuild/background phases.
- The initial query path supports `locate`, `search`, `grep` mode contracts and filters such as `kind:`, `ext:`, `path:` and `source:`.
- Security policy skips secret directories, secret-like filenames, dev build outputs and symlink escapes.
- Tantivy, notify-rs, sqlite-vec, OCR and semantic search are scaffolded as follow-up phases, not enabled in this slice.

## CLI

```bash
visionclip locate docker-compose.yml
visionclip search "architecture notes kind:document"
visionclip search --semantic "database architecture"
visionclip search --hybrid "invoice from last month"
visionclip grep "auth middleware" ./src

visionclip index status
visionclip index add ~/Projects
visionclip index remove ~/Downloads
visionclip index rebuild
visionclip index audit
visionclip index pause
visionclip index resume
```

Every search command accepts `--json`.

## GTK Overlay

The base overlay is available when the CLI is built with GTK support:

```bash
cargo run -p visionclip --features gtk-overlay -- --search-overlay
```

The installed GNOME shortcut for the overlay is `Alt+Space` by default. It is configurable through `[ui.search_overlay].shortcut` or `VISIONCLIP_SEARCH_OVERLAY_SHORTCUT` during installation.

The current overlay implements the Liquid Crystal shell: frameless GTK window, custom Cairo-drawn glass layers, 82px command input, animated AI processing bar, keyboard Escape close, outside-click/deactivation close, daemon-backed search/open actions and configurable colors/effects through `[ui.search_overlay]`. GTK does not expose browser `backdrop-filter`, SVG `feDisplacementMap`, or real-time lensing/refraction APIs over the compositor background, so VisionClip approximates the material with a transparent window, near-zero tint, animated caustic highlights, chromatic edge glow, lens borders and subtle internal depth.

The results region is intentionally bounded. The `ListBox` sits inside a `ScrolledWindow` with a fixed maximum content height, hidden overflow and styled overlay scrollbar so long result sets stay clipped inside the rounded glass panel instead of leaking over the terminal or dock.

Supported visual presets mirror the Aether CSS design families:

```text
liquid_crystal, liquid_glass, liquid_glass_advanced, aurora_gel, crystal_mist,
fluid_amber, frost_lens, ice_ripple, mercury_drop, molten_glass,
nebula_prism, ocean_wave, plasma_flow, prisma_flow, silk_veil, glass, glassmorphism,
frosted, bright_overlay, dark_overlay, dark_glass, high_contrast, vibrant,
desaturated, monochrome, vintage, inverted, color_shifted, animated_glass,
accessible_glass, neumorphism, neumorphic_pressed, neumorphic_concave,
neumorphic_colored, neumorphic_accessible
```

Main tuning knobs:

```toml
[ui.search_overlay]
glass_style = "liquid_crystal"
blur_radius_px = 32
panel_opacity = 0.04
corner_radius_px = 28
border_opacity = 0.30
shadow_intensity = 0.28
highlight_intensity = 0.42
saturation = 1.18
contrast = 1.06
brightness = 1.00
refraction_strength = 0.86
chromatic_aberration = 0.28
liquid_noise = 0.52
```

## Security Defaults

Search skips sensitive roots such as `~/.ssh`, `~/.gnupg`, browser profiles, cloud credentials, keyrings, caches and trash. It also excludes `.env`, key/certificate files, token/secret/password/credential filenames, SQLite databases and dev build directories such as `node_modules`, `target`, `vendor`, virtualenvs, `.git`, `dist` and `build`.

Local search content is treated as sensitive. Chunks and embeddings must not be logged, and cloud providers must not receive local file content unless a later explicit permission flow allows it.

## Next Phases

1. Add Tantivy indexing for body text, snippets and richer lexical ranking.
2. Add durable background jobs and progress reporting for large extractors.
3. Add notify-rs/inotify watcher with debounce, overflow handling and incremental rescan.
4. Wire GTK overlay debounce to daemon search responses and keyboard actions.
5. Add sqlite-vec semantic mode behind `semantic_index = true`.
6. Add OCR and multimodal indexing after the text path is stable.
