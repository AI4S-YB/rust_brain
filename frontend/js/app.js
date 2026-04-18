/* ============================================================
   RustBrain — Transcriptomics Analysis Platform
   Frontend Application (Warm Light Theme)
   ============================================================ */

(function () {
  'use strict';

  // ── i18n helper (falls back to key if i18n.js failed to load) ─
  const t = (k, v) => (window.I18N ? window.I18N.t(k, v) : k);
  const navKey = (id) => 'nav.' + String(id).replace(/-/g, '_');

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

  // Views with localized breadcrumb labels (keys live in i18n.js as `nav.<id>` with '-' → '_').
  const KNOWN_VIEWS = new Set([
    'dashboard', 'settings', 'gff-convert', 'star-index', 'star-align',
    ...MODULES.map(m => m.id),
  ]);

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

  // ── XSS helper ────────────────────────────────────────────
  const escapeHtml = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ));

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
    const label = KNOWN_VIEWS.has(view) ? t(navKey(view)) : view;
    bc.innerHTML = `
      <span class="breadcrumb-home">${t('brand.name')}</span>
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
    else if (view === 'gff-convert') content.innerHTML = renderGffConvert();
    else if (view === 'star-index') content.innerHTML = renderStarIndex();
    else if (view === 'star-align') content.innerHTML = renderStarAlign();
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
      const nameKey = navKey(m.id);
      return `
        <div class="pipeline-stage animate-slide-up" style="animation-delay: ${i * 60}ms">
          <div class="pipeline-node ${m.status}" data-view="${m.id}" style="--node-color: ${COLOR_MAP[m.color]}">
            <div class="pipeline-node-icon"><i data-lucide="${m.icon}"></i></div>
            <div class="pipeline-node-title">${t(nameKey)}</div>
            <div class="pipeline-node-desc">${m.tool}</div>
            <div class="pipeline-node-status">
              <span class="dot"></span>
              ${m.status === 'ready' ? t('badge.available') : t('badge.coming_soon')}
            </div>
          </div>
          ${connector}
        </div>`;
    }).join('');

    const projName = state.projectOpen ? state.projectName : t('project.none_open');
    const projStatus = state.projectOpen
      ? `<span class="badge badge-green" style="margin-left:8px">${t('project.open_badge')}</span>`
      : `<span class="badge badge-muted" style="margin-left:8px">${t('project.closed_badge')}</span>`;

    return `
      <div class="module-view">
        <div class="dashboard-hero animate-slide-up">
          <h1 class="dashboard-title">
            <span class="dashboard-title-accent">${t('dashboard.title_accent')}</span> ${t('dashboard.title_rest')}
          </h1>
          <p class="dashboard-subtitle">
            ${t('dashboard.subtitle')}
          </p>
        </div>

        <div class="card animate-slide-up" style="animation-delay: 40ms; margin-bottom: 16px; padding: 16px 24px;">
          <div class="card-header" style="margin-bottom: 12px">
            <span class="card-title"><i data-lucide="folder-open" style="width:15px;height:15px;vertical-align:-2px;margin-right:6px"></i>${t('project.section_title')}</span>
            ${projStatus}
          </div>
          <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;">
            <span style="font-size:0.9rem;color:var(--text-secondary);flex:1;min-width:120px;" id="dash-proj-name">${projName}</span>
            <button class="btn btn-secondary btn-sm" onclick="projectNew()"><i data-lucide="folder-plus"></i> ${t('project.new')}</button>
            <button class="btn btn-secondary btn-sm" onclick="projectOpen()"><i data-lucide="folder-open"></i> ${t('project.open')}</button>
          </div>
        </div>

        <div class="pipeline-flow-container card animate-slide-up" style="animation-delay: 60ms; padding: 16px 24px;">
          <div class="card-header" style="margin-bottom: 8px">
            <span class="card-title">${t('dashboard.pipeline_section')}</span>
            <span class="badge badge-teal">${t('dashboard.modules_badge', { n: MODULES.length })}</span>
          </div>
          <div class="pipeline-flow stagger">
            ${pipelineNodes}
          </div>
        </div>

        <div class="stats-row stagger">
          <div class="stat-card animate-slide-up">
            <div class="stat-label">${t('dashboard.stat_modules_ready')}</div>
            <div class="stat-value">4<span class="stat-unit">/ 7</span></div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">${t('dashboard.stat_rust_tools')}</div>
            <div class="stat-value">4</div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">${t('dashboard.stat_active_jobs')}</div>
            <div class="stat-value">0</div>
          </div>
          <div class="stat-card animate-slide-up">
            <div class="stat-label">${t('dashboard.stat_speed_gain')}</div>
            <div class="stat-value">28<span class="stat-unit">x</span></div>
          </div>
        </div>

        <div class="dashboard-grid">
          <div class="card animate-slide-up" style="animation-delay: 200ms">
            <div class="card-header">
              <span class="card-title">${t('dashboard.quick_start')}</span>
            </div>
            <div class="quick-actions">
              ${renderQuickAction('qc', 'microscope', 'teal', t('dashboard.quick.qc_title'), t('dashboard.quick.qc_desc'))}
              ${renderQuickAction('trimming', 'scissors', 'blue', t('dashboard.quick.trim_title'), t('dashboard.quick.trim_desc'))}
              ${renderQuickAction('differential', 'flame', 'coral', t('dashboard.quick.deseq_title'), t('dashboard.quick.deseq_desc'))}
              ${renderQuickAction('network', 'share-2', 'green', t('dashboard.quick.wgcna_title'), t('dashboard.quick.wgcna_desc'))}
            </div>
          </div>

          <div class="card animate-slide-up" style="animation-delay: 260ms">
            <div class="card-header">
              <span class="card-title">${t('dashboard.tool_suite')}</span>
            </div>
            <div>
              ${renderToolInfo('fastqc-rs', t('dashboard.tool_desc.fastqc'), 'GPL-3.0')}
              ${renderToolInfo('cutadapt-rs', t('dashboard.tool_desc.cutadapt'), 'MIT')}
              ${renderToolInfo('DESeq2_rs', t('dashboard.tool_desc.deseq2'), 'MIT')}
              ${renderToolInfo('WGCNA_rs', t('dashboard.tool_desc.wgcna'), 'GPL-2.0')}
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
    const name = prompt(t('project.prompt_name'));
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
    if (!mod) return renderEmptyState(t('common.module_not_found'));

    const hex = COLOR_MAP[mod.color];
    const nameKey = navKey(mod.id);
    const header = `
      <div class="module-header animate-slide-up">
        <div class="module-icon" style="background: ${hex}12; color: ${hex};">
          <i data-lucide="${mod.icon}"></i>
        </div>
        <div>
          <h1 class="module-title">${t(nameKey)}</h1>
          <p class="module-desc">${t('module.powered_by')} <strong style="color: ${hex}">${mod.tool}</strong></p>
          <div class="module-badges">
            <span class="badge badge-${mod.color}">${mod.status === 'ready' ? t('badge.available') : t('badge.coming_soon')}</span>
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
              <span class="panel-title">${t('qc.input_files')}</span>
              <span class="badge badge-teal">${t('qc.files_count', { n: state.files.qc.length })}</span>
            </div>
            <div class="panel-body">
              <div class="file-drop-zone" data-module="qc" data-accept=".fastq,.fq,.fastq.gz,.fq.gz,.bam,.sam">
                <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
                <div class="file-drop-text">${t('qc.drop_text')}</div>
                <div class="file-drop-hint">${t('qc.drop_hint')}</div>
              </div>
              <div class="file-list" id="qc-file-list"></div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">${t('qc.parameters')}</span></div>
            <div class="panel-body">
              <div class="form-row">
                <div class="form-group">
                  <label class="form-label">${t('qc.threads')}</label>
                  <input type="number" class="form-input" id="qc-threads" value="4" min="1" max="32">
                </div>
                <div class="form-group">
                  <label class="form-label">${t('qc.format')}</label>
                  <select class="form-select" id="qc-format">
                    <option>${t('qc.format_auto')}</option><option>FASTQ</option><option>BAM</option><option>SAM</option>
                  </select>
                </div>
              </div>
              <div class="form-group">
                <label class="form-label">${t('qc.output_dir')}</label>
                <input type="text" class="form-input" id="qc-output" placeholder="${t('qc.output_dir_ph')}">
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">
                  ${t('common.advanced_options')} <i data-lucide="chevron-down"></i>
                </button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-casava"> ${t('qc.casava')}</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="qc-nogroup"> ${t('qc.nogroup')}</label></div>
                  <div class="form-group"><label class="form-label">${t('qc.kmer')}</label><input type="number" class="form-input" id="qc-kmer" value="7" min="2" max="10"></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('qc')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('qc')"><i data-lucide="play"></i> ${t('qc.run_qc')}</button>
            </div>
            ${renderLogPanel('qc')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">${t('qc.results')}</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="qc-chart">${t('qc.tab_quality')}</div>
                <div class="tab" data-tab="qc-summary">${t('qc.tab_summary')}</div>
                <div class="tab" data-tab="qc-log">${t('qc.tab_log')}</div>
              </div>
              <div class="tab-content active" data-tab="qc-chart">
                <div class="chart-container" id="qc-quality-chart" style="height:320px;"></div>
              </div>
              <div class="tab-content" data-tab="qc-summary">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">Pass</div><div class="result-metric-label">${t('qc.metric_overall')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">35.2</div><div class="result-metric-label">${t('qc.metric_mean_quality')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">12.4M</div><div class="result-metric-label">${t('qc.metric_total_reads')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">150</div><div class="result-metric-label">${t('qc.metric_read_length')}</div></div>
                </div>
                <table class="data-table"><thead><tr><th>${t('qc.col_module')}</th><th>${t('qc.col_status')}</th></tr></thead><tbody>
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
            <div class="panel-header"><span class="panel-title">${t('trimming.input_files')}</span></div>
            <div class="panel-body">
              <div class="file-drop-zone" data-module="trimming" data-accept=".fastq,.fq,.fastq.gz,.fq.gz">
                <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
                <div class="file-drop-text">${t('trimming.drop_text')}</div>
                <div class="file-drop-hint">${t('trimming.drop_hint')}</div>
              </div>
              <div class="file-list" id="trimming-file-list"></div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">${t('trimming.adapter_settings')}</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">${t('trimming.adapter_preset')}</label>
                <select class="form-select" id="trim-preset">
                  <option>${t('trimming.preset_illumina')}</option>
                  <option>${t('trimming.preset_nextera')}</option>
                  <option>${t('trimming.preset_smallrna')}</option>
                  <option>${t('trimming.preset_bgi')}</option>
                  <option>${t('trimming.preset_custom')}</option>
                </select>
              </div>
              <div class="form-group">
                <label class="form-label">${t('trimming.adapter_3')}</label>
                <input type="text" class="form-input" id="trim-adapter" value="AGATCGGAAGAGC" placeholder="AGATCGGAAGAGC">
                <span class="form-hint">${t('trimming.adapter_3_hint')}</span>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">${t('trimming.quality_cutoff')}</label><input type="number" class="form-input" id="trim-quality" value="20" min="0" max="42"></div>
                <div class="form-group"><label class="form-label">${t('trimming.min_length')}</label><input type="number" class="form-input" id="trim-minlen" value="20" min="1"></div>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">${t('trimming.max_n')}</label><input type="number" class="form-input" id="trim-maxn" value="-1"><span class="form-hint">${t('trimming.max_n_hint')}</span></div>
                <div class="form-group"><label class="form-label">${t('trimming.threads')}</label><input type="number" class="form-input" id="trim-threads" value="4" min="1" max="16"></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('trimming.paired_options')} <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-paired"> ${t('trimming.paired_mode')}</label></div>
                  <div class="form-group"><label class="form-label">${t('trimming.adapter_r2')}</label><input type="text" class="form-input" id="trim-adapter2" placeholder="AGATCGGAAGAGC"></div>
                </div></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('trimming.trim_galore')} <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-galore"> ${t('trimming.enable_galore')}</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-fastqc"> ${t('trimming.post_fastqc')}</label></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="trim-rrbs"> ${t('trimming.rrbs')}</label></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('trimming')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('trimming')" style="background:var(--mod-blue);border-color:var(--mod-blue)"><i data-lucide="play"></i> ${t('trimming.run_trim')}</button>
            </div>
            ${renderLogPanel('trimming')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">${t('trimming.results')}</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="trim-stats">${t('trimming.tab_stats')}</div>
                <div class="tab" data-tab="trim-chart">${t('trimming.tab_chart')}</div>
                <div class="tab" data-tab="trim-log">${t('qc.tab_log')}</div>
              </div>
              <div class="tab-content active" data-tab="trim-stats">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">10.2M</div><div class="result-metric-label">${t('trimming.metric_reads_processed')}</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">98.7%</div><div class="result-metric-label">${t('trimming.metric_reads_passing')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">4.3%</div><div class="result-metric-label">${t('trimming.metric_adapter_found')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">142</div><div class="result-metric-label">${t('trimming.metric_mean_length')}</div></div>
                </div>
                <table class="data-table"><thead><tr><th>${t('trimming.col_metric')}</th><th>${t('trimming.col_value')}</th></tr></thead><tbody>
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


  // ── GFF Convert ───────────────────────────────────────────
  function renderGffConvert() {
    return `
    <h2>${t('gff_convert.title')}</h2>
    <p>${t('gff_convert.desc')}</p>
    <form id="form-gff-convert">
      <label>${t('gff_convert.input_file')}
        <input type="text" name="input_file" data-pick="file" placeholder="/path/to/anno.gff3" required />
        <button type="button" data-pick-for="input_file">${t('common.browse')}</button>
      </label>
      <label>${t('gff_convert.target_format')}
        <select name="target_format" required>
          <option value="gtf">${t('gff_convert.target_gtf')}</option>
          <option value="gff3">${t('gff_convert.target_gff3')}</option>
        </select>
      </label>
      <details><summary>${t('gff_convert.advanced')}</summary>
        <label>${t('gff_convert.extra_args')}
          <textarea name="extra_args" placeholder="--keep-comments&#10;--force-exons"></textarea>
        </label>
      </details>
      <button type="submit">${t('gff_convert.submit')}</button>
    </form>
    <div id="gff-convert-runs"></div>
    ${renderLogPanel('gff_convert')}
  `;
  }

  async function submitGffConvert(form) {
    const fd = new FormData(form);
    const extra_args = (fd.get('extra_args') || '').toString()
      .split('\n').map(s => s.trim()).filter(Boolean);
    const params = {
      input_file: fd.get('input_file'),
      target_format: fd.get('target_format'),
      extra_args,
    };
    try {
      const runId = await window.__TAURI__.core.invoke('run_module', {
        moduleId: 'gff_convert', params,
      });
      state.runIdToModule = state.runIdToModule || {};
      state.runIdToModule[runId] = 'gff_convert';
      navigate('gff-convert');
    } catch (err) {
      alert('Failed to start run: ' + err);
    }
  }

  // ── STAR Index ────────────────────────────────────────────
  function renderStarIndex() {
    const prefill = (state.prefill && state.prefill.star_index) || {};
    state.prefill = {};
    const gtfValue = prefill.gtf_file || '';
    return `
    <h2>${t('star_index.title')}</h2>
    <p>${t('star_index.desc')}</p>
    <form id="form-star-index">
      <label>${t('star_index.genome_fasta')}
        <input type="text" name="genome_fasta" data-pick="file" placeholder="/path/to/genome.fa" required />
        <button type="button" data-pick-for="genome_fasta">${t('common.browse')}</button>
      </label>
      <label>${t('star_index.gtf')}
        <input type="text" name="gtf_file" data-pick="file" value="${escapeHtml(gtfValue)}" placeholder="/path/to/annotation.gtf" required />
        <button type="button" data-pick-for="gtf_file">${t('common.browse')}</button>
      </label>
      <label>${t('star_index.threads')} <input type="number" name="threads" value="4" min="1" /></label>
      <label>${t('star_index.sjdb')} <input type="number" name="sjdb_overhang" value="100" min="1" /></label>
      <label>${t('star_index.sa_nbases')} <input type="number" name="genome_sa_index_nbases" value="14" min="1" max="18" /></label>
      <details><summary>${t('star_index.advanced')}</summary>
        <label>${t('star_index.extra_args')}
          <textarea name="extra_args" placeholder="--limitGenomeGenerateRAM&#10;31000000000"></textarea>
        </label>
      </details>
      <button type="submit">${t('star_index.submit')}</button>
    </form>
    <div id="star-index-runs"></div>
    ${renderLogPanel('star_index')}
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
      state.runIdToModule = state.runIdToModule || {};
      state.runIdToModule[runId] = 'star_index';
      navigate('star-index');
    } catch (err) { alert('Failed to start run: ' + err); }
  }


  // ── STAR Alignment ────────────────────────────────────────
  function renderStarAlign() {
    return `
    <h2>${t('star_align.title')}</h2>
    <p>${t('star_align.desc')}</p>
    <form id="form-star-align">
      <label>${t('star_align.genome_dir')}
        <input type="text" name="genome_dir" required placeholder="/path/to/star_index" />
        <button type="button" data-pick-for="genome_dir" data-pick-mode="dir">${t('common.browse')}</button>
      </label>
      <label>${t('star_align.reads_1')}
        <input type="text" name="reads_1" required placeholder="/path/to/S1_R1.fq.gz /path/to/S2_R1.fq.gz" />
        <button type="button" data-pick-for="reads_1" data-pick-mode="multi">${t('common.browse')}</button>
      </label>
      <label>${t('star_align.reads_2')}
        <input type="text" name="reads_2" placeholder="/path/to/S1_R2.fq.gz /path/to/S2_R2.fq.gz" />
        <button type="button" data-pick-for="reads_2" data-pick-mode="multi">${t('common.browse')}</button>
      </label>
      <label>${t('star_align.sample_names')}
        <textarea name="sample_names" placeholder="S1&#10;S2"></textarea>
      </label>
      <label>${t('star_align.threads')} <input type="number" name="threads" value="4" min="1" /></label>
      <fieldset>
        <legend>${t('star_align.strand')}</legend>
        <label><input type="radio" name="strand" value="unstranded" checked /> ${t('star_align.strand_unstranded')}</label>
        <label><input type="radio" name="strand" value="forward" /> ${t('star_align.strand_forward')}</label>
        <label><input type="radio" name="strand" value="reverse" /> ${t('star_align.strand_reverse')}</label>
      </fieldset>
      <details><summary>${t('star_align.advanced')}</summary>
        <label>${t('star_align.extra_args')}
          <textarea name="extra_args" placeholder="--outFilterMultimapNmax&#10;10"></textarea>
        </label>
      </details>
      <button type="submit">${t('star_align.submit')}</button>
    </form>
    <div id="star-align-runs"></div>
    ${renderLogPanel('star_align')}
  `;
  }

  async function submitStarAlign(form) {
    const fd = new FormData(form);
    const splitPaths = (s) => (s || '').toString().split(/\s+/).map(x => x.trim()).filter(Boolean);
    const splitLines = (s) => (s || '').toString().split('\n').map(x => x.trim()).filter(Boolean);
    const params = {
      genome_dir:    fd.get('genome_dir'),
      reads_1:       splitPaths(fd.get('reads_1')),
      reads_2:       splitPaths(fd.get('reads_2')),
      sample_names:  splitLines(fd.get('sample_names')),
      threads:       parseInt(fd.get('threads'), 10) || 4,
      strand:        fd.get('strand') || 'unstranded',
      extra_args:    splitLines(fd.get('extra_args')),
    };
    if (params.sample_names.length === 0) delete params.sample_names;
    if (params.reads_2.length === 0)     delete params.reads_2;
    try {
      const runId = await window.__TAURI__.core.invoke('run_module', { moduleId: 'star_align', params });
      state.runIdToModule = state.runIdToModule || {};
      state.runIdToModule[runId] = 'star_align';
      state.currentRunId = runId;
      navigate('star-align');
    } catch (err) { alert('Failed to start run: ' + err); }
  }

  function renderStarAlignResult(result, runId) {
    const suffix = runId || 'current';
    const chartId = `star-align-chart-${suffix}`;
    const previewId = `star-align-preview-${suffix}`;
    const btnId = `star-to-deseq-${suffix}`;

    const samples = (result.summary && result.summary.samples) || [];
    const matrixPath = result.summary && result.summary.counts_matrix;
    const data = {
      names: samples.map(s => s.name),
      uniq:  samples.map(s => (s.stats && s.stats.uniquely_mapped) || 0),
      multi: samples.map(s => (s.stats && s.stats.multi_mapped) || 0),
      unmap: samples.map(s => (s.stats && s.stats.unmapped) || 0),
    };
    setTimeout(() => renderMappingRateChart(chartId, data), 0);

    let previewHtml = '';
    if (matrixPath) {
      window.__TAURI__.core.invoke('read_table_preview', { path: matrixPath, max_rows: 50, max_cols: 10 })
        .then(rows => {
          const el = document.getElementById(previewId);
          if (!el || !rows || rows.length === 0) return;
          const header = rows[0].map(c => `<th>${c}</th>`).join('');
          const body = rows.slice(1).map(r => '<tr>' + r.map(c => `<td>${c}</td>`).join('') + '</tr>').join('');
          el.innerHTML = `<table class="preview-table"><thead><tr>${header}</tr></thead><tbody>${body}</tbody></table>`;
        }).catch(() => {});
    } else {
      previewHtml = `<p><em>${t('star_align.no_matrix')}</em></p>`;
    }

    return `
    <h3>${t('star_align.mapping_rate')}</h3>
    <div id="${chartId}" style="width: 100%; height: 320px;"></div>
    <h3>${t('star_align.matrix_preview')}</h3>
    <div id="${previewId}">${t('common.loading')}</div>
    ${matrixPath ? `<button id="${btnId}" data-matrix="${matrixPath}">${t('star_align.use_in_deseq')}</button>` : ''}
    ${previewHtml}
  `;
  }

  function renderGffConvertResult(result, runId) {
    const s = result.summary || {};
    const out = (result.output_files && result.output_files[0]) || s.output || '';
    const fmt = String(s.target_format || '').toUpperCase();
    return `
      <div class="run-result-card">
        <h3>${t('gff_convert.converted_heading', { format: escapeHtml(fmt) })}</h3>
        <dl class="result-kv">
          <dt>${t('gff_convert.kv_input')}</dt><dd class="path">${escapeHtml(s.input || '')}</dd>
          <dt>${t('gff_convert.kv_output')}</dt><dd class="path">${escapeHtml(out)}</dd>
          <dt>${t('gff_convert.kv_input_size')}</dt><dd>${s.input_bytes ?? '?'} ${t('gff_convert.kv_bytes_suffix')}</dd>
          <dt>${t('gff_convert.kv_output_size')}</dt><dd>${s.output_bytes ?? '?'} ${t('gff_convert.kv_bytes_suffix')}</dd>
          <dt>${t('gff_convert.kv_elapsed')}</dt><dd>${s.elapsed_ms ?? '?'} ${t('gff_convert.kv_ms_suffix')}</dd>
        </dl>
        <button type="button" data-gff-use-in-star="${escapeHtml(out)}">${t('gff_convert.use_in_star')}</button>
      </div>
    `;
  }

  function renderMappingRateChart(elId, data) {
    const el = document.getElementById(elId);
    if (!el || !window.echarts) return;
    const chart = window.echarts.init(el, ECHART_THEME);
    chart.setOption({
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      legend: { data: ['Unique', 'Multi', 'Unmapped'] },
      grid: { left: 60, right: 20, top: 40, bottom: 50 },
      xAxis: { type: 'category', data: data.names },
      yAxis: { type: 'value', name: 'Reads' },
      series: [
        { name: 'Unique',   type: 'bar', stack: 'total', data: data.uniq },
        { name: 'Multi',    type: 'bar', stack: 'total', data: data.multi },
        { name: 'Unmapped', type: 'bar', stack: 'total', data: data.unmap },
      ],
    });
  }


  // ── Differential Expression ────────────────────────────────
  function renderDifferential() {
    const prefill = (state.prefill && state.prefill.differential) || {};
    state.prefill = {};
    return `
      <div class="module-layout">
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:100ms">
            <div class="panel-header"><span class="panel-title">${t('differential.input_data')}</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">${t('differential.counts_matrix')}</label>
                ${prefill.counts_matrix
                  ? `<input type="text" class="form-input" id="deseq-counts-matrix" value="${prefill.counts_matrix}" placeholder="${t('differential.counts_matrix_ph')}">`
                  : `<div class="file-drop-zone" data-module="differential" data-accept=".tsv,.csv,.txt" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="table"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_counts')}</div>
                  <div class="file-drop-hint">${t('differential.drop_counts_hint')}</div>
                </div>`}
              </div>
              <div class="form-group">
                <label class="form-label">${t('differential.sample_info')}</label>
                <div class="file-drop-zone" data-module="differential-coldata" data-accept=".tsv,.csv,.txt" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">${t('differential.drop_coldata')}</div>
                  <div class="file-drop-hint">${t('differential.drop_coldata_hint')}</div>
                </div>
              </div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">${t('differential.parameters')}</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">${t('differential.design_var')}</label>
                <input type="text" class="form-input" id="deseq-design" value="condition" placeholder="${t('differential.design_var_ph')}">
                <span class="form-hint">${t('differential.design_var_hint')}</span>
              </div>
              <div class="form-group">
                <label class="form-label">${t('differential.ref_level')}</label>
                <input type="text" class="form-input" id="deseq-ref" value="control" placeholder="${t('differential.ref_level_ph')}">
                <span class="form-hint">${t('differential.ref_level_hint')}</span>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">${t('differential.padj')}</label><input type="number" class="form-input" id="deseq-padj" value="0.01" step="0.01" min="0" max="1"></div>
                <div class="form-group"><label class="form-label">${t('differential.lfc')}</label><input type="number" class="form-input" id="deseq-lfc" value="1.0" step="0.1" min="0"></div>
              </div>
              <div class="form-group">
                <label class="form-label">${t('differential.output_file')}</label>
                <input type="text" class="form-input" id="deseq-output" value="deseq2_results.tsv" placeholder="results.tsv">
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm" onclick="resetForm('differential')"><i data-lucide="rotate-ccw"></i> ${t('common.reset')}</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('differential')" style="background:var(--mod-coral);border-color:var(--mod-coral)"><i data-lucide="play"></i> ${t('differential.run_deseq')}</button>
            </div>
            ${renderLogPanel('differential')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">${t('differential.results')}</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="deseq-volcano">${t('differential.tab_volcano')}</div>
                <div class="tab" data-tab="deseq-ma">${t('differential.tab_ma')}</div>
                <div class="tab" data-tab="deseq-table">${t('differential.tab_table')}</div>
                <div class="tab" data-tab="deseq-custom">${t('differential.tab_custom')}</div>
                <div class="tab" data-tab="deseq-log">${t('qc.tab_log')}</div>
              </div>
              <div class="tab-content active" data-tab="deseq-volcano">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">64,102</div><div class="result-metric-label">${t('differential.metric_total')}</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-coral)">347</div><div class="result-metric-label">${t('differential.metric_up')}</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-blue)">325</div><div class="result-metric-label">${t('differential.metric_down')}</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-teal)">672</div><div class="result-metric-label">${t('differential.metric_sig')}</div></div>
                </div>
                <div class="chart-container" id="deseq-volcano-chart" style="height:380px;"></div>
              </div>
              <div class="tab-content" data-tab="deseq-ma">
                <div class="chart-container" id="deseq-ma-chart" style="height:380px;"></div>
              </div>
              <div class="tab-content" data-tab="deseq-table">
                <div style="display:flex;justify-content:flex-end;margin-bottom:8px;">
                  <button class="btn btn-ghost btn-sm" onclick="exportTableAsTSV('deseq-results-table', 'deseq2_results.tsv')">${t('common.export_tsv')}</button>
                </div>
                <div style="max-height:400px;overflow-y:auto;">
                  <table class="data-table" id="deseq-results-table"><thead><tr><th>${t('differential.col_gene')}</th><th>${t('differential.col_lfc')}</th><th>${t('differential.col_pvalue')}</th><th>${t('differential.col_padj')}</th></tr></thead><tbody></tbody></table>
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
            <div class="panel-header"><span class="panel-title">${t('network.input_data')}</span></div>
            <div class="panel-body">
              <div class="form-group">
                <label class="form-label">${t('network.expr_matrix')}</label>
                <div class="file-drop-zone" data-module="network" data-accept=".csv,.tsv" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="grid-3x3"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">${t('network.drop_expr')}</div>
                  <div class="file-drop-hint">${t('network.drop_expr_hint')}</div>
                </div>
              </div>
              <div class="form-group">
                <label class="form-label">${t('network.trait_data')}</label>
                <div class="file-drop-zone" data-module="network-trait" data-accept=".csv,.tsv" style="padding:20px">
                  <div class="file-drop-icon" style="margin-bottom:8px"><i data-lucide="file-text"></i></div>
                  <div class="file-drop-text" style="font-size:0.85rem">${t('network.drop_trait')}</div>
                  <div class="file-drop-hint">${t('network.drop_trait_hint')}</div>
                </div>
              </div>
            </div>
          </div>
          <div class="module-panel animate-slide-up" style="animation-delay:160ms">
            <div class="panel-header"><span class="panel-title">${t('network.parameters')}</span></div>
            <div class="panel-body">
              <div class="form-group"><label class="form-label">${t('network.corr_method')}</label>
                <select class="form-select" id="wgcna-corr"><option>${t('network.corr_pearson')}</option><option>${t('network.corr_biweight')}</option></select></div>
              <div class="form-group"><label class="form-label">${t('network.net_type')}</label>
                <select class="form-select" id="wgcna-nettype"><option>${t('network.net_signed')}</option><option>${t('network.net_unsigned')}</option><option>${t('network.net_signed_hybrid')}</option></select></div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">${t('network.soft_thresh')}</label><input type="number" class="form-input" id="wgcna-thresh" value="6" min="1" max="30"><span class="form-hint">${t('network.soft_thresh_hint')}</span></div>
                <div class="form-group"><label class="form-label">${t('network.min_module')}</label><input type="number" class="form-input" id="wgcna-minmod" value="30" min="10"></div>
              </div>
              <div class="form-row">
                <div class="form-group"><label class="form-label">${t('network.merge_cut')}</label><input type="number" class="form-input" id="wgcna-mergecut" value="0.25" step="0.05" min="0" max="1"></div>
                <div class="form-group"><label class="form-label">${t('network.tom_type')}</label>
                  <select class="form-select" id="wgcna-tom"><option>${t('network.net_signed')}</option><option>${t('network.net_unsigned')}</option></select></div>
              </div>
              <div class="collapsible">
                <button class="collapsible-trigger" onclick="toggleCollapsible(this)">${t('common.advanced_options')} <i data-lucide="chevron-down"></i></button>
                <div class="collapsible-content"><div class="collapsible-body">
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-pam"> ${t('network.pam')}</label></div>
                  <div class="form-group"><label class="form-label">${t('network.deep_split')}</label>
                    <select class="form-select" id="wgcna-deepsplit"><option value="0">0</option><option value="1">1</option><option value="2" selected>${t('network.deep_default')}</option><option value="3">3</option><option value="4">4</option></select></div>
                  <div class="form-group"><label class="form-checkbox"><input type="checkbox" id="wgcna-cytoscape"> ${t('network.cytoscape')}</label></div>
                </div></div>
              </div>
            </div>
            <div class="panel-footer">
              <button class="btn btn-secondary btn-sm"><i data-lucide="zap"></i> ${t('network.pick_threshold')}</button>
              <button class="btn btn-primary btn-sm" onclick="runModule('network')" style="background:var(--mod-green);border-color:var(--mod-green)"><i data-lucide="play"></i> ${t('network.run_wgcna')}</button>
            </div>
            ${renderLogPanel('network')}
          </div>
        </div>
        <div>
          <div class="module-panel animate-slide-up" style="animation-delay:220ms">
            <div class="panel-header"><span class="panel-title">${t('network.results')}</span></div>
            <div class="panel-body">
              <div class="tabs">
                <div class="tab active" data-tab="wgcna-modules">${t('network.tab_modules')}</div>
                <div class="tab" data-tab="wgcna-trait">${t('network.tab_trait')}</div>
                <div class="tab" data-tab="wgcna-custom">${t('network.tab_custom')}</div>
                <div class="tab" data-tab="wgcna-log">${t('qc.tab_log')}</div>
              </div>
              <div class="tab-content active" data-tab="wgcna-modules">
                <div class="results-summary">
                  <div class="result-metric"><div class="result-metric-value">5,000</div><div class="result-metric-label">${t('network.metric_genes')}</div></div>
                  <div class="result-metric"><div class="result-metric-value" style="color:var(--mod-green)">12</div><div class="result-metric-label">${t('network.metric_modules')}</div></div>
                  <div class="result-metric"><div class="result-metric-value">R²=0.87</div><div class="result-metric-label">${t('network.metric_fit')}</div></div>
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
    const nameKey = navKey(mod.id);
    return `
      <div class="card animate-slide-up" style="animation-delay:100ms">
        <div class="empty-state" style="padding:64px 24px">
          <div class="empty-state-icon"><i data-lucide="${mod.icon}"></i></div>
          <h3 class="empty-state-title">${t(nameKey)}</h3>
          <p class="empty-state-text">${t('module.soon_body', { tool: `<strong>${mod.tool}</strong>` })}</p>
          <div style="margin-top:20px"><span class="badge badge-muted" style="font-size:0.8rem;padding:6px 14px">${t('badge.in_development')}</span></div>
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
    const rows = statuses.map(s => {
      const available = s.configured_path || s.bundled_path || s.detected_on_path;
      return `
      <tr>
        <td>${s.display_name}</td>
        <td class="path">${s.configured_path ? escapeHtml(s.configured_path) : `<em>${t('settings.not_set')}</em>`}</td>
        <td class="path">${s.bundled_path ? escapeHtml(s.bundled_path) : `<em>${t('settings.not_bundled')}</em>`}</td>
        <td class="path">${s.detected_on_path ? escapeHtml(s.detected_on_path) : `<em>${t('settings.not_on_path')}</em>`}</td>
        <td>${available ? `<span class="ok">${t('settings.ok')}</span>` : `<span class="warn">${t('settings.missing')}</span>`}</td>
        <td>
          <button data-act="browse" data-id="${s.id}">${t('common.browse')}</button>
          ${s.configured_path ? `<button data-act="clear" data-id="${s.id}">${t('settings.clear')}</button>` : ''}
        </td>
      </tr>
    `;
    }).join('');
    const cur = window.I18N ? window.I18N.getLang() : 'en';
    return `
      <h2>${t('settings.binary_title')}</h2>
      <p>${t('settings.binary_intro_html')}</p>
      <table class="settings-table">
        <thead><tr>
          <th>${t('settings.col_tool')}</th>
          <th>${t('settings.col_configured')}</th>
          <th>${t('settings.col_bundled')}</th>
          <th>${t('settings.col_path')}</th>
          <th>${t('settings.col_status')}</th>
          <th>${t('settings.col_actions')}</th>
        </tr></thead>
        <tbody>${rows}</tbody>
      </table>

      <h2 style="margin-top:32px">${t('settings.language_section_full')}</h2>
      <div class="settings-language">
        <label><input type="radio" name="lang-choice" value="en" ${cur === 'en' ? 'checked' : ''}> ${t('settings.language_en')}</label>
        <label style="margin-left:16px"><input type="radio" name="lang-choice" value="zh" ${cur === 'zh' ? 'checked' : ''}> ${t('settings.language_zh')}</label>
      </div>
    `;
  }


  // ── Charts ─────────────────────────────────────────────────
  // ── Per-module result HTML dispatcher ─────────────────────
  function renderRunResultHtml(moduleId, result, runId) {
    let html = '';
    switch (moduleId) {
      case 'gff_convert': html = renderGffConvertResult(result, runId); break;
      case 'star_align': html = renderStarAlignResult(result, runId); break;
      case 'star_index': html = `<pre>${escapeHtml(JSON.stringify(result.summary, null, 2))}</pre>`; break;
      default: html = `<pre>${escapeHtml(JSON.stringify(result, null, 2))}</pre>`; break;
    }
    return html;
  }

  async function loadRunsForView(moduleId, containerId) {
    const container = document.getElementById(containerId);
    if (!container) return;
    try {
      const runs = await window.__TAURI__.core.invoke('list_runs', { moduleId });
      if (!runs || runs.length === 0) {
        container.innerHTML = '<p><em>No runs yet.</em></p>';
        return;
      }
      container.innerHTML = runs.map(run => {
        const status = run.status || 'unknown';
        const ts = run.finished_at || run.started_at || '';
        const resultHtml = (status === 'Done' && run.result)
          ? renderRunResultHtml(moduleId, run.result, run.id)
          : `<p><em>Status: ${status}</em></p>`;
        return `<details open><summary>Run ${run.id} &mdash; ${status} ${ts ? '(' + ts + ')' : ''}</summary>${resultHtml}</details>`;
      }).join('');
    } catch (err) {
      container.innerHTML = `<p><em>Could not load runs: ${err}</em></p>`;
    }
  }

  function initChartsForView(view) {
    switch (view) {
      case 'qc':           renderQCCharts(); break;
      case 'trimming':     renderTrimmingCharts(); break;
      case 'differential': renderDESeq2Charts(); break;
      case 'network':      renderWGCNACharts(); break;
      case 'gff-convert':  loadRunsForView('gff_convert', 'gff-convert-runs'); break;
      case 'star-align':   loadRunsForView('star_align', 'star-align-runs'); break;
      case 'star-index':   loadRunsForView('star_index', 'star-index-runs'); break;
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

    // STAR Alignment: form submit
    document.addEventListener('submit', (e) => {
      if (e.target.id === 'form-star-align') { e.preventDefault(); submitStarAlign(e.target); }
    });

    // GFF Convert: form submit
    document.addEventListener('submit', (e) => {
      if (e.target.id === 'form-gff-convert') { e.preventDefault(); submitGffConvert(e.target); }
    });

    // DESeq2 handoff button
    document.addEventListener('click', (e) => {
      const btn = e.target.closest('[id^="star-to-deseq"]');
      if (!btn) return;
      state.prefill = state.prefill || {};
      state.prefill.differential = { counts_matrix: btn.dataset.matrix };
      navigate('differential');
    });

    // GFF Convert → STAR Index handoff button
    document.addEventListener('click', (e) => {
      const gffBtn = e.target.closest('[data-gff-use-in-star]');
      if (gffBtn) {
        state.prefill = state.prefill || {};
        state.prefill.star_index = { gtf_file: gffBtn.dataset.gffUseInStar };
        location.hash = '#star-index';
      }
    });

    // Generic file-pick handler for data-pick-for buttons (supports data-pick-mode: file|multi|dir)
    document.addEventListener('click', async (e) => {
      const btn = e.target.closest('[data-pick-for]');
      if (!btn) return;
      const mode = btn.dataset.pickMode || 'file';
      let picked;
      if (mode === 'dir') {
        picked = await window.__TAURI__.core.invoke('select_directory');
      } else {
        picked = await window.__TAURI__.core.invoke('select_files', { multiple: mode === 'multi' });
      }
      const field = btn.dataset.pickFor;
      const input = btn.parentElement.querySelector(`[name="${field}"]`);
      if (!input) return;
      if (Array.isArray(picked)) input.value = picked.join(' ');
      else if (picked) input.value = picked;
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

    // Language toggle (header buttons)
    document.querySelectorAll('.lang-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        if (window.I18N) window.I18N.setLang(btn.dataset.lang);
      });
    });

    // Settings: language radio
    document.addEventListener('change', (e) => {
      const r = e.target.closest('input[name="lang-choice"]');
      if (r && window.I18N) window.I18N.setLang(r.value);
    });

    const syncLangButtons = () => {
      const cur = window.I18N ? window.I18N.getLang() : 'en';
      document.querySelectorAll('.lang-btn').forEach(b => {
        b.classList.toggle('active', b.dataset.lang === cur);
      });
    };
    syncLangButtons();

    // Re-render dynamic content when language changes
    window.addEventListener('langchange', () => {
      syncLangButtons();
      navigate(state.currentView);
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
    const displayName = mod ? t(navKey(mod.id)) : id;
    st.textContent = `${t('status.running_prefix')} ${displayName}…`;
    js.textContent = t('status.one_job');
    const badge = document.querySelector(`.nav-item[data-view="${id}"] .nav-badge`);
    if (badge) { badge.className = 'nav-badge running'; badge.textContent = t('badge.running'); }

    const params = collectModuleParams(id);
    try {
      await api.invoke('validate_params', { moduleId: id, params });
      const runId = await api.invoke('run_module', { moduleId: id, params });
      if (runId) state.runIdToModule[runId] = id;
    } catch (err) {
      console.warn(`[runModule] invoke failed for ${id}:`, err);
    }

    st.textContent = t('status.ready'); js.textContent = t('status.no_jobs');
    if (badge) { badge.className = 'nav-badge done'; badge.textContent = t('badge.done'); }
  };

  window.resetForm = function (id) { state.files[id] = []; navigate(id); };


  // --- run-log streaming support (shared across all modules) ---
  const LOG_BUFFER_MAX = 500;
  state.logsByRun = state.logsByRun || {};
  state.runIdToModule = state.runIdToModule || {};

  function appendRunLog(runId, line, stream) {
    const panelKey = state.runIdToModule[runId] || runId;
    const buf = (state.logsByRun[panelKey] = state.logsByRun[panelKey] || []);
    buf.push({ line, stream });
    while (buf.length > LOG_BUFFER_MAX) buf.shift();
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
    <summary>${t('common.log_panel')}</summary>
    <pre>${escapeHtml(text)}</pre>
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
    if (window.I18N) window.I18N.applyI18n(document);
    setupEvents();

    // Tauri event listeners
    api.listen('run-progress', event => {
      const st = document.getElementById('statusText');
      const log = document.querySelector('.log-output');
      if (st) st.textContent = event.payload?.message || (t('status.running_prefix') + '…');
      if (log) log.innerHTML += `\n<span class="log-info">[INFO]</span> ${event.payload?.message || ''}`;
    });
    api.listen('run-completed', event => {
      const st = document.getElementById('statusText');
      const js = document.getElementById('jobStatus');
      if (st) st.textContent = t('status.ready');
      if (js) js.textContent = t('status.no_jobs');
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
