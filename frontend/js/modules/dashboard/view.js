import { MODULES, COLOR_MAP } from '../../core/constants.js';
import { t, navKey } from '../../core/i18n-helpers.js';
import { projectApi } from '../../api/project.js';
import { projectOpenFromPath } from './project.js';
import { escapeHtml } from '../../ui/escape.js';
import { inputsApi } from '../../api/inputs.js';
import { samplesApi } from '../../api/samples.js';
import { assetsApi } from '../../api/assets.js';
import { modulesApi } from '../../api/modules.js';
import { formatBytes } from '../run-result.js';

export function renderDashboardView(container) {
  container.innerHTML = renderDashboardHtml();
  populateRecentProjects();
  populateProjectOverview();
}

async function populateProjectOverview() {
  const card = document.getElementById('project-overview-card');
  if (!card) return;
  // Fire all four queries in parallel; tolerate "no project open" → hide card.
  const results = await Promise.allSettled([
    inputsApi.list(),
    samplesApi.list(),
    assetsApi.list(),
    modulesApi.listRuns(null),
  ]);
  if (results.every(r => r.status === 'rejected')) {
    card.hidden = true;
    return;
  }
  const inputs = results[0].status === 'fulfilled' ? results[0].value || [] : [];
  const samples = results[1].status === 'fulfilled' ? results[1].value || [] : [];
  const assets = results[2].status === 'fulfilled' ? results[2].value || [] : [];
  const runs = results[3].status === 'fulfilled' ? results[3].value || [] : [];

  const inputBytes = inputs.reduce((a, r) => a + (r.size_bytes || 0), 0);
  const assetBytes = assets.reduce((a, r) => a + (r.size_bytes || 0), 0);
  const missing = inputs.filter(r => r.missing).length;
  const runsByStatus = runs.reduce((acc, r) => {
    acc[r.status] = (acc[r.status] || 0) + 1;
    return acc;
  }, {});

  const tile = (icon, label, value, hint) => `
    <div class="overview-tile">
      <div class="overview-tile-icon"><i data-lucide="${icon}"></i></div>
      <div>
        <div class="overview-tile-value">${escapeHtml(String(value))}</div>
        <div class="overview-tile-label">${escapeHtml(label)}</div>
        ${hint ? `<div class="overview-tile-hint">${escapeHtml(hint)}</div>` : ''}
      </div>
    </div>`;

  card.innerHTML = `
    <div class="card-header" style="margin-bottom: 12px">
      <span class="card-title">
        <i data-lucide="bar-chart-3" style="width:15px;height:15px;vertical-align:-2px;margin-right:6px"></i>
        ${escapeHtml(t('dashboard.overview_title'))}
      </span>
    </div>
    <div class="overview-grid">
      ${tile('database', t('nav.inputs'),  inputs.length, missing ? t('dashboard.overview_missing', { n: missing }) : formatBytes(inputBytes))}
      ${tile('users',    t('nav.samples'), samples.length, samples.filter(s => s.paired).length + ' PE')}
      ${tile('package',  t('nav.assets'),  assets.length, formatBytes(assetBytes))}
      ${tile('list-checks', t('nav.tasks'), runs.length, [
        runsByStatus.Done     ? `${runsByStatus.Done} ✓`  : null,
        runsByStatus.Failed   ? `${runsByStatus.Failed} ✗` : null,
        runsByStatus.Running  ? `${runsByStatus.Running} ●` : null,
      ].filter(Boolean).join(' · '))}
    </div>
  `;
  if (window.lucide) window.lucide.createIcons();
}

async function populateRecentProjects() {
  const list = document.getElementById('recent-projects-list');
  if (!list) return;
  let paths = [];
  try { paths = await projectApi.listRecent() || []; }
  catch { paths = []; }
  if (!paths.length) {
    list.innerHTML = `<div class="recent-empty">${t('project.recent_empty')}</div>`;
    return;
  }
  list.innerHTML = paths.map(p => {
    const name = p.split(/[\\/]/).filter(Boolean).pop() || p;
    return `
      <button class="recent-project" data-recent-path="${escapeHtml(p)}" title="${escapeHtml(p)}">
        <i data-lucide="folder"></i>
        <span class="recent-project-name">${escapeHtml(name)}</span>
        <span class="recent-project-path">${escapeHtml(p)}</span>
        <i data-lucide="arrow-right" class="recent-project-arrow"></i>
      </button>`;
  }).join('');
  list.querySelectorAll('[data-recent-path]').forEach(btn => {
    btn.addEventListener('click', () => projectOpenFromPath(btn.dataset.recentPath));
  });
  if (window.lucide) window.lucide.createIcons();
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

      <div class="card animate-slide-up recent-projects-card" style="animation-delay: 40ms; margin-bottom: 16px; padding: 16px 24px;">
        <div class="card-header" style="margin-bottom: 12px">
          <span class="card-title"><i data-lucide="clock" style="width:15px;height:15px;vertical-align:-2px;margin-right:6px"></i>${t('project.recent_title')}</span>
          <span style="font-size:0.75rem;color:var(--text-muted);margin-left:auto">${t('project.recent_hint')}</span>
        </div>
        <div id="recent-projects-list" class="recent-projects-list">
          <div class="recent-empty">${t('common.loading')}</div>
        </div>
      </div>

      <div id="project-overview-card" class="card animate-slide-up project-overview-card" style="animation-delay: 55ms; margin-bottom: 16px; padding: 16px 24px;"></div>

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
