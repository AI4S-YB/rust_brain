import { VirtualList } from './virtual-list.js';
import { colorSeq, colorQual } from './coloring.js';
import { t } from '../../core/i18n-helpers.js';

const _tauri = typeof window !== 'undefined' ? window.__TAURI__?.core?.invoke : undefined;
const api = _tauri
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

export function renderFastqViewerView(content) {
  content.innerHTML = `
    <div class="module-view fastq-viewer">
      <header class="fastq-toolbar">
        <div class="fastq-title">
          <div class="fastq-title-icon"><i data-lucide="file-search"></i></div>
          <div>
            <h1>${t('nav.fastq_viewer')}</h1>
            <div class="fastq-meta">
              <span class="fastq-path" title="${t('utility.fastq_viewer.no_file_open')}">${t('utility.fastq_viewer.no_file_open')}</span>
              <span class="fastq-count-wrap"><span class="fastq-count">—</span> ${t('utility.fastq_viewer.records')}</span>
            </div>
          </div>
        </div>
        <div class="fastq-controls">
          <button class="btn btn-primary fastq-open" data-act="fastq-open"><i data-lucide="folder-open"></i> ${t('utility.fastq_viewer.open_file')}</button>
          <label class="fastq-field">${t('utility.fastq_viewer.record')} <input type="number" class="fastq-jump" min="0" value="0"></label>
          <label class="fastq-field fastq-range">% <input type="range" class="fastq-pct" min="0" max="100" value="0"></label>
          <div class="fastq-search-group">
            <i data-lucide="search" class="fastq-search-icon"></i>
            <input type="text" class="fastq-search" placeholder="${t('utility.fastq_viewer.search_placeholder')}">
            <button class="btn btn-secondary" data-act="fastq-search-next"><i data-lucide="skip-forward"></i> ${t('utility.fastq_viewer.find_next')}</button>
          </div>
        </div>
      </header>
      <div class="fastq-list"></div>
    </div>
  `;

  const host = content.querySelector('.fastq-list');
  const pathEl = content.querySelector('.fastq-path');
  const countEl = content.querySelector('.fastq-count');
  const jumpEl = content.querySelector('.fastq-jump');
  const pctEl = content.querySelector('.fastq-pct');
  const searchEl = content.querySelector('.fastq-search');

  const state = { total: 0, searchCursor: 0 };

  const list = new VirtualList({
    host,
    recordHeight: 112,
    overscan: 10,
    fetchBatch: (start, count) => api('fastq_viewer_read_records', { startRecord: start, count }),
    renderRecord: (rec, i) => renderRecordEl(rec, i),
  });

  async function openFile() {
    const path = await api('select_files', { multiple: false });
    if (!path || !path[0]) return;
    pathEl.textContent = path[0];
    pathEl.title = path[0];
    const res = await api('fastq_viewer_open', { path: path[0] });
    state.total = res.total_records;
    countEl.textContent = res.total_records.toLocaleString();
    list.setTotal(res.total_records);
    jumpEl.max = res.total_records - 1;
  }

  content.addEventListener('click', async (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'fastq-open') openFile();
    if (act === 'fastq-search-next') {
      const q = searchEl.value.trim();
      if (!q) return;
      const hits = await api('fastq_viewer_search_id', {
        query: q, fromRecord: state.searchCursor, limit: 1,
      });
      if (hits.length) {
        list.scrollToIndex(hits[0].record_n);
        state.searchCursor = hits[0].record_n + 1;
      } else {
        state.searchCursor = 0; // wrap
      }
    }
  });

  jumpEl.addEventListener('change', () => list.scrollToIndex(Number(jumpEl.value)));
  pctEl.addEventListener('change', async () => {
    const n = await api('fastq_viewer_seek_percent', { pct: Number(pctEl.value) / 100 });
    list.scrollToIndex(n);
  });
}

function renderRecordEl(rec, i) {
  const el = document.createElement('div');
  el.className = 'fastq-record';
  el.innerHTML = `
    <div class="fastq-record-line fastq-record-idline">
      <span class="fastq-record-label">ID</span>
      <span><span class="fastq-record-index">#${i}</span> <span class="fastq-record-id">${escapeHtml(rec.id)}</span></span>
    </div>
    <div class="fastq-record-line">
      <span class="fastq-record-label">SEQ</span>
      <span class="fastq-record-seq seq">${colorSeq(rec.seq)}</span>
    </div>
    <div class="fastq-record-line fastq-record-plus">
      <span class="fastq-record-label">+</span>
      <span>${escapeHtml(rec.plus)}</span>
    </div>
    <div class="fastq-record-line">
      <span class="fastq-record-label">QUAL</span>
      <span class="fastq-record-qual qual">${colorQual(rec.qual)}</span>
    </div>
  `;
  return el;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
