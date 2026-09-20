import { NativeAnnotationLayer } from './pkg/annotation_layer.js';
import init, {
  WebAnnotationEvent,
  WebDocument,
} from './pkg/web_canvas_example.js';

function position(element, [left, top, right, bottom]) {
  Object.assign(element.style, {
    left: `${left}px`,
    top: `${top}px`,
    width: `${right - left}px`,
    height: `${bottom - top}px`,
  });
}

function releaseCanvas(canvas) {
  canvas.width = 0;
  canvas.height = 0;
}

function removeUnused(elements, retainedKeys, dispose = () => {}) {
  for (const [key, element] of elements) {
    if (retainedKeys.has(key)) continue;
    dispose(element);
    element.remove();
    elements.delete(key);
  }
}

function createPage(index) {
  const element = document.createElement('section');
  element.className = 'page';
  element.dataset.page = String(index);
  element.setAttribute('aria-label', `Page ${index + 1}`);

  const canvas = document.createElement('canvas');
  canvas.className = 'layer content';
  const annotations = document.createElement('div');
  annotations.className = 'layer annotations';
  const selection = document.createElement('div');
  selection.className = 'layer selection';
  selection.setAttribute('aria-hidden', 'true');
  element.append(canvas, annotations, selection);

  return {
    element, canvas, annotations, selection,
    nativeLayer: undefined,
    highlights: new Map(),
    snapshot: undefined,
  };
}

