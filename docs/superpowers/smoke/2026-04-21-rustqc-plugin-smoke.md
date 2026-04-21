# RustQC Plugin — Smoke Checklist

Manual end-to-end test for the third-party tool plugin system, shipped via the
RustQC bundled manifest.

## Prerequisites

- A built RustQC binary (Linux/macOS) downloaded from
  https://seqeralabs.github.io/RustQC/, somewhere readable.
- A small `.fastq.gz` sample (any short FASTQ works for the smoke pass).

If you don't have a RustQC binary handy, you can substitute `/bin/echo` to
exercise the wiring (steps 1-6 will pass; the run itself will exit 0 with
no real outputs).

## Steps

1. **Boot:** `cd crates/rb-app && cargo tauri dev` — the app launches with
   no console errors.
2. **Sidebar:** A new "Plugins" section appears at the bottom of the
   sidebar with a `RustQC` entry showing the plug icon badge.
3. **Missing binary:** Click `RustQC` — the view shows the
   "needs a binary path" guidance card with an "Open Settings" button.
4. **Settings → Binaries:** The binaries table now lists `RustQC` alongside
   `star`, `cutadapt-rs`, `gffread-rs`. Click "Browse" and select your
   RustQC binary path.
5. **Settings → Plugins:** Bundled section lists `rustqc`. No load errors
   reported.
6. **Re-open RustQC view:** The parameter form renders — drop zone for
   input files, threads number input, nogroup checkbox, format select,
   output dir text, extra args text.
7. **Run:** Drop a FASTQ file, click "Run". A toast confirms run started
   and the log panel streams stderr lines.
8. **Result:** When the run completes, the runs panel shows the run
   with a generic plugin result view: status, output dir, output file
   list, and an expandable "Command" details block.
9. **AI integration (optional):** Open Chat. Ask the assistant to "use
   RustQC on this file" — it should invoke the auto-derived `run_rustqc`
   tool with the right `input_files` argument.
10. **Reload plugins:** Drop a custom `.toml` into
    `<config_dir>/rust_brain/plugins/`, open Settings → Plugins, click
    "Reload plugins". The plugin appears in the User section.

## Common issues

- Plugin not in sidebar → check `cargo build -p rb-app` succeeded
  (the manifest is embedded at compile time via `include_dir!`).
- Manifest parse error in Settings → Plugins → Errors → check the source
  label and message; fix the TOML and click "Reload plugins".
- Run fails with "BinaryError::NotFound" → revisit Settings → Binaries
  and confirm the path is set and executable.
