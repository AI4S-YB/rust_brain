import { state } from './state.js';
import { navigate, initChartsForView } from './router.js';
import { t, setLang, getLang } from './i18n-helpers.js';
import { api } from './tauri.js';
import { handleFileDrop } from '../ui/file-drop.js';
import { submitStarIndex } from '../modules/star-index/run.js';
import { submitStarAlign } from '../modules/star-align/run.js';
import { submitGffConvert } from '../modules/gff-convert/run.js';
import { binaryApi } from '../api/binary.js';
import { filesApi } from '../api/files.js';
import { alertModal } from '../ui/modal.js';
import { projectNew, projectOpen } from '../modules/dashboard/project.js';
import { runModule, resetForm } from './actions.js';
import { cancelModuleRun } from './run-controls.js';
import { deleteRunWithConfirm } from '../modules/run-result.js';
import { inputsApi } from '../api/inputs.js';
import { invalidatePickerCache } from '../ui/registry-picker.js';
import { toggleCollapsible } from '../ui/collapsible.js';
import { exportTableAsTSV } from '../ui/export-tsv.js';
import { setFontSize } from './font-size.js';

export function setupEvents() {
  document.addEventListener('click', e => {
    const act = e.target.closest('[data-act]');
    if (act && dispatchAction(act, e)) return;

    const nav = e.target.closest('[data-view]');
    if (nav) {
      e.preventDefault();
      navigate(nav.dataset.view);
      document.getElementById('sidebar')?.classList.remove('open');
      return;
    }

    const tab = e.target.closest('.tab');
    if (tab && tab.dataset.tab) {
      const box = tab.closest('[data-tab-scope]') || tab.closest('.panel-body') || tab.closest('.module-panel');
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

  setupProjectMenu();

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
      api.invoke('select_files', { accept: z.dataset.accept || '*' })
        .then(files => {
          if (files && Array.isArray(files) && files.length > 0) {
            handleFileDrop(z, files.map(f => ({ name: f.split(/[\\/]/).pop(), size: 0, path: f })));
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

  document.addEventListener('submit', (e) => {
    if (e.target.id === 'form-star-index') { e.preventDefault(); submitStarIndex(e.target); }
    else if (e.target.id === 'form-star-align') { e.preventDefault(); submitStarAlign(e.target); }
    else if (e.target.id === 'form-gff-convert') { e.preventDefault(); submitGffConvert(e.target); }
  });

  document.addEventListener('click', (e) => {
    const btn = e.target.closest('[id^="star-to-deseq"]');
    if (!btn) return;
    state.prefill = state.prefill || {};
    state.prefill.differential = { counts_matrix: btn.dataset.matrix };
    navigate('differential');
  });

  document.addEventListener('click', (e) => {
    const gffBtn = e.target.closest('[data-gff-use-in-star]');
    if (gffBtn) {
      state.prefill = state.prefill || {};
      state.prefill.star_index = { gtf_file: gffBtn.dataset.gffUseInStar };
      location.hash = '#star-index';
    }
  });

  document.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-pick-for]');
    if (!btn) return;
    const mode = btn.dataset.pickMode || 'file';
    let picked;
    if (mode === 'dir') {
      picked = await filesApi.selectDirectory();
    } else {
      picked = await filesApi.selectFiles({ multiple: mode === 'multi' });
    }
    const field = btn.dataset.pickFor;
    const input = btn.parentElement.querySelector(`[name="${field}"]`);
    if (!input) return;
    if (Array.isArray(picked)) input.value = picked.join(' ');
    else if (picked) input.value = picked;
    input.title = input.value;

    // P4 enhancement: picked file(s) are also auto-registered as project Inputs
    // so they show up in the registry picker next time. Dir picks are skipped
    // (directories aren't a registerable Input kind). Failures are silent —
    // this is a best-effort hook, the form path still works on its own.
    if (mode !== 'dir') {
      const paths = Array.isArray(picked) ? picked : (picked ? [picked] : []);
      let anyOk = false;
      for (const p of paths) {
        try {
          await inputsApi.register(p);
          anyOk = true;
        } catch { /* silent */ }
      }
      if (anyOk) {
        // Next time a picker is opened (on view re-render or interaction),
        // it will include the freshly-registered file. We deliberately don't
        // re-render open pickers here to avoid resetting an existing selection.
        invalidatePickerCache();
      }
    }
  });

  document.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-act="browse"]');
    if (btn) {
      const picked = await filesApi.selectFiles({ multiple: false });
      if (picked && picked[0]) {
        try {
          await binaryApi.setPath(btn.dataset.id, picked[0]);
          navigate('settings');
        } catch (err) { alertModal({ title: 'Error', message: 'Failed: ' + err }); }
      }
      return;
    }
    const clr = e.target.closest('[data-act="clear"]');
    if (clr) {
      try {
        await binaryApi.clearPath(clr.dataset.id);
        navigate('settings');
      } catch (err) { alertModal({ title: 'Error', message: 'Failed: ' + err }); }
    }
  });

  document.querySelectorAll('.lang-btn').forEach(btn => {
    btn.addEventListener('click', () => setLang(btn.dataset.lang));
  });

  document.addEventListener('change', (e) => {
    const r = e.target.closest('input[name="lang-choice"]');
    if (r) setLang(r.value);
    const f = e.target.closest('input[name="font-size-choice"]');
    if (f) setFontSize(f.value);
  });

  const syncLangButtons = () => {
    const cur = getLang();
    document.querySelectorAll('.lang-btn').forEach(b => {
      b.classList.toggle('active', b.dataset.lang === cur);
    });
  };
  syncLangButtons();

  window.addEventListener('langchange', () => {
    syncLangButtons();
    navigate(state.currentView);
  });
}

