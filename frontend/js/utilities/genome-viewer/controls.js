export function renderControls(container) {
  container.innerHTML = `
    <div class="gv-controls" style="display:flex;gap:8px;align-items:center;padding:8px;background:#f5f1eb;border-bottom:1px solid #e7e5e4">
      <button class="btn" data-act="gv-load-reference">Load Reference FASTA</button>
      <button class="btn" data-act="gv-add-track">Add Track</button>
      <input type="text" class="gv-search" placeholder="chr1:10-50  or  gene name" style="flex:1;padding:4px 8px;font-family:monospace">
      <button class="btn" data-act="gv-search-go">Go</button>
    </div>
    <div class="gv-track-list" style="padding:6px 8px;background:#faf8f4;border-bottom:1px solid #f1ede7;font-size:13px"></div>
    <div class="gv-browser" style="height:calc(100vh - 240px);background:#fff"></div>
  `;
}

export function renderTrackList(host, tracks, onRemove, onToggle) {
  if (!tracks.length) {
    host.innerHTML = '<span style="color:#a8a29e">No tracks loaded</span>';
    return;
  }
  host.innerHTML = tracks.map(t => `
    <span class="gv-track-chip" style="display:inline-flex;align-items:center;margin-right:8px;padding:2px 6px;border:1px solid #d6d3d1;border-radius:4px;background:#fff">
      <input type="checkbox" class="gv-track-toggle" data-track="${t.track_id}" ${t.visible ? 'checked' : ''} style="margin-right:4px">
      <span style="margin-right:4px">${escapeHtml(filename(t.path))} <span style="color:#a8a29e">(${t.kind}, ${t.feature_count})</span></span>
      <button class="gv-track-remove" data-track="${t.track_id}" style="background:none;border:none;color:#a8a29e;cursor:pointer">x</button>
    </span>
  `).join('');
  host.querySelectorAll('.gv-track-remove').forEach(b => {
    b.addEventListener('click', () => onRemove(b.dataset.track));
  });
  host.querySelectorAll('.gv-track-toggle').forEach(cb => {
    cb.addEventListener('change', () => onToggle(cb.dataset.track, cb.checked));
  });
}

function filename(p) {
  return String(p).split(/[\\/]/).pop();
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
