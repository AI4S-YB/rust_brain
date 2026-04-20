import { MODULES, COLOR_MAP } from '../../core/constants.js';
import { state } from '../../core/state.js';
import { t, navKey } from '../../core/i18n-helpers.js';

export function renderDashboardView(container) {
  container.innerHTML = renderDashboardHtml();
}

function renderDashboardHtml() {
  const pipelineModules = MODULES.filter(m => !m.utility);
  const pipelineNodes = pipelineModules.map((m, i) => {
    const connector = i < pipelineModules.length - 1
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
          <span class="badge badge-teal">${t('dashboard.modules_badge', { n: pipelineModules.length })}</span>
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
            ${renderToolInfo('STAR_rs', t('dashboard.tool_desc.star'), 'MIT')}
            ${renderToolInfo('gffread_rs', t('dashboard.tool_desc.gffread'), 'MIT')}
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
