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
      <header class="utility-header">
        <h1>${t('nav.fastq_viewer')}</h1>
        <div class="utility-controls">
          <button class="btn" data-act="fastq-open">${t('utility.fastq_viewer.open_file')}</button>
          <label>${t('utility.fastq_viewer.record')} <input type="number" class="fastq-jump" min="0" value="0" style="width:100px"></label>
          <label>% <input type="range" class="fastq-pct" min="0" max="100" value="0" style="width:120px"></label>
          <input type="text" class="fastq-search" placeholder="${t('utility.fastq_viewer.search_placeholder')}" style="width:200px">
          <button class="btn" data-act="fastq-search-next">${t('utility.fastq_viewer.find_next')}</button>
        </div>
        <div class="utility-meta"><span class="fastq-path">${t('utility.fastq_viewer.no_file_open')}</span> · <span class="fastq-count">—</span> ${t('utility.fastq_viewer.records')}</div>
      </header>
      <div class="fastq-list" style="height:70vh;border:1px solid #e7e5e4;border-radius:6px;background:#faf8f4"></div>
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
    recordHeight: 88,  // 4 text lines × ~22 px
    overscan: 10,
    fetchBatch: (start, count) => api('fastq_viewer_read_records', { startRecord: start, count }),
    renderRecord: (rec, i) => renderRecordEl(rec, i),
  });

  async function openFile() {
    const path = await api('select_files', { multiple: false });
    if (!path || !path[0]) return;
    pathEl.textContent = path[0];
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
  el.style.padding = '4px 8px';
  el.style.fontFamily = 'monospace';
  el.style.fontSize = '13px';
  el.style.borderBottom = '1px solid #f1ede7';
  el.innerHTML = `
    <div style="color:#5c7080">#${i} ${escapeHtml(rec.id)}</div>
    <div class="seq">${colorSeq(rec.seq)}</div>
    <div style="color:#a8a29e">${escapeHtml(rec.plus)}</div>
    <div class="qual">${colorQual(rec.qual)}</div>
  `;
  return el;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
