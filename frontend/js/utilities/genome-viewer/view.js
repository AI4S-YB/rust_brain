import igv from '/vendor/igv/igv.esm.min.js';
import { TauriReferenceReader, TauriFeatureReader } from './igv-adapter.js';
import { renderControls, renderTrackList } from './controls.js';

const api = (typeof window !== 'undefined' && window.__TAURI__?.core?.invoke)
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

const state = {
  browser: null,
  reference: null,        // { path, chroms: [...] }
  tracks: [],             // TrackMeta[]
  position: null,
  saveTimer: null,
};

export async function renderGenomeViewerView(content) {
  content.innerHTML = `<div class="module-view genome-viewer" style="padding:0"></div>`;
  const root = content.querySelector('.genome-viewer');
  renderControls(root);
  const browserHost = root.querySelector('.gv-browser');
  const trackListHost = root.querySelector('.gv-track-list');

  root.addEventListener('click', (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'gv-load-reference') loadReference();
    if (act === 'gv-add-track') addTrack();
    if (act === 'gv-search-go') doSearch(root.querySelector('.gv-search').value.trim());
  });
  root.querySelector('.gv-search').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doSearch(e.currentTarget.value.trim());
  });

  // Auto-restore.
  try {
    const saved = await api('genome_viewer_get_session_state');
    if (saved) await restoreSession(saved, browserHost, trackListHost);
  } catch (err) { console.warn('session restore failed', err); }

  async function loadReference() {
    const chosen = await api('select_files', { multiple: false });
    if (!chosen || !chosen[0]) return;
    const meta = await api('genome_viewer_load_reference', { path: chosen[0] });
    state.reference = meta;
    await createBrowser(browserHost, meta);
    scheduleSave();
  }

  async function addTrack() {
    const chosen = await api('select_files', { multiple: true });
    if (!chosen) return;
    for (const p of chosen) {
      try {
        const tm = await api('genome_viewer_add_track', { path: p, kindHint: null });
        state.tracks.push(tm);
        if (state.browser) {
          await state.browser.loadTrack({
            name: p.split(/[\\/]/).pop(),
            type: 'annotation',
            format: tm.kind,
            reader: new TauriFeatureReader({ trackId: tm.track_id, kind: tm.kind }),
          });
        }
        if (tm.suggest_bgzip) {
          const ok = confirm(
            `${p.split(/[\\/]/).pop()} is large (>200 MB). Build bgzip + tabix index for faster future opens?\n` +
            `This will create a .gz file next to the original (the original is preserved).`
          );
          if (ok) {
            try {
              const res = await api('genome_viewer_bgzip_and_tabix', { path: p });
              console.log(`bgzipped: ${res.new_path}`);
            } catch (err) {
              alert(`bgzip failed: ${err?.message || err}`);
            }
          }
        }
      } catch (err) {
        alert(`Failed to load ${p}: ${err?.message || err}`);
      }
    }
    refreshTrackList(trackListHost);
    scheduleSave();
  }

  async function doSearch(q) {
    if (!q || !state.browser) return;
    if (/^[\w.]+:\d+(?:[-,]\s*[\d,]+)?$/i.test(q)) {
      state.browser.search(q.replace(/,/g, ''));
      return;
    }
    const hits = await api('genome_viewer_search_feature', { query: q, limit: 1 });
    if (hits.length) {
      const h = hits[0];
      state.browser.search(`${h.chrom}:${h.start}-${h.end}`);
    }
  }

  function refreshTrackList(host) {
    renderTrackList(host, state.tracks, removeTrack, toggleTrack);
  }

  async function removeTrack(id) {
    await api('genome_viewer_remove_track', { trackId: id });
    state.tracks = state.tracks.filter(t => t.track_id !== id);
    if (state.browser) {
      const t = state.browser.findTracks(tr => tr.reader?.trackId === id);
      t.forEach(x => state.browser.removeTrack(x));
    }
    refreshTrackList(trackListHost);
    scheduleSave();
  }

  function toggleTrack(id, visible) {
    const t = state.tracks.find(t => t.track_id === id);
    if (t) t.visible = visible;
    if (state.browser) {
      const tr = state.browser.findTracks(tr => tr.reader?.trackId === id)[0];
      if (tr) tr.visible = visible;
      state.browser.update();
    }
    scheduleSave();
  }

  function scheduleSave() {
    clearTimeout(state.saveTimer);
    state.saveTimer = setTimeout(async () => {
      try {
        await api('genome_viewer_save_session_state', {
          state: {
            version: 1,
            reference: state.reference ? { path: state.reference.path } : null,
            tracks: state.tracks.map(t => ({ path: t.path, kind: t.kind, visible: t.visible })),
            position: state.position,
          },
        });
      } catch (err) { console.warn('session save failed', err); }
    }, 1000);
  }
}

async function createBrowser(host, meta) {
  host.innerHTML = '';
  const chrom0 = meta.chroms[0];
  const referenceConfig = {
    id: 'rustbrain-ref',
    name: meta.path.split(/[\\/]/).pop(),
    reader: new TauriReferenceReader({ path: meta.path }),
    chromosomes: meta.chroms.map(c => ({ name: c.name, bpLength: Number(c.length) })),
  };
  state.browser = await igv.createBrowser(host, {
    reference: referenceConfig,
    locus: `${chrom0.name}:1-${Math.min(10000, Number(chrom0.length))}`,
    showNavigation: true,
    showIdeogram: false,
    tracks: [],
  });
  state.browser.on('locuschange', (ref) => {
    if (ref && ref.chr) {
      state.position = { chrom: ref.chr, start: ref.start + 1, end: ref.end };
    }
  });
}

async function restoreSession(saved, browserHost, trackListHost) {
  if (!saved.reference) return;
  try {
    const meta = await api('genome_viewer_load_reference', { path: saved.reference.path });
    state.reference = meta;
    await createBrowser(browserHost, meta);
  } catch (e) { console.warn('ref restore failed', e); return; }

  for (const t of saved.tracks || []) {
    try {
      const tm = await api('genome_viewer_add_track', { path: t.path, kindHint: null });
      tm.visible = t.visible;
      state.tracks.push(tm);
      await state.browser.loadTrack({
        name: t.path.split(/[\\/]/).pop(),
        type: 'annotation',
        format: tm.kind,
        reader: new TauriFeatureReader({ trackId: tm.track_id, kind: tm.kind }),
      });
    } catch (e) { console.warn(`track restore skipped: ${t.path}`, e); }
  }
  renderTrackList(trackListHost, state.tracks,
    async (id) => {
      await api('genome_viewer_remove_track', { trackId: id });
      state.tracks = state.tracks.filter(x => x.track_id !== id);
    },
    () => {}
  );
  if (saved.position) {
    state.position = saved.position;
    try { state.browser.search(`${saved.position.chrom}:${saved.position.start}-${saved.position.end}`); } catch (_) {}
  }
}
