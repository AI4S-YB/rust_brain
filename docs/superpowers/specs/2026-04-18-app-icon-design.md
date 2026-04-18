# RustBrain App Icon — Design Spec

**Date:** 2026-04-18
**Status:** Approved

## Problem

The Tauri desktop app ships with 32×32 placeholder icons (`crates/rb-app/icons/icon.png`, `icon.ico` — ~100 bytes each). The bundled app has no recognizable identity in taskbars, docks, installers, or `.desktop` entries. macOS and installer targets lack the platform-specific variants they expect.

## Goal

A single custom-designed icon, delivered in every size Tauri's bundler targets, that:
- Is unambiguously an "R" at 16×16 (taskbar size).
- Feels like a natural extension of the existing "Warm Botanical Lab" brand (teal primary, Zilla Slab display, warm cream surfaces).
- Rewards closer inspection at larger sizes with a subtle bio/brain metaphor.

## Concept

A sculpted capital **R** rooted in Zilla Slab, knocked out in warm cream against a solid teal squircle. The counter (bowl) of the R is shaped as a single cerebral gyrus fold. One fold is traced by a coral hairline — the "unforgettable detail," reading as capillary / neural trace / fluorescent marker depending on the viewer.

## Visual Specification

### Container
- Squircle (superellipse, n ≈ 4), corner radius ≈ 22% of tile edge.
- Fill: `#0d7377` (brand `--accent`).
- Top-edge inner highlight: 1px stroke, white at 3% opacity (subtle dimension, no skeuomorphism).
- No outer stroke, no baked-in drop shadow (OS applies its own).

### R Glyph
- Derived from Zilla Slab Bold, custom-drawn for small-size legibility.
- Stroke weight ~12% heavier than stock Zilla Bold so counters don't close up at 16×16.
- Squared, slightly flared bracket terminals (slab-serif character).
- Fill: `#f8f5f0` (brand `--bg-page`, warm cream).
- Occupies ~68% of tile diagonal (safe area preserved for OS masks).
- Leg kicks outward at 28° with a gentle inward curve — subtle forward lean.

### Brain-Fold Counter
- The enclosed counter is an S-curve, not a plain oval — divides the counter into two lobes resembling a cerebral gyrus.
- Rendered in negative space (teal shows through).
- Reads as a stylized counter at 16×16; the fold metaphor emerges at 64×64+.

### Coral Hairline
- 1.5px stroke in `#c9503c` (brand `--mod-coral`).
- Traces the inner edge of one brain-fold lobe.
- Collapses into fold edge at 16×16 (invisible); distinctive at 32×32+.

### Corner Mark (≥128×128 only)
- 4-dot constellation in the lower-right corner, cream at 40% opacity.
- Echoes well-plate / microarray grid motif.
- Suppressed at smaller sizes to keep the icon clean.

## Deliverables

| File | Purpose |
|------|---------|
| `crates/rb-app/icons/icon.svg` | Master SVG, 512×512 viewBox (source of truth) |
| `crates/rb-app/icons/icon.png` | 512×512 PNG (Tauri bundle primary) |
| `crates/rb-app/icons/icon.ico` | Multi-res ICO (16, 32, 48, 64, 128, 256) |
| `crates/rb-app/icons/icon.icns` | macOS bundle icon (16–1024, @1x + @2x) |
| `crates/rb-app/icons/32x32.png` | Tauri standard size |
| `crates/rb-app/icons/128x128.png` | Tauri standard size |
| `crates/rb-app/icons/128x128@2x.png` | Tauri standard size (retina) |

`tauri.conf.json` → `bundle.icon` updated to reference all platform variants.

## Acceptance Criteria

1. At 16×16 the icon is unambiguously an "R" (squint test passes).
2. At 512×512 the brain-fold counter and coral hairline are both visible and legible.
3. `cargo tauri build` succeeds with the new icons wired in.
4. SVG master uses only brand palette variables (`#0d7377`, `#f8f5f0`, `#c9503c`) — no outside colors introduced.
5. The generated `.ico` contains at least the 16/32/48/256 sub-images.

## Out of Scope

- Animated / motion variants (splash screen, loading state).
- Alternative marks (favicon for a web landing page, social share image, `.desktop` category icon variants).
- Dark-mode variant — the solid teal tile reads well on both light and dark desktops; revisit only if a user raises a contrast issue.
- Frontend in-app logo treatments (sidebar header, about dialog) — scoped separately.
