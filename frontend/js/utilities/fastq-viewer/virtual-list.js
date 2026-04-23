// A record-oriented virtualized list. Assumes every record has the same pixel height.
// Renders only records in the viewport ± overscan.
export class VirtualList {
  constructor({ host, recordHeight, overscan, renderRecord, fetchBatch }) {
    this.host = host;
    this.recordHeight = recordHeight;
    this.overscan = overscan;
    this.renderRecord = renderRecord;    // (record, index) => HTMLElement
    this.fetchBatch = fetchBatch;        // (start, count) => Promise<record[]>
    this.total = 0;
    this.cache = new Map();               // index → record
    this.pending = new Map();             // index → Promise

    this.host.classList.add('virtual-list');
    this.host.style.overflowY = 'auto';
    this.host.style.position = 'relative';
    this.spacer = document.createElement('div');
    this.viewport = document.createElement('div');
    this.viewport.style.position = 'absolute';
    this.viewport.style.top = '0';
    this.viewport.style.left = '0';
    this.viewport.style.right = '0';
    this.host.appendChild(this.spacer);
    this.host.appendChild(this.viewport);

    this.host.addEventListener('scroll', () => this._schedule());
    this._scheduled = false;
  }

  setTotal(total) {
    this.total = total;
    this.spacer.style.height = `${total * this.recordHeight}px`;
    this.cache.clear();
    this._schedule();
  }

  scrollToIndex(index) {
    this.host.scrollTop = index * this.recordHeight;
  }

  _schedule() {
    if (this._scheduled) return;
    this._scheduled = true;
    requestAnimationFrame(() => {
      this._scheduled = false;
      this._render();
    });
  }

  async _render() {
    const scrollTop = this.host.scrollTop;
    const hostH = this.host.clientHeight;
    const firstVisible = Math.max(0, Math.floor(scrollTop / this.recordHeight) - this.overscan);
    const lastVisible = Math.min(this.total - 1, Math.ceil((scrollTop + hostH) / this.recordHeight) + this.overscan);
    if (lastVisible < firstVisible) {
      this.viewport.innerHTML = '';
      return;
    }
    await this._ensureRange(firstVisible, lastVisible);
    this._paint(firstVisible, lastVisible);
  }

  async _ensureRange(first, last) {
    const missing = [];
    for (let i = first; i <= last; i++) {
      if (!this.cache.has(i) && !this.pending.has(i)) missing.push(i);
    }
    if (missing.length === 0) return;
    // Coalesce contiguous gaps.
    missing.sort((a, b) => a - b);
    const runs = [];
    let runStart = missing[0];
    let runEnd = missing[0];
    for (let i = 1; i < missing.length; i++) {
      if (missing[i] === runEnd + 1) runEnd = missing[i];
      else { runs.push([runStart, runEnd]); runStart = missing[i]; runEnd = missing[i]; }
    }
    runs.push([runStart, runEnd]);

    const promises = runs.map(([s, e]) => {
      const count = e - s + 1;
      const p = this.fetchBatch(s, count).then(recs => {
        recs.forEach((r, i) => this.cache.set(s + i, r));
      }).finally(() => {
        for (let i = s; i <= e; i++) this.pending.delete(i);
      });
      for (let i = s; i <= e; i++) this.pending.set(i, p);
      return p;
    });
    await Promise.all(promises);
  }

  _paint(first, last) {
    this.viewport.style.transform = `translateY(${first * this.recordHeight}px)`;
    this.viewport.innerHTML = '';
    for (let i = first; i <= last; i++) {
      const rec = this.cache.get(i);
      if (!rec) continue;
      const el = this.renderRecord(rec, i);
      el.style.height = `${this.recordHeight}px`;
      el.style.boxSizing = 'border-box';
      this.viewport.appendChild(el);
    }
  }
}