class PdfViewer {
  constructor() {
    this.elements = Object.fromEntries(
      ['pages', 'status', 'error', 'file', 'zoom', 'rotate', 'copy'].map(id => {
        const element = document.getElementById(id);
        if (!element) throw new Error(`Missing viewer element: ${id}`);
        return [id, element];
      }),
    );
    this.pdf = undefined;
    this.pages = new Map();
    this.visiblePages = new Set();
    this.rotation = 0;
    this.selectionRevision = undefined;
    this.selectionFrame = undefined;
    this.fullSelection = true;
    this.drag = undefined;
    this.loadRequest = 0;
    this.observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        const index = Number(entry.target.dataset.page);
        if (entry.isIntersecting) this.visiblePages.add(index);
        else this.visiblePages.delete(index);
      }
      this.scheduleSelection(true);
    }, { rootMargin: '200px' });
    this.bindEvents();
  }

  showError(error) {
    this.elements.error.textContent = error instanceof Error ? error.message : String(error);
  }

  handle(callback, { clearError = true } = {}) {
    return async event => {
      try {
        if (clearError) this.elements.error.textContent = '';
        await callback(event);
      } catch (error) {
        this.showError(error);
      }
    };
  }

  setDocumentControlsEnabled(enabled) {
    for (const name of ['zoom', 'rotate', 'copy']) this.elements[name].disabled = !enabled;
  }

  bindEvents() {
    const { pages, file, zoom, rotate, copy } = this.elements;
    file.addEventListener('change', this.handle(async () => {
      const selected = file.files.item(0);
      if (selected) await this.loadBytes(() => selected.arrayBuffer());
    }));
    zoom.addEventListener('change', this.handle(() => this.renderPages()));
    rotate.addEventListener('click', this.handle(() => {
      this.rotation = (this.rotation + 90) % 360;
      this.renderPages();
    }));
    copy.addEventListener('click', this.handle(async () => {
      if (!this.pdf) return;
      const text = this.pdf.selected_text();
      await navigator.clipboard.writeText(text);
      this.elements.status.textContent = `Copied ${text.length} characters`;
    }));
    pages.addEventListener('pointerdown', this.handle(event => this.startSelection(event)));
    window.addEventListener('pointermove', this.handle(event => this.extendSelection(event), { clearError: false }));
    const endSelection = event => {
      if (this.drag?.pointerId === event.pointerId) this.drag = undefined;
    };
    window.addEventListener('pointerup', endSelection);
    window.addEventListener('pointercancel', endSelection);
    window.addEventListener('blur', () => { this.drag = undefined; });
    window.addEventListener('copy', this.handle(event => {
      if (event.target instanceof Element && event.target.closest('input, select, textarea, [contenteditable]')) return;
      const text = this.pdf?.selected_text();
      if (!text || !event.clipboardData) return;
      event.clipboardData.setData('text/plain', text);
      event.preventDefault();
    }, { clearError: false }));
  }

  disposeDocument() {
    this.observer.disconnect();
    this.visiblePages.clear();
    if (this.selectionFrame !== undefined) cancelAnimationFrame(this.selectionFrame);
    this.selectionFrame = undefined;
    this.selectionRevision = undefined;
    this.fullSelection = true;
    this.drag = undefined;
    for (const page of this.pages.values()) {
      page.snapshot?.free();
      releaseCanvas(page.canvas);
      page.nativeLayer?.dispose();
    }
    this.pages.clear();
    this.elements.pages.replaceChildren();
    this.pdf?.free();
    this.pdf = undefined;
  }

  async loadBytes(readBytes) {
    const request = ++this.loadRequest;
    try {
      const bytes = await readBytes();
      // A slower fetch or file read must not replace a more recent choice.
      if (request !== this.loadRequest) return;
      const next = new WebDocument(new Uint8Array(bytes));
      this.disposeDocument();
      this.pdf = next;
      this.setDocumentControlsEnabled(true);
      const fragment = document.createDocumentFragment();
      for (let index = 0; index < next.page_count(); index++) {
        const page = createPage(index);
        this.pages.set(index, page);
        fragment.append(page.element);
      }
      this.elements.pages.append(fragment);
      for (const page of this.pages.values()) this.observer.observe(page.element);
      this.renderPages();
      this.elements.status.textContent = `${next.page_count()} pages · Canvas 2D`;
      const diagnostics = JSON.parse(next.annotation_diagnostics());
      if (diagnostics.length) this.showError(diagnostics.map(d => `Page ${d.page + 1}${d.object ? `, object ${d.object.number}` : ''}: ${d.message}`).join('\n'));
    } catch (error) {
      if (request === this.loadRequest) throw error;
    }
  }

  renderPages() {
    if (!this.pdf) return;
    const zoom = Number(this.elements.zoom.value);
    for (const [index, page] of this.pages) {
      const [width, height] = this.pdf.page_size(index);
      const snapshot = this.pdf.render(index, page.canvas, zoom, window.devicePixelRatio, this.rotation);
      // The page owns the snapshot even if a subsequent DOM update fails.
      page.snapshot?.free();
      page.snapshot = snapshot;
      const sideways = this.rotation % 180 !== 0;
      page.element.style.width = `${(sideways ? height : width) * zoom}px`;
      page.element.style.height = `${(sideways ? width : height) * zoom}px`;
      const transform = `matrix(${snapshot.device_to_css().join(',')})`;
      for (const layer of [page.canvas, page.selection]) {
        Object.assign(layer.style, { width: `${width}px`, height: `${height}px`, transform });
      }
      this.updateAnnotations(page);
    }
    this.scheduleSelection(true);
  }

  updateAnnotations(page) {
    if (!page.nativeLayer) {
      page.nativeLayer = new NativeAnnotationLayer(page.annotations, {
        onCommand: (entry, command, revision, viewportRevision) => this.sendAnnotationEvent(entry, command, revision, viewportRevision),
        toPageDelta: (entry, dx, dy) => Array.from(this.pdf.page_delta(entry.page, dx, dy)),
        onAction: entry => { this.elements.status.textContent = entry.action ? `Annotation action: ${typeof entry.action === 'string' ? entry.action : Object.keys(entry.action)[0]}` : entry.text || ''; },
        onError: error => this.showError(error),
      });
    }
    const snapshot = page.snapshot;
    page.nativeLayer.reconcile({
      entries: JSON.parse(snapshot.entries_json()),
      annotationRevision: snapshot.content_revision(),
      viewportRevision: snapshot.viewport_revision(),
    });
  }

  sendAnnotationEvent(entry, command, revision, viewportRevision) {
    const event = new WebAnnotationEvent(JSON.stringify({
      target: { page: entry.page, annotation_id: entry.id, viewport_revision: viewportRevision },
      expected_revision: revision, operation: command,
    }));
    let receipt;
    try {
      receipt = JSON.parse(this.pdf.accept_event(event));
    } catch (error) {
      this.refreshAnnotations([entry.page]);
      throw error;
    }
    // Projection is retryable after commitment; never report the accepted edit as rejected.
    try { this.refreshAnnotations(receipt.pages); }
    catch (error) { this.showError(`Edit committed; presentation refresh failed: ${error}`); }
  }

  dispatchAnnotationCommand(command) {
    if (!this.pdf) throw new Error('No document is open');
    const receipt = JSON.parse(this.pdf.dispatch_command(JSON.stringify(command)));
    try { this.refreshAnnotations(receipt.pages); }
    catch (error) { this.showError(`Edit committed; presentation refresh failed: ${error}`); }
    return receipt;
  }

  refreshAnnotations(changed) {
    const revision = this.pdf.annotation_revision();
    for (const page of this.pages.values()) page.nativeLayer?.advanceRevision(revision);
    for (const index of changed) {
      const page = this.pages.get(index);
      if (!page) continue;
      if (!page.snapshot) continue;
      const snapshot = this.pdf.annotations(index);
      page.snapshot.free();
      page.snapshot = snapshot;
      this.updateAnnotations(page);
    }
  }

  hitTest(event) {
    if (!this.pdf) return undefined;
    const element = document.elementFromPoint(event.clientX, event.clientY)?.closest('.page');
    if (!element || !this.elements.pages.contains(element)) return undefined;
    const bounds = element.getBoundingClientRect();
    const point = this.pdf.hit_test(Number(element.dataset.page), event.clientX - bounds.left, event.clientY - bounds.top);
    return point.length ? Array.from(point) : undefined;
  }

  startSelection(event) {
    if (event.button !== 0 || !event.isPrimary || event.target.closest('.annotation')) return;
    const anchor = this.hitTest(event);
    if (!anchor) return;
    this.pdf.select([...anchor, ...anchor]);
    this.drag = { anchor, pointerId: event.pointerId };
    this.scheduleSelection();
    event.preventDefault();
  }

  extendSelection(event) {
    if (this.drag?.pointerId !== event.pointerId) return;
    const focus = this.hitTest(event);
    if (!focus) return;
    this.pdf.select([...this.drag.anchor, ...focus]);
    this.scheduleSelection();
  }

  scheduleSelection(full = false) {
    this.fullSelection ||= full;
    if (this.selectionFrame !== undefined || !this.pdf) return;
    this.selectionFrame = requestAnimationFrame(() => {
      this.selectionFrame = undefined;
      try {
        this.updateSelection();
      } catch (error) {
        this.showError(error);
      }
    });
  }

  updateSelection() {
    const batches = this.pdf.selection_updates(
      [...this.visiblePages], this.fullSelection ? undefined : this.selectionRevision,
    );
    try {
      for (const batch of batches) {
        const page = this.pages.get(batch.page());
        if (!page) continue;
        const keys = batch.keys();
        const bounds = batch.bounds();
        removeUnused(page.highlights, new Set(keys));
        keys.forEach((key, index) => {
          let highlight = page.highlights.get(key);
          if (!highlight) {
            highlight = document.createElement('div');
            highlight.className = 'highlight';
            page.selection.append(highlight);
            page.highlights.set(key, highlight);
          }
          position(highlight, bounds.subarray(index * 4, index * 4 + 4));
        });
      }
      const latest = batches.at(-1);
      if (latest) this.selectionRevision = latest.selection_revision();
      this.fullSelection = false;
    } finally {
      for (const batch of batches) batch.free();
    }
  }

  async start() {
    await init();
    this.elements.file.disabled = false;
  }
}

const viewer = new PdfViewer();
// Keep the example's inspection hooks current across document replacements.
window.safePdf = {
  get document() { return viewer.pdf; },
  pages: viewer.pages,
  renderPages: () => viewer.renderPages(),
  dispatchAnnotationCommand: command => viewer.dispatchAnnotationCommand(command),
  refreshAnnotations: pages => viewer.refreshAnnotations(pages),
};
await viewer.handle(() => viewer.start())();
