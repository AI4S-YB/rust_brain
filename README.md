# RustBrain

A desktop transcriptomics analysis platform built entirely in Rust. Integrates RNA-seq tools behind a unified UI with interactive visualizations and project-based data management.

**Tech Stack:** Rust + Tauri v2 + WebView + ECharts

## Features

- **AI analysis mode (Phase 1)** — create a project in AI mode and drive analyses through natural-language chat. The copilot proposes plans, you approve or edit them, and AI-initiated runs share the same run history as the manual UI. Works with any OpenAI-compatible endpoint (OpenAI / DeepSeek / Moonshot / Qwen / vLLM / Ollama `/v1/*`).
- **QC Analysis** — Powered by [fastqc-rs](https://github.com/AI4S-YB/fastqc-rs), 2.1-4.7x faster than Java FastQC
- **Adapter Trimming** — Powered by [cutadapt-rs](https://github.com/AI4S-YB/cutadapt-rs), byte-identical output to Python cutadapt
- **GFF Conversion** — Powered by [gffread_rs](https://github.com/AI4S-YB/gffread_rs), GFF3↔GTF conversion so annotations from any source feed straight into STAR
- **Alignment & Quantification** — Powered by [STAR_rs](https://github.com/AI4S-YB/STAR_rs), splice-aware alignment with per-gene read counting; auto-merges per-sample `ReadsPerGene.out.tab` into a DESeq2-ready counts matrix
- **Differential Expression** — Powered by [DESeq2_rs](https://github.com/AI4S-YB/DESeq2_rs), 28x faster than R DESeq2 with 99.6% accuracy
- **Project Management** — Create/open projects with isolated work directories, full run history
- **Interactive Visualization** — ECharts-based volcano plots, MA plots, quality scores, heatmaps
- **Custom Plotting** — User-defined scatter/bar/box/histogram charts from result data
- **Result Export** — Charts as PNG, tables as TSV
- **Cross-Platform** — Windows, macOS, Linux desktop builds via Tauri

## Architecture

```
rust_brain/
├── crates/
│   ├── rb-core/          # Module trait, Project model, async Runner
│   ├── rb-app/           # Tauri v2 desktop app (11 commands)
│   ├── rb-qc/            # fastqc-rs adapter
│   ├── rb-trimming/      # cutadapt-rs adapter
│   ├── rb-gff-convert/   # gffread-rs adapter (GFF3↔GTF)
│   ├── rb-star-index/    # STAR_rs genome indexing adapter
│   ├── rb-star-align/    # STAR_rs alignment + counts matrix merge
│   ├── rb-deseq2/        # DESeq2_rs adapter
│   └── rb-ai/            # AI orchestration (provider adapters, tool registry, chat session persistence, orchestrator main loop)
├── frontend/             # Vanilla HTML/CSS/JS + ECharts
└── deps/                 # Tool submodules
```

All analysis modules implement a unified `Module` trait:

```rust
#[async_trait]
pub trait Module: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError>;
    async fn run(&self, params: &Value, project_dir: &Path, progress_tx: Sender<Progress>)
        -> Result<ModuleResult, ModuleError>;
}
```

## Getting Started

### Prerequisites

- Rust 1.75+
- Tauri CLI: `cargo install tauri-cli --locked`
- Linux: `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev`

### Clone

```bash
git clone --recurse-submodules https://github.com/AI4S-YB/rust_brain.git
cd rust_brain
```

### Development

```bash
# Run desktop app (hot-reload)
cd crates/rb-app && cargo tauri dev

# Frontend-only preview (no backend)
cd frontend && python3 -m http.server 8090

# Run tests
cargo test --workspace

# Check + lint
cargo check --workspace
cargo clippy --workspace
```

### Build

```bash
cd crates/rb-app && cargo tauri build
```

Outputs: `.deb` / `.AppImage` (Linux), `.dmg` (macOS), `.msi` (Windows)

## Analysis Pipeline

```
Raw Reads → QC → Trimming → [GFF Convert] → Alignment → Quantification → DESeq2 → Enrichment
             ✅      ✅           ✅             ✅              ✅           ✅
```

## TODO

### Near-term

- [ ] Wire real analysis results to frontend charts (replace mock data)
- [ ] Parse fastqc-rs structured output (per-module pass/warn/fail, quality scores)
- [ ] Parse cutadapt-rs trimming statistics from output
- [ ] Add progress reporting inside each adapter (currently only start/end)
- [ ] Persist recent projects list to disk (currently in-memory only)
- [ ] Add proper error dialogs in frontend (replace `alert()`)

### Module Integration

- [ ] **WGCNA** — Integrate [WGCNA_rs](https://github.com/AI4S-YB/WGCNA_rs) (co-expression network analysis)
- [ ] **Enrichment Analysis** — Implement GO/KEGG enrichment module in Rust

### Pipeline & Workflow

- [ ] Pipeline orchestration — chain modules (QC → Trim → Align → Quant → DESeq2)
- [ ] Auto-connect upstream outputs as downstream inputs
- [ ] Breakpoint resume for interrupted pipelines
- [ ] Batch sample processing

### Visualization

- [ ] Gene-level detail view (click gene in volcano plot → show expression across samples)
- [ ] Heatmap with hierarchical clustering
- [ ] PCA / sample distance plot
- [ ] Brush-linked views (select genes in volcano → highlight in table and other plots)
- [ ] Custom plot: populate column dropdowns from actual result data

### Infrastructure

- [ ] Resolve cutadapt-core workspace dep inheritance (currently uses subprocess fallback)
- [ ] Auto-update via Tauri updater plugin
- [ ] i18n (Chinese / English)
- [ ] App icon and branding
- [ ] User settings persistence (save/load AppConfig)
- [ ] Logging to file (not just in-memory)

## STAR_rs dependency

`rb-star-index` and `rb-star-align` invoke the `star` binary from
https://github.com/AI4S-YB/STAR_rs.

**Released builds (`.deb` / `.AppImage` / `.dmg` / `.msi`):** `star` is bundled
with the app — no separate install needed. The build pipeline downloads the
matching prebuilt binary from STAR_rs releases and ships it under the app's
resource directory.

**Developing locally (`cargo tauri dev`):** The repo keeps
`crates/rb-app/binaries/` empty, so dev builds fall back to `$PATH` (or a user
override set in the Settings view). Install STAR_rs one of two ways:

- Grab a prebuilt binary from
  https://github.com/AI4S-YB/STAR_rs/releases and drop it on `$PATH`, e.g.:

        curl -sL https://github.com/AI4S-YB/STAR_rs/releases/download/v0.2.2/star-v0.2.2-x86_64-unknown-linux-gnu.tar.gz \
          | tar xz -C ~/.local/bin

- Or build from source:

        git clone https://github.com/AI4S-YB/STAR_rs.git
        cd STAR_rs && cargo build --release

**Resolution order at runtime:** user-configured Settings path → app-bundled
sidecar → `$PATH`. A Settings override always wins, so you can point dev or
installed builds at a custom `star` build without uninstalling.

## cutadapt-rs dependency

Similarly, `rb-trimming` invokes the `cutadapt-rs` binary from
https://github.com/AI4S-YB/cutadapt-rs. Same discovery mechanism: PATH or
Settings-configured path.

## gffread_rs dependency

`rb-gff-convert` invokes the `gffread-rs` binary from
https://github.com/AI4S-YB/gffread_rs.

**Released builds:** bundled automatically — no separate install needed.

**Local development:** grab a prebuilt binary:

    curl -sL https://github.com/AI4S-YB/gffread_rs/releases/download/v0.1.0/gffread-rs-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
      | tar xz -C ~/.local/bin

or build from source:

    git clone https://github.com/AI4S-YB/gffread_rs.git
    cd gffread_rs && cargo build -p gffread-rs --release

## License

MIT
