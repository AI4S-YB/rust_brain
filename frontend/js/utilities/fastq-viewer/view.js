import { VirtualList } from './virtual-list.js';
import { colorSeq, colorQual } from './coloring.js';
import { t } from '../../core/i18n-helpers.js';

const _tauri = typeof window !== 'undefined' ? window.__TAURI__?.core?.invoke : undefined;
const api = _tauri
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

const BATCH_SIZE = 1000;

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
              <span class="fastq-count-wrap"><span class="fastq-loaded">0</span> ${t('utility.fastq_viewer.loaded')}</span>
              <span class="fastq-bytes"></span>
              <span class="fastq-eof" hidden>EOF</span>
            </div>
          </div>
        </div>
        <div class="fastq-controls">
          <button class="btn btn-primary fastq-open" data-act="fastq-open"><i data-lucide="folder-open"></i> ${t('utility.fastq_viewer.open_file')}</button>
          <button class="btn btn-secondary" data-act="fastq-load-more"><i data-lucide="chevrons-down"></i> ${t('utility.fastq_viewer.load_more')}</button>
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
  const loadedEl = content.querySelector('.fastq-loaded');
  const bytesEl = content.querySelector('.fastq-bytes');
  const eofEl = content.querySelector('.fastq-eof');
  const searchEl = content.querySelector('.fastq-search');

  const state = {
    loaded: 0,
    totalBytes: 0,
    eof: false,
    searchCursor: 0,
    inflightLoadMore: false,
  };

  function formatBytes(n) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  function applyStatus(s) {
    state.loaded = s.loaded_records;
    state.eof = s.eof;
    loadedEl.textContent = s.loaded_records.toLocaleString();
    if (s.total_bytes > 0) {
      const pct = Math.min(100, Math.floor((s.bytes_read / s.total_bytes) * 100));
      bytesEl.textContent = `· ${formatBytes(s.bytes_read)} / ${formatBytes(s.total_bytes)} (${pct}%)`;
    } else {
      bytesEl.textContent = `· ${formatBytes(s.bytes_read)}`;
    }
    eofEl.hidden = !s.eof;
    list.setTotal(s.loaded_records);
  }

  const list = new VirtualList({
    host,
    recordHeight: 112,
    overscan: 10,
    fetchBatch: async (start, count) => {
      const res = await api('fastq_viewer_read', { from: start, count });
      applyStatus({
        loaded_records: res.loaded_records,
        bytes_read: res.bytes_read,
        total_bytes: state.totalBytes,
        eof: res.eof,
      });
      maybeAutoLoadMore();
      return res.records;
    },
    renderRecord: (rec, i) => renderRecordEl(rec, i),
  });

  // Auto-load more when user scrolls to near the tail of the loaded window.
  host.addEventListener('scroll', maybeAutoLoadMore);

  async function maybeAutoLoadMore() {
    if (state.eof || state.inflightLoadMore) return;
    const total = state.loaded;
    if (total === 0) return;
    const firstVisible = Math.floor(host.scrollTop / 112);
    const lastVisible = Math.ceil((host.scrollTop + host.clientHeight) / 112);
    // Within 50 records of the tail → preload the next batch.
    if (lastVisible >= total - 50) {
      await loadMore();
    }
  }

  async function loadMore() {
    if (state.eof || state.inflightLoadMore) return;
    state.inflightLoadMore = true;
    try {
      const res = await api('fastq_viewer_read', {
        from: state.loaded,
        count: BATCH_SIZE,
      });
      applyStatus({
        loaded_records: res.loaded_records,
        bytes_read: res.bytes_read,
        total_bytes: state.totalBytes,
        eof: res.eof,
      });
    } finally {
      state.inflightLoadMore = false;
    }
  }

  async function openFile() {
    const paths = await api('select_files', { multiple: false });
    if (!paths || !paths[0]) return;
    pathEl.textContent = paths[0];
    pathEl.title = paths[0];
    const info = await api('fastq_viewer_open', { path: paths[0] });
    state.totalBytes = info.total_bytes;
    state.loaded = 0;
    state.eof = false;
    state.searchCursor = 0;
    list.setTotal(0);
    // Pull the first batch so the UI isn't empty.
    await loadMore();
  }

  if (content._fastqViewerClickHandler) {
    content.removeEventListener('click', content._fastqViewerClickHandler);
  }
  content._fastqViewerClickHandler = async (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'fastq-open') openFile();
    if (act === 'fastq-load-more') loadMore();
    if (act === 'fastq-search-next') {
      const q = searchEl.value.trim();
      if (!q) return;
      const res = await api('fastq_viewer_search_id', {
        query: q,
        from: state.searchCursor,
        limit: 1,
      });
      if (res.hits.length) {
        const n = res.hits[0].record_n;
        state.searchCursor = n + 1;
        // Ensure the record is loaded, then scroll to it.
        await list.fetchBatch(n, 1);
        list.scrollToIndex(n);
      } else if (res.eof) {
        state.searchCursor = 0;
      } else {
        state.searchCursor = res.scanned_through;
      }
    }
  };
  content.addEventListener('click', content._fastqViewerClickHandler);
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
