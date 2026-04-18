/* ============================================================
   RustBrain — Transcriptomics Analysis Platform
   Frontend Application (Warm Light Theme)
   ============================================================ */

(function () {
  'use strict';

  // ── Configuration ──────────────────────────────────────────
  const MODULES = [
    { id: 'qc',             name: 'QC Analysis',          icon: 'microscope',  color: 'teal',   tool: 'fastqc-rs',    status: 'ready' },
    { id: 'trimming',       name: 'Adapter Trimming',     icon: 'scissors',    color: 'blue',   tool: 'cutadapt-rs',  status: 'ready' },
    { id: 'alignment',      name: 'Read Alignment',       icon: 'git-merge',   color: 'purple', tool: 'HISAT2',       status: 'soon' },
    { id: 'quantification', name: 'Quantification',       icon: 'bar-chart-3', color: 'gold',   tool: 'StringTie',    status: 'soon' },
    { id: 'differential',   name: 'Differential Expr.',   icon: 'flame',       color: 'coral',  tool: 'DESeq2_rs',    status: 'ready' },
    { id: 'network',        name: 'Network Analysis',     icon: 'share-2',     color: 'green',  tool: 'WGCNA_rs',     status: 'ready' },
    { id: 'enrichment',     name: 'Enrichment',           icon: 'target',      color: 'slate',  tool: 'TBD',          status: 'soon' },
  ];

  const COLOR_MAP = {
    teal:   '#0d7377',
    blue:   '#3b6ea5',
    purple: '#7c5cbf',
    gold:   '#b8860b',
    coral:  '#c9503c',
    green:  '#2d8659',
    slate:  '#5c7080',
  };

  // ── ECharts theme / helpers ────────────────────────────────
  const ECHART_THEME = {
    backgroundColor: '#faf8f4',
    textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
    title: { textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' } },
    grid: { left: 60, right: 24, top: 44, bottom: 50 },
    toolbox: {
      feature: {
        saveAsImage: { title: 'Save PNG', pixelRatio: 2 },
        dataZoom: { title: { zoom: 'Zoom', back: 'Reset' } },
      },
      right: 20, top: 10,
    },
  };

  function createChart(container) {
    return echarts.init(container, null, { renderer: 'canvas' });
  }

  // ── State ──────────────────────────────────────────────────
  const state = {
    currentView: 'dashboard',
    files: {},
    pipelineStatus: {},
    projectOpen: false,
    projectName: '',
  };
  MODULES.forEach(m => {
    state.files[m.id] = [];
    state.pipelineStatus[m.id] = 'idle';
  });

  // ── Tauri API ──────────────────────────────────────────────
  const api = {
    invoke(command, args) { return window.__TAURI__.core.invoke(command, args); },
    listen(event, callback) { return window.__TAURI__.event.listen(event, callback); }
  };

  // ── TSV export ─────────────────────────────────────────────
  function exportTableAsTSV(tableId, filename) {
    const table = document.getElementById(tableId);
    if (!table) return;
    const rows = Array.from(table.querySelectorAll('tr'));
    const tsv = rows.map(row =>
      Array.from(row.querySelectorAll('th, td')).map(cell => cell.textContent.trim()).join('\t')
    ).join('\n');
    const blob = new Blob([tsv], { type: 'text/tab-separated-values' });
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob);
    a.download = filename || 'export.tsv'; a.click(); URL.revokeObjectURL(a.href);
  }
  window.exportTableAsTSV = exportTableAsTSV;

  // ── Router ─────────────────────────────────────────────────
  function navigate(view) {
    state.currentView = view;

    document.querySelectorAll('.nav-item').forEach(el => {
      el.classList.toggle('active', el.dataset.view === view);
    });

    const bc = document.getElementById('breadcrumb');
    const label = view === 'dashboard' ? 'Dashboard'
      : view === 'settings' ? 'Settings'
      : view === 'star-index' ? 'STAR Index'
      : view === 'star-align' ? 'STAR Alignment'
      : MODULES.find(m => m.id === view)?.name || view;
    bc.innerHTML = `
      <span class="breadcrumb-home">RustBrain</span>
      <i data-lucide="chevron-right" class="breadcrumb-sep"></i>
      <span class="breadcrumb-current">${label}</span>
    `;

    const content = document.getElementById('content');
    content.scrollTop = 0;

    if (view === 'dashboard') content.innerHTML = renderDashboard();
    else if (view === 'settings') {
      content.innerHTML = '<h2>Settings — Binary Paths</h2><p>Loading…</p>';
      renderSettings().then(html => {
        const root = document.getElementById('content');
        if (root && state.currentView === 'settings') root.innerHTML = html;
        if (window.lucide) lucide.createIcons();
      });
    }
    else if (view === 'star-index') content.innerHTML = renderStarIndex();
    else content.innerHTML = renderModule(view);

    if (window.lucide) lucide.createIcons();
    requestAnimationFrame(() => initChartsForView(view));
  }


  // ── Dashboard ──────────────────────────────────────────────
  function renderDashboard() {
    const pipelineNodes = MODULES.map((m, i) => {
      const connector = i < MODULES.length - 1
        ? '<div class="pipeline-connector"><div class="pipeline-connector-line"></div></div>'
        : '';
      return `
        <div class="pipeline-stage animate-slide-up" style="animation-delay: ${i * 60}ms">
          <div class="pipeline-node ${m.status}" data-view="${m.id}" style="--node-color: ${COLOR_MAP[m.color]}">
            <div class="pipeline-node-icon"><i data-lucide="${m.icon}"></i></div>
            <div class="pipeline-node-title">${m.name}</div>
            <div class="pipeline-node-desc">${m.tool}</div>
            <div class="pipeline-node-status">
              <span class="dot"></span>
              ${m.status === 'ready' ? 'Available' : 'Coming Soon'}
            </div>
          </div>
          ${connector}
        </div>`;
    }).join('');

    const projName = state.projectOpen ? state.projectName : 'No project open';
    const projStatus = state.projectOpen
      ? `<span class="badge badge-green" style="margin-left:8px">Open</span>`
      : `<span class="badge badge-muted" style="margin-left:8px">Closed</span>`;

    return `
      <div class="module-view">
        <div class="dashboard-hero animate-slide-up">
          <h1 class="dashboard-title">
            <span class="dashboard-title-accent">Transcriptomics</span> Pipeline
          </h1>
          <p class="dashboard-subtitle">
            End-to-end RNA-seq analysis powered by Rust. From raw reads to biological insights.
          </p>
        </div>

        <div class="card animate-slide-up" style="animation-delay: 40ms; margin-bottom: 16px; padding: 16px 24px;">
          <div class="card-header" style="margin-bottom: 12px">
            <span class="card-title"><i data-lucide="folder-open" style="width:15px;height:15px;vertical-align:-2px;margin-right:6px"></i>Project</span>
            ${projStatus}
          </div>
          <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;">
            <span style="font-size:0.9rem;color:var(--text-secondary);flex:1;min-width:120px;" id="dash-proj-name">${projName}</span>
            <button class="btn btn-secondary btn-sm" onclick="projectNew()"><i data-lucide="folder-plus"></i> New Project</button>
            <button class="btn btn-secondary btn-sm" onclick="projectOpen()"><i data-lucide="folder-open"></i> Open Project</button>
          </div>
        </div>

        <div class="pipeline-flow-container card animate-slide-up" style="animation-delay: 60ms; padding: 16px 24px;">
          <div class="card-header" style="margin-bottom: 8px">
            <span class="card-title">Analysis Pipeline</span>
            <span class="badge badge-teal">7 modules</span>
          </div>
          <div class="pipeline-flow stagger">
            ${pipelineNodes}
          </div>
        </div>

        <div class="stats-row stagger">
          <div class="stat-card animate-slide-up">
            <div class="stat-label">Modules Ready</div>
            <div class="stat-value">4<span class="stat-unit">/ 7</span></div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">Rust Tools</div>
            <div class="stat-value">4</div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">Active Jobs</div>
            <div class="stat-value">0</div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">Speed Gain</div>
            <div class="stat-value">28<span class="stat-unit">x</span></div>
          </div>
        </div>

        <div class="dashboard-grid">
          <div class="card animate-slide-up" style="animation-delay: 200ms">
            <div class="card-header">
              <span class="card-title">Quick Start</span>
            </div>
            <div class="quick-actions">
              ${renderQuickAction('qc', 'microscope', 'teal', 'Run QC', 'FastQC quality analysis')}
              ${renderQuickAction('trimming', 'scissors', 'blue', 'Trim Reads', 'Adapter removal')}
              ${renderQuickAction('differential', 'flame', 'coral', 'DESeq2 Analysis', 'Differential expression')}
              ${renderQuickAction('network', 'share-2', 'green', 'WGCNA', 'Co-expression network')}
            </div>
          </div>

          <div class="card animate-slide-up" style="animation-delay: 260ms">
            <div class="card-header">
              <span class="card-title">Rust Tool Suite</span>
            </div>
            <div>
              ${renderToolInfo('fastqc-rs', '2.1-4.7x faster than Java FastQC', 'GPL-3.0')}
              ${renderToolInfo('cutadapt-rs', 'Byte-identical to Python cutadapt', 'MIT')}
              ${renderToolInfo('DESeq2_rs', '28x faster, 99.6% accuracy vs R', 'MIT')}
              ${renderToolInfo('WGCNA_rs', 'Bit-exact co-expression analysis', 'GPL-2.0')}
            </div>
          </div>
        </div>
      </div>`;
  }

  function renderQuickAction(view, icon, color, title, desc) {
    const hex = COLOR_MAP[color];
    return `
      <div class="quick-action" data-view="${view}">
        <div class="quick-action-icon" style="background: ${hex}12; color: ${hex};">
          <i data-lucide="${icon}"></i>
        </div>
        <div>
          <div class="quick-action-text">${title}</div>
          <div class="quick-action-desc">${desc}</div>
        </div>
      </div>`;
  }

  function renderToolInfo(name, desc, license) {
    return `
      <div class="tool-info">
        <span class="tool-info-name">${name}</span>
        <span class="tool-info-desc">${desc}</span>
        <span class="badge badge-teal">${license}</span>
      </div>`;
  }

  // ── Project management helpers ─────────────────────────────
  window.projectNew = async function () {
    const name = prompt('Enter project name:');
    if (!name) return;
    try {
      const dir = await api.invoke('select_directory', {});
      await api.invoke('create_project', { name, directory: dir });
      state.projectOpen = true;
      state.projectName = name;
      document.getElementById('projectName').textContent = name;
    } catch (err) {
      console.warn('[projectNew] invoke failed, using local fallback:', err);
      state.projectOpen = true;
      state.projectName = name;
      document.getElementById('projectName').textContent = name;
    }
    const el = document.getElementById('dash-proj-name');
    if (el) el.textContent = name;
  };

  window.projectOpen = async function () {
    try {
      const dir = await api.invoke('select_directory', {});
      const result = await api.invoke('open_project', { directory: dir });
      const name = (result && result.name) ? result.name : dir || 'Opened Project';
      state.projectOpen = true;
      state.projectName = name;
      document.getElementById('projectName').textContent = name;
      const el = document.getElementById('dash-proj-name');
      if (el) el.textContent = name;
    } catch (err) {
      console.warn('[projectOpen] invoke failed:', err);
    }
  };


  // ── Module View ────────────────────────────────────────────
  function renderModule(moduleId) {
    const mod = MODULES.find(m => m.id === moduleId);
    if (!mod) return renderEmptyState('Module not found');

    const hex = COLOR_MAP[mod.color];
    const header = `
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: ${hex}12; color: ${hex};">
          <i data-lucide="${mod.icon}"></i>
        </div>
        <div>
          <h1 class="module-title">${mod.name}</h1>
          <p class="module-desc">Powered by <strong style="color: ${hex}">${mod.tool}</strong></p>
          <div class="module-badges">
            <span class="badge badge-${mod.color}">${mod.status === 'ready' ? 'Available' : 'Coming Soon'}</span>
          </div>
        </div>
      </div>`;

    if (mod.status === 'soon') {
      return `<div class="module-view">${header}${renderComingSoon(mod)}</div>`;
    }

    let content = '';
    switch (moduleId) {
      case 'qc':           content = renderQC(mod); break;
      case 'trimming':     content = renderTrimming(mod); break;
      case 'differential': content = renderDifferential(mod); break;
      case 'network':      content = renderNetwork(mod); break;
      default:             content = renderComingSoon(mod); break;
    }

    return `<div class="module-view">${header}${content}</div>`;
  }


  // ── QC Module ──────────────────────────────────────────────
  function renderQC() {
    return `
      <div class="module-layout">
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:100ms">
            <div class="panel-header">
              <span class="panel-title">Input Files</span>
              <span class="badge badge-teal">${state.files.qc.length} files</span>
            </div>
            <div class="panel-body">
              <div class="file-drop-zone" data-module="qc" data-accept=".fastq,.fq,.fastq.gz,.fq.gz,.bam,.sam">
                <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
                <div class="file-drop-text">Drop FASTQ / BAM files here</div>
                <div class="file-drop-hint">Supports .fastq, .fq, .fastq.gz, .bam, .sam</div>
              </div>
              <div class="file-list" id="qc-file-list"></div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">Parameters</span></div>
            <div class="panel-body">
              <div class="form-row">
                <div class="form-group">
                  <label class="form-label">Threads</label>
                  <input type="number" class="form-input" id="qc-threads" value="4" min="1" max="32">
                </div>
                <div class="form-group">
                  <label class="form-label">Format</label>
                  <select class="form-select" id="qc-format">
                    <option>Auto-detect</option><option>FASTQ</option><option>BAM</option><option>SAM</option>
                  </select>
                </div>
              </div>
              <div class="form-group">
                <label class="form-label">Output Directory</label>
                <input type="text" class="form-input" id="qc-output" placeholder="/path/to/output">
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">
                  Advanced Options <i data-lucide="chevron-down"></i>
                </button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-casava"> CASAVA mode</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-nogroup"> Disable base grouping</label></div>
                  <div class="form-group"><label class="form-label">K-mer Size</label><input type="number" class="form-input" id="qc-kmer" value="7" min="2" max="10"></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('qc')"><i data-lucide="rotate-ccw"></i> Reset</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('qc')"><i data-lucide="play"></i> Run QC</button>
            </div>
            ${renderLogPanel('qc')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">Results</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="qc-chart">Quality Scores</div>
                <div class="tab" data-tab="qc-summary">Summary</div>
                <div class="tab" data-tab="qc-log">Log</div>
              </div>
              <div class="tab-content active" data-tab="qc-chart">
                <div class="chart-container" id="qc-quality-chart" style="height:320px;"></div>
              </div>
              <div class="tab-content" data-tab="qc-summary">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">Pass</div><div class="result-metric-label">Overall</div></div>
                  <div class="result-metric"><div class="result-metric-value">35.2</div><div class="result-metric-label">Mean Quality</div></div>
                  <div class="result-metric"><div class="result-metric-value">12.4M</div><div class="result-metric-label">Total Reads</div></div>
                  <div class="result-metric"><div class="result-metric-value">150</div><div class="result-metric-label">Read Length</div></div>
                </div>
                <table class="data-table"><thead><tr><th>Module</th><th>Status</th></tr></thead><tbody>
                  <tr><td>Per base sequence quality</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Per sequence quality scores</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Per base sequence content</td><td><span class="badge badge-gold">WARN</span></td></tr>
                  <tr><td>Per sequence GC content</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Per base N content</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Sequence length distribution</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Sequence duplication levels</td><td><span class="badge badge-gold">WARN</span></td></tr>
                  <tr><td>Overrepresented sequences</td><td><span class="badge badge-green">PASS</span></td></tr>
                  <tr><td>Adapter content</td><td><span class="badge badge-green">PASS</span></td></tr>
                </tbody></table>
              </div>
              <div class="tab-content" data-tab="qc-log">
                <div class="log-output"><span class="log-info">[INFO]</span> fastqc-rs v0.12.1
<span class="log-info">[INFO]</span> Processing sample_R1.fastq.gz...
<span class="log-info">[INFO]</span> Threads: 4, Format: auto-detect
<span class="log-success">[DONE]</span> Analysis complete: 12,432,891 reads
<span class="log-info">[INFO]</span> Output written to ./fastqc_output/</div>
              </div>
            </div>
          </div>
        </div>
      </div>`;
  }


  // ── Trimming Module ────────────────────────────────────────
  function renderTrimming() {
    return `
      <div class="module-layout">
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:100ms">
            <div class="panel-header"><span class="panel-title">Input Files</span></div>
            <div class="panel-body">
              <div class="file-drop-zone" data-module="trimming" data-accept=".fastq,.fq,.fastq.gz,.fq.gz">
                <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
                <div class="file-drop-text">Drop FASTQ / FASTA files here</div>
                <div class="file-drop-hint">Supports .fastq, .fq, .fasta (plain or gzipped)</div>
              </div>
              <div class="file-list" id="trimming-file-list"></div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">Adapter Settings</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">Adapter Preset</label>
                <select class="form-select" id="trim-preset">
                  <option>Illumina Universal (AGATCGGAAGAGC)</option>
                  <option>Nextera Transposase</option>
                  <option>Illumina Small RNA</option>
                  <option>BGIseq</option>
                  <option>Custom Sequence</option>
                </select>
              </div>
              <div class="form-group">
                <label class="form-label">3' Adapter (-a)</label>
                <input type="text" class="form-input" id="trim-adapter" value="AGATCGGAAGAGC" placeholder="AGATCGGAAGAGC">
                <span class="form-hint">Sequence to trim from 3' end</span>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">Quality Cutoff (-q)</label><input type="number" class="form-input" id="trim-quality" value="20" min="0" max="42"></div>
                <div class="form-group"><label class="form-label">Min Length (-m)</label><input type="number" class="form-input" id="trim-minlen" value="20" min="1"></div>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">Max N Bases</label><input type="number" class="form-input" id="trim-maxn" value="-1"><span class="form-hint">-1 = no limit</span></div>
                <div class="form-group"><label class="form-label">Threads</label><input type="number" class="form-input" id="trim-threads" value="4" min="1" max="16"></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">Paired-End Options <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-paired"> Paired-end mode</label></div>
                  <div class="form-group"><label class="form-label">R2 Adapter (-A)</label><input type="text" class="form-input" id="trim-adapter2" placeholder="AGATCGGAAGAGC"></div>
                </div></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">Trim Galore Mode <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-galore"> Enable Trim Galore wrapper</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-fastqc"> Run FastQC after trimming</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-rrbs"> RRBS mode</label></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('trimming')"><i data-lucide="rotate-ccw"></i> Reset</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('trimming')" style="background:var(--mod-blue);border-color:var(--mod-blue)"><i data-lucide="play"></i> Run Trimming</button>
            </div>
            ${renderLogPanel('trimming')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">Trimming Results</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="trim-stats">Statistics</div>
                <div class="tab" data-tab="trim-chart">Length Distribution</div>
                <div class="tab" data-tab="trim-log">Log</div>
              </div>
              <div class="tab-content active" data-tab="trim-stats">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">10.2M</div><div class="result-metric-label">Reads Processed</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">98.7%</div><div class="result-metric-label">Reads Passing</div></div>
                  <div class="result-metric"><div class="result-metric-value">4.3%</div><div class="result-metric-label">Adapter Found</div></div>
                  <div class="result-metric"><div class="result-metric-value">142</div><div class="result-metric-label">Mean Length</div></div>
                </div>
                <table class="data-table"><thead><tr><th>Metric</th><th>Value</th></tr></thead><tbody>
                  <tr><td>Total reads processed</td><td>10,243,891</td></tr>
                  <tr><td>Reads with adapters</td><td>438,215 (4.3%)</td></tr>
                  <tr><td>Reads too short</td><td>132,045 (1.3%)</td></tr>
                  <tr><td>Reads passing filters</td><td>10,111,846 (98.7%)</td></tr>
                  <tr><td>Base pairs processed</td><td>1,536,583,650</td></tr>
                  <tr><td>Quality-trimmed</td><td>12,456,789 bp (0.8%)</td></tr>
                  <tr><td>Total written</td><td>1,435,822,132 bp (93.4%)</td></tr>
                </tbody></table>
              </div>
              <div class="tab-content" data-tab="trim-chart">
                <div class="chart-container" id="trim-length-chart" style="height:320px;"></div>
              </div>
              <div class="tab-content" data-tab="trim-log">
                <div class="log-output"><span class="log-info">[INFO]</span> cutadapt-rs v0.1.0
<span class="log-info">[INFO]</span> Adapter: AGATCGGAAGAGC (3' regular)
<span class="log-info">[INFO]</span> Quality cutoff: 20, Min length: 20
<span class="log-info">[INFO]</span> Processing with 4 threads...
<span class="log-success">[DONE]</span> 10,243,891 reads processed in 48.2s
<span class="log-info">[INFO]</span> Output: trimmed_R1.fastq.gz</div>
              </div>
            </div>
          </div>
        </div>
      </div>`;
  }


  // ── STAR Index ────────────────────────────────────────────
  function renderStarIndex() {
    return `
    <h2>STAR Genome Index</h2>
    <p>Build a STAR index from a reference genome FASTA and GTF annotation. Required before any alignment run.</p>
    <form id="form-star-index">
      <label>Genome FASTA
        <input type="text" name="genome_fasta" data-pick="file" placeholder="/path/to/genome.fa" required />
        <button type="button" data-pick-for="genome_fasta">Browse…</button>
      </label>
      <label>GTF annotation
        <input type="text" name="gtf_file" data-pick="file" placeholder="/path/to/annotation.gtf" required />
        <button type="button" data-pick-for="gtf_file">Browse…</button>
      </label>
      <label>Threads <input type="number" name="threads" value="4" min="1" /></label>
      <label>sjdbOverhang <input type="number" name="sjdb_overhang" value="100" min="1" /></label>
      <label>genomeSAindexNbases <input type="number" name="genome_sa_index_nbases" value="14" min="1" max="18" /></label>
      <details><summary>Advanced</summary>
        <label>Extra args (one per line)
          <textarea name="extra_args" placeholder="--limitGenomeGenerateRAM 31000000000"></textarea>
        </label>
      </details>
      <button type="submit">Build Index</button>
    </form>
    <div id="star-index-runs"></div>
  `;
  }

  async function submitStarIndex(form) {
    const fd = new FormData(form);
    const extra_args = (fd.get('extra_args') || '').toString().split('\n').map(s => s.trim()).filter(Boolean);
    const params = {
      genome_fasta: fd.get('genome_fasta'),
      gtf_file:     fd.get('gtf_file'),
      threads:      parseInt(fd.get('threads'), 10) || 4,
      sjdb_overhang: parseInt(fd.get('sjdb_overhang'), 10) || 100,
      genome_sa_index_nbases: parseInt(fd.get('genome_sa_index_nbases'), 10) || 14,
      extra_args,
    };
    try {
      const runId = await window.__TAURI__.core.invoke('run_module', { moduleId: 'star_index', params });
      navigate('star-index');
    } catch (err) { alert('Failed to start run: ' + err); }
  }


  // ── Differential Expression ────────────────────────────────
  function renderDifferential() {
    return `
      <div class="module-layout">
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:100ms">
            <div class="panel-header"><span class="panel-title">Input Data</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">Count Matrix (TSV)</label>
                <div class="file-drop-zone" data-module="differential" data-accept=".tsv,.csv,.txt" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="table"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">Drop counts matrix file</div>
                  <div class="file-drop-hint">Genes in rows, samples in columns (TSV)</div>
                </div>
              </div>
              <div class="form-group">
                <label class="form-label">Sample Information (TSV)</label>
                <div class="file-drop-zone" data-module="differential-coldata" data-accept=".tsv,.csv,.txt" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">Drop coldata / sample info</div>
                  <div class="file-drop-hint">Sample names, conditions, covariates</div>
                </div>
              </div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">DESeq2 Parameters</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">Design Variable</label>
                <input type="text" class="form-input" id="deseq-design" value="condition" placeholder="e.g. condition, treatment">
                <span class="form-hint">Column in sample info for comparison</span>
              </div>
              <div class="form-group">
                <label class="form-label">Reference Level</label>
                <input type="text" class="form-input" id="deseq-ref" value="control" placeholder="e.g. control, untreated">
                <span class="form-hint">Baseline for fold-change calculation</span>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">padj Cutoff</label><input type="number" class="form-input" id="deseq-padj" value="0.01" step="0.01" min="0" max="1"></div>
                <div class="form-group"><label class="form-label">|log2FC| Cutoff</label><input type="number" class="form-input" id="deseq-lfc" value="1.0" step="0.1" min="0"></div>
              </div>
              <div class="form-group">
                <label class="form-label">Output File</label>
                <input type="text" class="form-input" id="deseq-output" value="deseq2_results.tsv" placeholder="results.tsv">
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('differential')"><i data-lucide="rotate-ccw"></i> Reset</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('differential')" style="background:var(--mod-coral);border-color:var(--mod-coral)"><i data-lucide="play"></i> Run DESeq2</button>
            </div>
            ${renderLogPanel('differential')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">Analysis Results</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="deseq-volcano">Volcano Plot</div>
                <div class="tab" data-tab="deseq-ma">MA Plot</div>
                <div class="tab" data-tab="deseq-table">Results Table</div>
                <div class="tab" data-tab="deseq-custom">Custom Plot</div>
                <div class="tab" data-tab="deseq-log">Log</div>
              </div>
              <div class="tab-content active" data-tab="deseq-volcano">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">64,102</div><div class="result-metric-label">Total Genes</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-coral)">347</div><div class="result-metric-label">Up-regulated</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-blue)">325</div><div class="result-metric-label">Down-regulated</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-teal)">672</div><div class="result-metric-label">Significant</div></div>
                </div>
                <div class="chart-container" id="deseq-volcano-chart" style="height:380px;"></div>
              </div>
              <div class="tab-content" data-tab="deseq-ma">
                <div class="chart-container" id="deseq-ma-chart" style="height:380px;"></div>
              </div>
              <div class="tab-content" data-tab="deseq-table">
                <div style="display:flex;justify-content:flex-end;margin-bottom:8px;">
                  <button class="btn btn-ghost btn-sm" onclick="exportTableAsTSV('deseq-results-table', 'deseq2_results.tsv')">Export TSV</button>
                </div>
                <div style="max-height:400px;overflow-y:auto;">
                  <table class="data-table" id="deseq-results-table"><thead><tr><th>Gene</th><th>log2FC</th><th>p-value</th><th>padj</th></tr></thead><tbody></tbody></table>
                </div>
              </div>
              <div class="tab-content" data-tab="deseq-custom">
                ${renderCustomPlotPanel('differential')}
              </div>
              <div class="tab-content" data-tab="deseq-log">
                <div class="log-output"><span class="log-info">[INFO]</span> DESeq2_rs v0.1.0
<span class="log-info">[INFO]</span> Counts: 64,102 genes x 8 samples
<span class="log-info">[INFO]</span> Design: ~condition, Reference: control
<span class="log-info">[INFO]</span> Estimating size factors...
<span class="log-info">[INFO]</span> Estimating dispersions...
<span class="log-info">[INFO]</span> Fitting NB GLM (IRLS)...
<span class="log-info">[INFO]</span> Wald test + BH adjustment...
<span class="log-success">[DONE]</span> 672 significant genes (padj &lt; 0.01, |log2FC| &gt; 1)
<span class="log-info">[INFO]</span> Output: deseq2_results.tsv</div>
              </div>
            </div>
          </div>
        </div>
      </div>`;
  }


  // ── Network Module ─────────────────────────────────────────
  function renderNetwork() {
    return `
      <div class="module-layout">
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:100ms">
            <div class="panel-header"><span class="panel-title">Input Data</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">Expression Matrix (CSV)</label>
                <div class="file-drop-zone" data-module="network" data-accept=".csv,.tsv" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="grid-3x3"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">Drop expression matrix</div>
                  <div class="file-drop-hint">Samples in rows, genes in columns</div>
                </div>
              </div>
              <div class="form-group">
                <label class="form-label">Trait Data (optional)</label>
                <div class="file-drop-zone" data-module="network-trait" data-accept=".csv,.tsv" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">Drop trait data</div>
                  <div class="file-drop-hint">For module-trait association</div>
                </div>
              </div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">WGCNA Parameters</span></div>
            <div class="panel-body">
              <div class="form-group"><label class="form-label">Correlation Method</label>
                <select class="form-select" id="wgcna-corr"><option>Pearson</option><option>Biweight Midcorrelation</option></select></div>
              <div class="form-group"><label class="form-label">Network Type</label>
                <select class="form-select" id="wgcna-nettype"><option>Signed</option><option>Unsigned</option><option>Signed Hybrid</option></select></div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">Soft Threshold</label><input type="number" class="form-input" id="wgcna-thresh" value="6" min="1" max="30"><span class="form-hint">Use threshold picker</span></div>
                <div class="form-group"><label class="form-label">Min Module Size</label><input type="number" class="form-input" id="wgcna-minmod" value="30" min="10"></div>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">Merge Cut Height</label><input type="number" class="form-input" id="wgcna-mergecut" value="0.25" step="0.05" min="0" max="1"></div>
                <div class="form-group"><label class="form-label">TOM Type</label>
                  <select class="form-select" id="wgcna-tom"><option>Signed</option><option>Unsigned</option></select></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">Advanced Options <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-pam"> PAM refinement</label></div>
                  <div class="form-group"><label class="form-label">Deep Split</label>
                    <select class="form-select" id="wgcna-deepsplit"><option value="0">0</option><option value="1">1</option><option value="2" selected>2 (default)</option><option value="3">3</option><option value="4">4</option></select></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-cytoscape"> Export Cytoscape network</label></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm"><i data-lucide="zap"></i> Pick Threshold</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('network')" style="background:var(--mod-green);border-color:var(--mod-green)"><i data-lucide="play"></i> Run WGCNA</button>
            </div>
            ${renderLogPanel('network')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">Network Results</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="wgcna-modules">Modules</div>
                <div class="tab" data-tab="wgcna-trait">Trait Heatmap</div>
                <div class="tab" data-tab="wgcna-custom">Custom Plot</div>
                <div class="tab" data-tab="wgcna-log">Log</div>
              </div>
              <div class="tab-content active" data-tab="wgcna-modules">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">5,000</div><div class="result-metric-label">Genes</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">12</div><div class="result-metric-label">Modules</div></div>
                  <div class="result-metric"><div class="result-metric-value">R²=0.87</div><div class="result-metric-label">Scale-Free Fit</div></div>
                </div>
                <div class="chart-container" id="wgcna-module-chart" style="height:320px;"></div>
              </div>
              <div class="tab-content" data-tab="wgcna-trait">
                <div class="chart-container" id="wgcna-trait-chart" style="height:380px;"></div>
              </div>
              <div class="tab-content" data-tab="wgcna-custom">
                ${renderCustomPlotPanel('network')}
              </div>
              <div class="tab-content" data-tab="wgcna-log">
                <div class="log-output"><span class="log-info">[INFO]</span> WGCNA_rs v0.1.0
<span class="log-info">[INFO]</span> Matrix: 50 samples x 5,000 genes
<span class="log-info">[INFO]</span> Correlation: Pearson, Network: Signed
<span class="log-info">[INFO]</span> Soft threshold = 6 (R² = 0.87)
<span class="log-info">[INFO]</span> Computing TOM...
<span class="log-info">[INFO]</span> Hierarchical clustering (NN-chain)...
<span class="log-info">[INFO]</span> Dynamic tree cutting...
<span class="log-info">[INFO]</span> Merging modules (cutHeight=0.25)...
<span class="log-success">[DONE]</span> 12 modules identified
<span class="log-info">[INFO]</span> Output: module_assignments.tsv</div>
              </div>
            </div>
          </div>
        </div>
      </div>`;
  }


  // ── Custom Plot Panel ──────────────────────────────────────
  function renderCustomPlotPanel(moduleId) {
    const axisOptions = moduleId === 'differential'
      ? ['log2FC', 'baseMean', '-log10(padj)', 'pvalue', 'padj']
      : ['module_size', 'kME', 'connectivity', 'trait_correlation'];
    const opts = axisOptions.map(o => `<option value="${o}">${o}</option>`).join('');
    return `
      <div style="padding:12px 0;">
        <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:flex-end;margin-bottom:12px;">
          <div class="form-group" style="margin-bottom:0;min-width:120px;">
            <label class="form-label">X Axis</label>
            <select class="form-select" id="${moduleId}-custom-x">${opts}</select>
          </div>
          <div class="form-group" style="margin-bottom:0;min-width:120px;">
            <label class="form-label">Y Axis</label>
            <select class="form-select" id="${moduleId}-custom-y">${opts.replace('selected', '').replace(axisOptions[0], axisOptions[Math.min(1, axisOptions.length - 1)])}</select>
          </div>
          <div class="form-group" style="margin-bottom:0;min-width:110px;">
            <label class="form-label">Chart Type</label>
            <select class="form-select" id="${moduleId}-custom-type">
              <option value="scatter">Scatter</option>
              <option value="bar">Bar</option>
              <option value="boxplot">Boxplot</option>
              <option value="histogram">Histogram</option>
            </select>
          </div>
          <button class="btn btn-primary btn-sm" onclick="renderCustomPlot('${moduleId}')"><i data-lucide="bar-chart-2"></i> Draw</button>
        </div>
        <div class="chart-container" id="${moduleId}-custom-chart" style="height:320px;"></div>
      </div>`;
  }

  window.renderCustomPlot = function (moduleId) {
    const el = document.getElementById(`${moduleId}-custom-chart`);
    if (!el) return;
    const xSel = document.getElementById(`${moduleId}-custom-x`);
    const ySel = document.getElementById(`${moduleId}-custom-y`);
    const typeSel = document.getElementById(`${moduleId}-custom-type`);
    const xKey = xSel ? xSel.value : 'X';
    const yKey = ySel ? ySel.value : 'Y';
    const chartType = typeSel ? typeSel.value : 'scatter';

    const n = 80;
    const xData = Array.from({ length: n }, () => (Math.random() - 0.5) * 8);
    const yData = xData.map(x => x * 0.4 + (Math.random() - 0.5) * 4);

    let existingChart = echarts.getInstanceByDom(el);
    if (existingChart) existingChart.dispose();
    const chart = createChart(el);

    let series;
    if (chartType === 'scatter') {
      series = [{
        type: 'scatter',
        data: xData.map((x, i) => [x, yData[i]]),
        symbolSize: 6,
        itemStyle: { color: '#0d7377', opacity: 0.65 },
      }];
    } else if (chartType === 'bar') {
      const labels = Array.from({ length: 12 }, (_, i) => `Group_${i + 1}`);
      const vals = labels.map(() => Math.round(Math.random() * 500 + 50));
      series = [{ type: 'bar', data: vals, itemStyle: { color: '#3b6ea5' } }];
      xData.splice(0, xData.length, ...labels);
    } else if (chartType === 'histogram') {
      const bins = Array.from({ length: 20 }, (_, i) => -4 + i * 0.4);
      const counts = bins.map(() => Math.round(Math.random() * 200 + 20));
      series = [{ type: 'bar', data: counts, barWidth: '96%', itemStyle: { color: '#7c5cbf' } }];
      xData.splice(0, xData.length, ...bins.map(b => b.toFixed(1)));
    } else if (chartType === 'boxplot') {
      const groups = ['Control', 'Treated', 'Recovery'];
      const bpData = groups.map(() => {
        const d = Array.from({ length: 50 }, () => Math.random() * 10).sort((a, b) => a - b);
        const q1 = d[12], med = d[24], q3 = d[37];
        return [d[2], q1, med, q3, d[47]];
      });
      series = [{ type: 'boxplot', data: bpData, itemStyle: { color: '#c9503c', borderColor: '#a03020' } }];
      xData.splice(0, xData.length, ...groups);
    }

    const useXCategory = ['bar', 'histogram', 'boxplot'].includes(chartType);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: `${yKey} vs ${xKey}`, textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 14, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: { trigger: chartType === 'scatter' ? 'item' : 'axis' },
      xAxis: { type: useXCategory ? 'category' : 'value', data: useXCategory ? xData : undefined, name: xKey, nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      yAxis: { type: 'value', name: yKey, nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      series,
    });
    window.addEventListener('resize', () => chart.resize());
  };


  // ── Coming Soon ────────────────────────────────────────────
  function renderComingSoon(mod) {
    return `
      <div class="card animate-slide-up" style="animation-delay:100ms">
        <div class="empty-state" style="padding:64px 24px">
          <div class="empty-state-icon"><i data-lucide="${mod.icon}"></i></div>
          <h3 class="empty-state-title">${mod.name}</h3>
          <p class="empty-state-text">This module is under development. The <strong>${mod.tool}</strong> backend integration is being prepared.</p>
          <div style="margin-top:20px"><span class="badge badge-muted" style="font-size:0.8rem;padding:6px 14px">In Development</span></div>
        </div>
      </div>`;
  }

  function renderEmptyState(msg) {
    return `<div class="empty-state"><div class="empty-state-icon"><i data-lucide="alert-circle"></i></div><h3 class="empty-state-title">${msg}</h3></div>`;
  }


  // ── Settings View ──────────────────────────────────────────
  async function renderSettings() {
    let statuses = [];
    try {
      statuses = await window.__TAURI__.core.invoke('get_binary_paths');
    } catch (e) {
      return `<div class="error">Failed to load settings: ${e}</div>`;
    }
    const rows = statuses.map(s => `
      <tr>
        <td>${s.display_name}</td>
        <td class="path">${s.configured_path ?? '<em>(not set)</em>'}</td>
        <td class="path">${s.detected_on_path ?? '<em>(not on PATH)</em>'}</td>
        <td>${s.configured_path || s.detected_on_path ? '<span class="ok">OK</span>' : '<span class="warn">Missing</span>'}</td>
        <td>
          <button data-act="browse" data-id="${s.id}">Browse…</button>
          ${s.configured_path ? `<button data-act="clear" data-id="${s.id}">Clear</button>` : ''}
        </td>
      </tr>
    `).join('');
    return `
      <h2>Settings — Binary Paths</h2>
      <p>When a binary is not on PATH, configure its full path here. Configured paths override PATH.</p>
      <table class="settings-table">
        <thead><tr><th>Tool</th><th>Configured</th><th>Detected on PATH</th><th>Status</th><th>Actions</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `;
  }


  // ── Charts ─────────────────────────────────────────────────
  function initChartsForView(view) {
    switch (view) {
      case 'qc':           renderQCCharts(); break;
      case 'trimming':     renderTrimmingCharts(); break;
      case 'differential': renderDESeq2Charts(); break;
      case 'network':      renderWGCNACharts(); break;
    }
  }

  function renderQCCharts() {
    const el = document.getElementById('qc-quality-chart');
    if (!el) return;

    const pos = Array.from({ length: 150 }, (_, i) => i + 1);
    const mean = pos.map(p => p < 5 ? 32 + Math.random() * 3 : p < 120 ? 34 + Math.random() * 2 : 34 - (p - 120) * 0.15 + Math.random() * 2);
    const lo = mean.map(q => q - 4 - Math.random() * 2);
    const hi = mean.map(q => q + 2 + Math.random());

    const chart = createChart(el);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'Per Base Sequence Quality', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: { trigger: 'axis' },
      xAxis: { type: 'category', data: pos, name: 'Position (bp)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      yAxis: { type: 'value', name: 'Phred Score', nameLocation: 'middle', nameGap: 40, min: 0, max: 42, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      visualMap: false,
      series: [
        {
          type: 'line', data: hi, symbol: 'none', lineStyle: { width: 0 }, showSymbol: false,
          areaStyle: { color: 'rgba(13,115,119,0.08)' }, stack: 'band', name: 'hi',
        },
        {
          type: 'line', data: lo, symbol: 'none', lineStyle: { width: 0 }, showSymbol: false,
          areaStyle: { color: 'rgba(13,115,119,0.08)' }, stack: 'band', name: 'lo',
        },
        {
          type: 'line', data: mean, name: 'Mean Quality', symbol: 'none',
          lineStyle: { color: '#0d7377', width: 2.5 }, smooth: false,
          markLine: {
            silent: true, symbol: 'none',
            lineStyle: { type: 'dashed', color: '#ccc', width: 1 },
            data: [{ yAxis: 28 }, { yAxis: 20 }],
          },
        },
      ],
      legend: { show: false },
    });
    window.addEventListener('resize', () => chart.resize());
  }

  function renderTrimmingCharts() {
    const el = document.getElementById('trim-length-chart');
    if (!el) return;

    const lens = Array.from({ length: 131 }, (_, i) => i + 20);
    const counts = lens.map(l => Math.floor(80000 * Math.exp(-0.5 * ((l - 148) / 8) ** 2) + Math.random() * 1000));
    const colors = lens.map(l => l < 50 ? 'rgba(184,134,11,0.7)' : 'rgba(59,110,165,0.6)');

    const chart = createChart(el);
    chart.setOption({
      backgroundColor: '#faf8f4',
      textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
      title: { text: 'Read Length Distribution After Trimming', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
      grid: ECHART_THEME.grid,
      toolbox: ECHART_THEME.toolbox,
      tooltip: { trigger: 'axis', formatter: params => `Length: ${params[0].name} bp<br>Count: ${params[0].value.toLocaleString()}` },
      xAxis: { type: 'category', data: lens.map(String), name: 'Read Length (bp)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
      yAxis: { type: 'value', name: 'Count', nameLocation: 'middle', nameGap: 50, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
      series: [{
        type: 'bar', data: counts.map((v, i) => ({ value: v, itemStyle: { color: colors[i] } })),
        barWidth: '95%',
      }],
    });
    window.addEventListener('resize', () => chart.resize());
  }

  function renderDESeq2Charts() {
    const volcEl = document.getElementById('deseq-volcano-chart');
    const maEl = document.getElementById('deseq-ma-chart');
    const tbody = document.querySelector('#deseq-results-table tbody');

    const n = 2000;
    const genes = [];
    for (let i = 0; i < n; i++) {
      const lfc = (Math.random() - 0.5) * 8;
      const bm = Math.pow(10, 1 + Math.random() * 4);
      const pv = Math.pow(10, -(Math.abs(lfc) * (1 + Math.random() * 3) + Math.random() * 2));
      const pa = Math.min(1, pv * n / (i + 1));
      genes.push({ name: `Gene_${String(i + 1).padStart(5, '0')}`, log2FC: lfc, baseMean: bm, pvalue: pv, padj: pa, nlp: -Math.log10(Math.max(pa, 1e-300)) });
    }

    if (volcEl) {
      const up = genes.filter(g => g.padj < 0.01 && g.log2FC > 1);
      const dn = genes.filter(g => g.padj < 0.01 && g.log2FC < -1);
      const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1);

      const chart = createChart(volcEl);
      chart.setOption({
        backgroundColor: '#faf8f4',
        textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
        title: { text: 'Volcano Plot', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
        grid: ECHART_THEME.grid,
        toolbox: ECHART_THEME.toolbox,
        tooltip: {
          trigger: 'item',
          formatter: p => `${p.data[2]}<br>log2FC: ${p.data[0].toFixed(2)}<br>-log10(padj): ${p.data[1].toFixed(1)}`,
        },
        legend: { data: ['Not Sig.', 'Up', 'Down'], right: 60, top: 10, textStyle: { fontSize: 11 } },
        xAxis: { type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
        yAxis: { type: 'value', name: '-log10(padj)', nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
        series: [
          {
            name: 'Not Sig.', type: 'scatter', symbolSize: 4,
            data: ns.map(g => [g.log2FC, g.nlp, g.name]),
            itemStyle: { color: 'rgba(168,162,158,0.35)' },
            large: true,
            markLine: {
              silent: true, symbol: 'none',
              lineStyle: { type: 'dashed', color: '#ddd6ca', width: 1 },
              data: [{ xAxis: -1 }, { xAxis: 1 }, { yAxis: 2 }],
            },
          },
          { name: 'Up', type: 'scatter', symbolSize: 5, data: up.map(g => [g.log2FC, g.nlp, g.name]), itemStyle: { color: '#c9503c' }, large: true },
          { name: 'Down', type: 'scatter', symbolSize: 5, data: dn.map(g => [g.log2FC, g.nlp, g.name]), itemStyle: { color: '#3b6ea5' }, large: true },
        ],
      });
      window.addEventListener('resize', () => chart.resize());
    }

    if (maEl) {
      const sig = genes.filter(g => g.padj < 0.01 && Math.abs(g.log2FC) > 1);
      const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1);

      const chart = createChart(maEl);
      chart.setOption({
        backgroundColor: '#faf8f4',
        textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
        title: { text: 'MA Plot', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
        grid: ECHART_THEME.grid,
        toolbox: ECHART_THEME.toolbox,
        tooltip: { trigger: 'item', formatter: p => `log10(Mean): ${p.data[0].toFixed(2)}<br>log2FC: ${p.data[1].toFixed(2)}` },
        legend: { data: ['Not Sig.', 'Significant'], right: 60, top: 10, textStyle: { fontSize: 11 } },
        xAxis: { type: 'value', name: 'log10(Mean Expression)', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
        yAxis: { type: 'value', name: 'log2 Fold Change', nameLocation: 'middle', nameGap: 40, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
        series: [
          {
            name: 'Not Sig.', type: 'scatter', symbolSize: 4,
            data: ns.map(g => [Math.log10(g.baseMean), g.log2FC]),
            itemStyle: { color: 'rgba(168,162,158,0.3)' }, large: true,
            markLine: { silent: true, symbol: 'none', lineStyle: { color: '#c8bfb0', width: 1 }, data: [{ yAxis: 0 }] },
          },
          {
            name: 'Significant', type: 'scatter', symbolSize: 5,
            data: sig.map(g => [Math.log10(g.baseMean), g.log2FC]),
            itemStyle: { color: '#c9503c', opacity: 0.6 }, large: true,
          },
        ],
      });
      window.addEventListener('resize', () => chart.resize());
    }

    if (tbody) {
      const sorted = [...genes].sort((a, b) => a.padj - b.padj).slice(0, 30);
      tbody.innerHTML = sorted.map(g => {
        const sc = g.padj < 0.01 && Math.abs(g.log2FC) > 1 ? 'significant' : '';
        const fc = g.log2FC > 0 ? 'positive' : 'negative';
        return `<tr><td class="gene-name">${g.name}</td><td class="${fc}">${g.log2FC.toFixed(3)}</td><td>${g.pvalue.toExponential(2)}</td><td class="${sc}">${g.padj.toExponential(2)}</td></tr>`;
      }).join('');
    }
  }

  function renderWGCNACharts() {
    const modEl = document.getElementById('wgcna-module-chart');
    const traitEl = document.getElementById('wgcna-trait-chart');

    if (modEl) {
      const names = ['turquoise','blue','brown','green','yellow','red','black','pink','magenta','purple','greenyellow','grey'];
      const sizes = [820,650,520,410,380,310,270,240,190,160,130,920];
      const colors = ['#40E0D0','#4169E1','#8B6914','#228B22','#DAA520','#DC143C','#444','#FF69B4','#C71585','#7B68EE','#7CCD7C','#999'];

      const chart = createChart(modEl);
      chart.setOption({
        backgroundColor: '#faf8f4',
        textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
        title: { text: 'Module Sizes', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
        grid: ECHART_THEME.grid,
        toolbox: ECHART_THEME.toolbox,
        tooltip: { trigger: 'axis', formatter: p => `${p[0].name}<br>${p[0].value} genes` },
        xAxis: { type: 'category', data: names, name: 'Module', nameLocation: 'middle', nameGap: 30, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false }, axisLabel: { rotate: 30 } },
        yAxis: { type: 'value', name: 'Gene Count', nameLocation: 'middle', nameGap: 45, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { lineStyle: { color: '#e8e2d8' } } },
        series: [{
          type: 'bar',
          data: sizes.map((v, i) => ({ value: v, itemStyle: { color: colors[i] + 'CC' } })),
        }],
      });
      window.addEventListener('resize', () => chart.resize());
    }

    if (traitEl) {
      const mods = ['turquoise','blue','brown','green','yellow','red'];
      const traits = ['Treatment','Time','Batch','Age'];
      const data = [];
      mods.forEach((m, mi) => {
        traits.forEach((t, ti) => {
          data.push([ti, mi, parseFloat(((Math.random() - 0.5) * 2).toFixed(2))]);
        });
      });

      const chart = createChart(traitEl);
      chart.setOption({
        backgroundColor: '#faf8f4',
        textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
        title: { text: 'Module-Trait Correlation', textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' }, top: 6, left: 10 },
        grid: { left: 90, right: 80, top: 50, bottom: 60 },
        toolbox: ECHART_THEME.toolbox,
        tooltip: { formatter: p => `${mods[p.data[1]]} vs ${traits[p.data[0]]}<br>r = ${p.data[2]}` },
        xAxis: { type: 'category', data: traits, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
        yAxis: { type: 'category', data: mods, axisLine: { lineStyle: { color: '#ddd6ca' } }, splitLine: { show: false } },
        visualMap: {
          min: -1, max: 1, calculable: true, orient: 'vertical', right: 10, top: 'center',
          inRange: { color: ['#3b6ea5', '#faf8f4', '#c9503c'] },
          textStyle: { color: '#57534e' },
        },
        series: [{
          type: 'heatmap',
          data,
          label: { show: true, formatter: p => p.data[2].toFixed(2), fontSize: 11 },
          emphasis: { itemStyle: { shadowBlur: 10, shadowColor: 'rgba(0,0,0,0.5)' } },
        }],
      });
      window.addEventListener('resize', () => chart.resize());
    }
  }


  // ── Module params collector ────────────────────────────────
  function collectModuleParams(moduleId) {
    const params = {};
    const panel = document.querySelector(`.module-panel`);
    if (!panel) return params;
    panel.querySelectorAll('input[id], select[id]').forEach(el => {
      if (el.type === 'checkbox') params[el.id] = el.checked;
      else params[el.id] = el.value;
    });
    return params;
  }


  // ── Events ─────────────────────────────────────────────────
  function setupEvents() {
    document.addEventListener('click', e => {
      const nav = e.target.closest('[data-view]');
      if (nav) { e.preventDefault(); navigate(nav.dataset.view); document.getElementById('sidebar').classList.remove('open'); return; }

      const tab = e.target.closest('.tab');
      if (tab && tab.dataset.tab) {
        const box = tab.closest('.panel-body') || tab.closest('.module-panel');
        if (box) {
          box.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t === tab));
          box.querySelectorAll('.tab-content').forEach(tc => tc.classList.toggle('active', tc.dataset.tab === tab.dataset.tab));
          const chart = box.querySelector('.tab-content.active .chart-container');
          if (chart && chart.children.length === 0) requestAnimationFrame(() => initChartsForView(state.currentView));
        }
        return;
      }

      const sn = e.target.closest('.settings-nav-item');
      if (sn) { document.querySelectorAll('.settings-nav-item').forEach(s => s.classList.toggle('active', s === sn)); return; }
    });

    document.getElementById('mobileToggle')?.addEventListener('click', () => {
      document.getElementById('sidebar').classList.toggle('open');
    });

    document.addEventListener('dragover', e => {
      const z = e.target.closest('.file-drop-zone');
      if (z) { e.preventDefault(); z.classList.add('dragover'); }
    });
    document.addEventListener('dragleave', e => {
      const z = e.target.closest('.file-drop-zone');
      if (z) z.classList.remove('dragover');
    });
    document.addEventListener('drop', e => {
      const z = e.target.closest('.file-drop-zone');
      if (z) { e.preventDefault(); z.classList.remove('dragover'); handleFileDrop(z, e.dataTransfer.files); }
    });
    document.addEventListener('click', e => {
      const z = e.target.closest('.file-drop-zone');
      if (z && !e.target.closest('.file-item-remove')) {
        // Try native Tauri file dialog first, fall back to HTML input
        api.invoke('select_files', { accept: z.dataset.accept || '*' })
          .then(files => {
            if (files && Array.isArray(files) && files.length > 0) {
              handleFileDrop(z, files.map(f => ({ name: f.split('/').pop(), size: 0 })));
            } else {
              throw new Error('no files');
            }
          })
          .catch(() => {
            const inp = document.createElement('input');
            inp.type = 'file'; inp.multiple = true; inp.accept = z.dataset.accept || '*';
            inp.onchange = () => handleFileDrop(z, inp.files);
            inp.click();
          });
      }
    });

    window.addEventListener('hashchange', () => {
      const h = location.hash.slice(1) || 'dashboard';
      if (h !== state.currentView) navigate(h);
    });

    // STAR Index: form submit
    document.addEventListener('submit', (e) => {
      if (e.target.id === 'form-star-index') { e.preventDefault(); submitStarIndex(e.target); }
    });

    // Generic file-pick handler for data-pick-for buttons
    document.addEventListener('click', async (e) => {
      const btn = e.target.closest('[data-pick-for]');
      if (!btn) return;
      const field = btn.dataset.pickFor;
      const picked = await window.__TAURI__.core.invoke('select_files', { multiple: false });
      if (picked && picked[0]) {
        const input = btn.parentElement.querySelector(`input[name="${field}"]`);
        if (input) input.value = picked[0];
      }
    });

    // Settings: binary path browse / clear buttons
    document.addEventListener('click', async (e) => {
      const btn = e.target.closest('[data-act="browse"]');
      if (btn) {
        const picked = await window.__TAURI__.core.invoke('select_files', { multiple: false });
        if (picked && picked[0]) {
          try {
            await window.__TAURI__.core.invoke('set_binary_path', { name: btn.dataset.id, path: picked[0] });
            navigate('settings');
          } catch (err) { alert('Failed: ' + err); }
        }
        return;
      }
      const clr = e.target.closest('[data-act="clear"]');
      if (clr) {
        try {
          await window.__TAURI__.core.invoke('clear_binary_path', { name: clr.dataset.id });
          navigate('settings');
        } catch (err) { alert('Failed: ' + err); }
      }
    });
  }

  function handleFileDrop(zone, fileList) {
    const mid = zone.dataset.module;
    if (!mid || !state.files[mid]) state.files[mid] = [];
    Array.from(fileList).forEach(f => {
      if (!state.files[mid]) state.files[mid] = [];
      state.files[mid].push({ name: f.name || f, size: f.size || 0 });
    });
    const list = document.getElementById(`${mid}-file-list`);
    if (list) renderFileList(list, mid);
  }

  function renderFileList(el, mid) {
    const files = state.files[mid] || [];
    el.innerHTML = files.map((f, i) => `
      <div class="file-item">
        <i data-lucide="file" style="width:14px;height:14px;color:var(--text-muted);flex-shrink:0"></i>
        <span class="file-item-name">${f.name}</span>
        <span class="file-item-size">${fmtSize(f.size)}</span>
        <span class="file-item-remove" data-module="${mid}" data-index="${i}"><i data-lucide="x"></i></span>
      </div>`).join('');
    if (window.lucide) lucide.createIcons();
    el.querySelectorAll('.file-item-remove').forEach(btn => {
      btn.addEventListener('click', e => {
        e.stopPropagation();
        state.files[btn.dataset.module].splice(parseInt(btn.dataset.index), 1);
        renderFileList(el, btn.dataset.module);
      });
    });
  }

  function fmtSize(b) {
    if (!b) return '0 B';
    const u = ['B','KB','MB','GB'];
    const i = Math.floor(Math.log(b) / Math.log(1024));
    return `${(b / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${u[i]}`;
  }

  // ── Global helpers ─────────────────────────────────────────
  window.toggleCollapsible = function (trigger) {
    const c = trigger.closest('.collapsible');
    if (c) { c.classList.toggle('open'); if (window.lucide) lucide.createIcons(); }
  };

  window.runModule = async function (id) {
    const st = document.getElementById('statusText');
    const js = document.getElementById('jobStatus');
    const mod = MODULES.find(m => m.id === id);
    st.textContent = `Running ${mod?.name || id}...`;
    js.textContent = '1 active job';
    const badge = document.querySelector(`.nav-item[data-view="${id}"] .nav-badge`);
    if (badge) { badge.className = 'nav-badge running'; badge.textContent = 'Running'; }

    const params = collectModuleParams(id);
    try {
      await api.invoke('validate_params', { module: id, params });
      const runId = await api.invoke('run_module', { module: id, params });
      if (runId) state.runIdToModule[runId] = id;
    } catch (err) {
      console.warn(`[runModule] invoke failed for ${id}:`, err);
    }

    st.textContent = 'Ready'; js.textContent = 'No active jobs';
    if (badge) { badge.className = 'nav-badge done'; badge.textContent = 'Done'; }
  };

  window.resetForm = function (id) { state.files[id] = []; navigate(id); };


  // --- run-log streaming support (shared across all modules) ---
  const LOG_BUFFER_MAX = 500;
  state.logsByRun = state.logsByRun || {};
  state.runIdToModule = state.runIdToModule || {};

  function appendRunLog(runId, line, stream) {
    const buf = (state.logsByRun[runId] = state.logsByRun[runId] || []);
    buf.push({ line, stream });
    while (buf.length > LOG_BUFFER_MAX) buf.shift();
    // resolve panel key: prefer module id mapping, fall back to runId
    const panelKey = state.runIdToModule[runId] || runId;
    const panel = document.querySelector(`[data-log-panel="${panelKey}"] pre`);
    if (panel) {
      const prefix = stream === 'stderr' ? '' : '[out] ';
      panel.textContent += prefix + line + '\n';
      if (!panel.dataset.userScrolled) {
        panel.scrollTop = panel.scrollHeight;
      }
    }
  }

  function renderLogPanel(panelKey) {
    const existing = state.logsByRun[panelKey] || [];
    const text = existing.map(e => (e.stream === 'stderr' ? '' : '[out] ') + e.line).join('\n');
    return `<details class="log-panel" data-log-panel="${panelKey}">
    <summary>Log</summary>
    <pre>${text}</pre>
  </details>`;
  }

  // Attach pre-scroll-watch so auto-scroll respects user intent
  document.addEventListener('scroll', (e) => {
    const pre = e.target;
    if (pre.tagName !== 'PRE' || !pre.closest('[data-log-panel]')) return;
    const nearBottom = pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20;
    if (nearBottom) delete pre.dataset.userScrolled;
    else pre.dataset.userScrolled = '1';
  }, true);


  // ── Init ───────────────────────────────────────────────────
  function init() {
    setupEvents();

    // Tauri event listeners
    api.listen('run-progress', event => {
      const st = document.getElementById('statusText');
      const log = document.querySelector('.log-output');
      if (st) st.textContent = event.payload?.message || 'Running...';
      if (log) log.innerHTML += `\n<span class="log-info">[INFO]</span> ${event.payload?.message || ''}`;
    });
    api.listen('run-completed', event => {
      const st = document.getElementById('statusText');
      const js = document.getElementById('jobStatus');
      if (st) st.textContent = 'Ready';
      if (js) js.textContent = 'No active jobs';
      if (event.payload?.module) navigate(event.payload.module);
    });
    api.listen('run-failed', event => {
      const st = document.getElementById('statusText');
      if (st) st.textContent = `Error: ${event.payload?.message || 'Run failed'}`;
      console.error('[run-failed]', event.payload);
    });

    // Wire up run-log streaming
    if (window.__TAURI__?.event) {
      window.__TAURI__.event.listen('run-log', (e) => {
        const { runId, line, stream } = e.payload || {};
        if (runId) appendRunLog(runId, line, stream);
      });
    }

    navigate(location.hash.slice(1) || 'dashboard');
    if (window.lucide) lucide.createIcons();
    console.log('%cRustBrain %cv0.1.0', 'font-weight:bold;font-size:14px;color:#0d7377', 'color:#57534e');
  }

  document.addEventListener('DOMContentLoaded', init);
})();
