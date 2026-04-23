import { state } from './state.js';
import { KNOWN_VIEWS, MODULES } from './constants.js';
import { t, navKey } from './i18n-helpers.js';
import { renderComingSoon, renderEmptyState } from '../ui/coming-soon.js';
import { renderDashboardView } from '../modules/dashboard/view.js';
import { loadRunsForView } from '../modules/run-result.js';
import { syncRunButtons } from './run-controls.js';

export async function navigate(view) {
  state.currentView = view;

  const navMatchView = view.startsWith('chat/') ? 'chat' : view;
  document.querySelectorAll('.nav-item').forEach(el => {
    el.classList.toggle('active', el.dataset.view === navMatchView);
  });

  const bc = document.getElementById('breadcrumb');
  const bcKey = navMatchView;
  const label = KNOWN_VIEWS.has(bcKey) ? t(navKey(bcKey)) : bcKey;
  if (bc) {
    bc.innerHTML = `
      <span class="breadcrumb-home">${t('brand.name')}</span>
      <i data-lucide="chevron-right" class="breadcrumb-sep"></i>
      <span class="breadcrumb-current">${label}</span>
    `;
  }

  const content = document.getElementById('content');
  if (!content) return;
  content.scrollTop = 0;

  if (view === 'dashboard') {
    renderDashboardView(content);
  } else if (view === 'settings') {
    content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
    const m = await import('../modules/settings/view.js');
    if (state.currentView === 'settings') await m.renderSettingsView(content);
  } else if (view === 'gff-convert') {
    const m = await import('../modules/gff-convert/view.js');
    if (state.currentView === view) m.renderGffConvertView(content);
  } else if (view === 'star-index') {
    const m = await import('../modules/star-index/view.js');
    if (state.currentView === view) m.renderStarIndexView(content);
  } else if (view === 'star-align') {
    const m = await import('../modules/star-align/view.js');
    if (state.currentView === view) m.renderStarAlignView(content);
  } else if (view === 'rustqc') {
    const m = await import('../modules/rustqc/view.js');
    if (state.currentView === view) m.renderRustqcView(content);
  } else if (view === 'plots') {
    const m = await import('../modules/plots/view.js');
    if (state.currentView === view) m.renderPlotsView(content);
  } else if (view === 'chat' || view.startsWith('chat/')) {
    content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
    const sessionId = view.startsWith('chat/') ? view.slice('chat/'.length) : null;
    if (sessionId) {
      const m = await import('../modules/chat/chat-view.js');
      if (state.currentView === view) m.renderChatView(content, sessionId);
    } else {
      const m = await import('../modules/chat/session-list.js');
      if (state.currentView === view) m.renderSessionListPage(content);
    }
  } else if (view === 'genome-viewer') {
    content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
    const m = await import('../utilities/genome-viewer/view.js');
    if (state.currentView === view) m.renderGenomeViewerView(content);
  } else if (view === 'fastq-viewer') {
    content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
    const m = await import('../utilities/fastq-viewer/view.js');
    if (state.currentView === view) m.renderFastqViewerView(content);
  } else {
    await renderModuleView(content, view);
  }

  syncRunButtons(content);
  if (window.lucide) window.lucide.createIcons();
  requestAnimationFrame(() => initChartsForView(view));
}

async function renderModuleView(content, moduleId) {
  const mod = MODULES.find(m => m.id === moduleId || m.view_id === moduleId);
  if (!mod) {
    content.innerHTML = `<div class="module-view">${renderEmptyState(t('common.module_not_found'))}</div>`;
    return;
  }
  if (mod.status === 'soon') {
    const { renderModuleHeader } = await import('../modules/module-header.js');
    content.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderComingSoon(mod)}</div>`;
    return;
  }
  if (mod.has_native_view === false) {
    const m = await import('../modules/plugin/view.js');
    if (state.currentView === moduleId) await m.renderPluginView(content, moduleId);
    return;
  }

  switch (moduleId) {
    case 'qc': {
      const m = await import('../modules/qc/view.js');
      if (state.currentView === moduleId) m.renderQCView(content);
      break;
    }
    case 'rustqc': {
      const m = await import('../modules/rustqc/view.js');
      if (state.currentView === moduleId) m.renderRustqcView(content);
      break;
    }
    case 'trimming': {
      const m = await import('../modules/trimming/view.js');
      if (state.currentView === moduleId) m.renderTrimmingView(content);
      break;
    }
    case 'differential': {
      const m = await import('../modules/differential/view.js');
      if (state.currentView === moduleId) m.renderDifferentialView(content);
      break;
    }
    case 'network': {
      const m = await import('../modules/network/view.js');
      if (state.currentView === moduleId) m.renderNetworkView(content);
      break;
    }
    default: {
      const { renderModuleHeader } = await import('../modules/module-header.js');
      content.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderComingSoon(mod)}</div>`;
    }
  }
}

async function initChartsForView(view) {
  switch (view) {
    case 'qc':           loadRunsForView('qc', 'qc-runs'); break;
    case 'rustqc':       loadRunsForView('rustqc', 'rustqc-runs'); break;
    case 'trimming':     loadRunsForView('trimming', 'trimming-runs'); break;
    case 'differential': loadRunsForView('deseq2', 'differential-runs'); break;
    case 'network':      loadRunsForView('wgcna', 'network-runs'); break;
    case 'gff-convert':  loadRunsForView('gff_convert', 'gff-convert-runs'); break;
    case 'star-align':   loadRunsForView('star_align', 'star-align-runs'); break;
    case 'star-index':   loadRunsForView('star_index', 'star-index-runs'); break;
  }
}

export { initChartsForView };