function dispatchAction(el, event) {
  switch (el.dataset.act) {
    case 'run-module':
      runModule(el.dataset.mod);
      return true;
    case 'cancel-run':
      cancelModuleRun(el.dataset.mod);
      return true;
    case 'reset-form':
      resetForm(el.dataset.mod);
      return true;
    case 'collapsible-toggle':
      toggleCollapsible(el);
      return true;
    case 'export-tsv':
      exportTableAsTSV(el.dataset.table, el.dataset.filename);
      return true;
    case 'goto-settings':
      navigate('settings');
      return true;
    case 'delete-run': {
      // The delete button lives inside <summary>; prevent the default toggle
      // so clicking it doesn't also collapse the details panel.
      event?.preventDefault();
      event?.stopPropagation();
      const runId = el.dataset.runId;
      const container = el.closest('[data-runs-module-id]');
      deleteRunWithConfirm(runId, container);
      return true;
    }
    case 'reload-plugins': {
      el.disabled = true;
      import('../api/plugins.js').then(m => m.pluginsApi.reload())
        .then(() => navigate('settings'))
        .catch(err => alertModal({ title: t('status.error_prefix'), message: String(err) }))
        .finally(() => { el.disabled = false; });
      return true;
    }
    default:
      // 'browse' / 'clear' (settings) and 'project-new'/'project-open' (menu)
      // are handled by their own targeted listeners elsewhere.
      return false;
  }
}

function setupProjectMenu() {
  const btn = document.getElementById('projectSelectorBtn');
  const menu = document.getElementById('projectMenu');
  const wrap = btn?.closest('.project-selector-wrap');
  if (!btn || !menu || !wrap) return;
  const close = () => {
    menu.hidden = true;
    wrap.classList.remove('open');
    btn.setAttribute('aria-expanded', 'false');
  };
  btn.addEventListener('click', e => {
    e.stopPropagation();
    const open = menu.hidden;
    menu.hidden = !open;
    wrap.classList.toggle('open', open);
    btn.setAttribute('aria-expanded', String(open));
  });
  menu.addEventListener('click', e => {
    const item = e.target.closest('.project-menu-item');
    if (!item) return;
    e.stopPropagation();
    close();
    const act = item.dataset.act;
    if (act === 'project-new') projectNew();
    else if (act === 'project-open') projectOpen();
  });
  document.addEventListener('click', e => {
    if (menu.hidden) return;
    if (!wrap.contains(e.target)) close();
  });
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape' && !menu.hidden) close();
  });
}
