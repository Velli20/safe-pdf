/**
 * @module safe-pdf-viewer
 *
 * Full-featured, embeddable PDF viewer component built on top of
 * {@link SafePdfRenderer}.
 *
 * Provides continuous-scroll page viewing, zoom controls, keyboard
 * navigation, Rust-owned text selection, and page prefetching — all
 * encapsulated in a single class that can be dropped into any web page.
 *
 * The viewer creates its own DOM inside a provided container element and
 * emits events so that consuming applications can build custom chrome
 * (toolbars, sidebars, etc.) on top.
 *
 * @example
 * ```js
 * import { SafePdfViewer } from './safe-pdf-viewer.js';
 *
 * const viewer = new SafePdfViewer(document.getElementById('viewer'), {
 *   wasmUrl: './dist/emscripten.js',
 * });
 *
 * await viewer.init();
 *
 * // From a file input
 * fileInput.addEventListener('change', (e) => {
 *   viewer.loadFile(e.target.files[0]);
 * });
 *
 * // React to page changes
 * viewer.addEventListener('pagechange', (e) => {
 *   console.log('Now on page', e.detail.pageNumber);
 * });
 * ```
 */

import { SafePdfRenderer } from './safe-pdf-renderer.js';

// ============================================================
// Constants
// ============================================================

/** Default PDF page width in points (8.5 in × 72). */
const DEFAULT_PAGE_WIDTH = 612;

/** Default PDF page height in points (11 in × 72). */
const DEFAULT_PAGE_HEIGHT = 792;

/** Gap between pages in the scroll view (px). */
const PAGE_GAP = 20;

/** Margin around the viewport within which pages are rendered ahead of time. */
const RENDER_MARGIN = '100%';

/** Idle-callback timeout for prefetch work (ms). */
const PREFETCH_TIMEOUT = 1000;

/** Minimum remaining idle time to start rendering a prefetch page (ms). */
const PREFETCH_MIN_IDLE = 10;

/**
 * Polyfill for `requestIdleCallback` (Safari, older browsers).
 * @type {typeof requestIdleCallback}
 */
const _requestIdleCallback =
  typeof requestIdleCallback === 'function'
    ? requestIdleCallback
    : (cb) => setTimeout(() => cb({ timeRemaining: () => 50 }), 1);

// ============================================================
// Component CSS (injected once)
// ============================================================

const VIEWER_CSS = /* css */ `
.spdf-scroll-container {
  width: 100%;
  height: 100%;
  overflow-y: auto;
  overflow-x: auto;
  position: relative;
  scroll-behavior: smooth;
}
.spdf-scroll-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 20px;
  gap: ${PAGE_GAP}px;
  min-height: 100%;
}
.spdf-page-wrapper {
  position: relative;
  flex: none;
  background: #fff;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  user-select: none;
  cursor: text;
}
.spdf-page-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  color: #999;
  font-size: 14px;
  background: #f0f0f0;
  width: 100%;
  height: 100%;
}
.spdf-page-number {
  position: absolute;
  bottom: -24px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 12px;
  color: #aaa;
  white-space: nowrap;
}
.spdf-page-img {
  width: 100%;
  height: 100%;
  display: block;
}
.spdf-selection-layer {
  position: absolute;
  left: 0;
  top: 0;
  transform-origin: 0 0;
  pointer-events: none;
  z-index: 2;
}
.spdf-selection-rect {
  position: absolute;
  background: rgba(51, 122, 255, 0.28);
  mix-blend-mode: multiply;
}
.spdf-loading-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}
.spdf-loading-overlay.spdf-hidden {
  display: none;
}
.spdf-spinner {
  width: 40px;
  height: 40px;
  border: 3px solid #333;
  border-top-color: #0066cc;
  border-radius: 50%;
  animation: spdf-spin 1s linear infinite;
}
@keyframes spdf-spin {
  to { transform: rotate(360deg); }
}
.spdf-empty-message {
  color: #aaa;
  padding: 40px;
  text-align: center;
}
`;

let _cssInjected = false;

/** Inject component styles into the document head (once). */
function injectCSS() {
  if (_cssInjected) return;
  const style = document.createElement('style');
  style.textContent = VIEWER_CSS;
  document.head.appendChild(style);
  _cssInjected = true;
}

// ============================================================
// SafePdfViewer
// ============================================================

/**
 * @typedef {object} SafePdfViewerOptions
 * @property {string}  wasmUrl             URL to the Emscripten ES module.
 * @property {number}  [initialZoom=1]     Initial zoom level (1 = 100%).
 * @property {string}  [emptyMessage]      Message shown when no PDF is loaded.
 * @property {boolean} [keyboardNav=true]  Enable built-in keyboard navigation.
 */

/**
 * Embeddable PDF viewer component.
 *
 * Emits the following events (via `addEventListener`):
 *
 * | Event          | `detail`                                         |
 * |----------------|--------------------------------------------------|
 * | `ready`        | `{}`                                             |
 * | `load`         | `{ pageCount: number, fileName?: string }`       |
 * | `pagechange`   | `{ page: number, pageNumber: number }`           |
 * | `zoomchange`   | `{ zoom: number, mode: string }`                 |
 * | `error`        | `{ message: string, error?: Error }`             |
 */
export class SafePdfViewer extends EventTarget {
  /** @type {HTMLElement} */
  #container;

  /** @type {SafePdfRenderer} */
  #renderer;

  /** @type {SafePdfViewerOptions} */
  #options;

  // ---- State ----
  #pageCount = 0;
  #currentPage = 0;
  #zoom = 1.0;
  #zoomMode = 'fixed'; // 'fixed' | 'fit-width' | 'fit-page'

  /**
   * PDF-point dimensions of each page, populated from WASM after load.
   * @type {Array<{width: number, height: number}>}
   */
  #pageSizes = [];

  /** @type {Set<number>} Pages intersecting the viewport plus the render margin. */
  #visiblePages = new Set();

  /** @type {IntersectionObserver|null} */
  #observer = null;

  /** @type {number|null} Pending animation frame for rendering visible pages. */
  #renderFrame = null;

  // ---- Prefetch ----
  /** @type {number[]} */
  #prefetchQueue = [];
  #isPrefetching = false;

  // ---- Text selection ----
  /** @type {{ anchor: number[], pointerId: number } | null} */
  #selectionDrag = null;
  /** @type {number|undefined} Last selection revision applied to the DOM. */
  #selectionRevision = undefined;
  /** @type {boolean} Whether the next selection update must refresh every visible page. */
  #fullSelection = true;
  /** @type {number|null} Pending animation frame for selection highlights. */
  #selectionFrame = null;
  /** @type {Map<number, Map<number, HTMLElement>>} Highlight nodes by page and key. */
  #highlights = new Map();

  // ---- DOM refs ----
  /** @type {HTMLElement} */
  #scrollContainer;
  /** @type {HTMLElement} */
  #scrollContent;
  /** @type {HTMLElement} */
  #loadingOverlay;

  // ---- Bound listeners (for cleanup) ----
  #boundHandleScroll;
  #boundHandleKeydown;
  #boundHandleResize;
  #boundHandlePointerDown;
  #boundHandlePointerMove;
  #boundHandlePointerUp;
  #boundHandleCopy;

  /**
   * Create a new SafePdfViewer.
   *
   * @param {HTMLElement}           container  DOM element to mount the viewer into.
   * @param {SafePdfViewerOptions}  options    Viewer configuration.
   */
  constructor(container, options = {}) {
    super();

    if (!(container instanceof HTMLElement)) {
      throw new TypeError('container must be an HTMLElement');
    }
    if (!options.wasmUrl) {
      throw new TypeError('options.wasmUrl is required');
    }

    this.#container = container;
    this.#options = options;
    this.#zoom = options.initialZoom ?? 1.0;

    this.#renderer = new SafePdfRenderer();

    // Bind event handlers so they can be removed later.
    this.#boundHandleScroll = this.#scheduleRender.bind(this);
    this.#boundHandleKeydown = this.#handleKeydown.bind(this);
    this.#boundHandleResize = this.#handleResize.bind(this);
    this.#boundHandlePointerDown = this.#handlePointerDown.bind(this);
    this.#boundHandlePointerMove = this.#handlePointerMove.bind(this);
    this.#boundHandlePointerUp = this.#handlePointerUp.bind(this);
    this.#boundHandleCopy = this.#handleCopy.bind(this);

    injectCSS();
    this.#buildDOM();
  }

  // ==================================================================
  // Public API
  // ==================================================================

  /**
   * Initialise the WASM backend. Must be called (and awaited) before
   * loading any PDF.
   *
   * @returns {Promise<void>}
   */
  async init() {
    this.#showLoading(true);

    try {
      await this.#renderer.init(this.#options.wasmUrl);
    } catch (err) {
      this.#emitError('Failed to initialise WASM renderer', err);
      this.#showLoading(false);
      throw err;
    }

    this.#attachEventListeners();
    this.#showLoading(false);

    this.dispatchEvent(new CustomEvent('ready'));
  }

  /**
   * Load a PDF from a `File` object (e.g. from an `<input type="file">`).
   *
   * @param {File} file
   * @returns {Promise<{ pageCount: number }>}
   */
  async loadFile(file) {
    this.#showLoading(true);

    try {
      const buffer = await file.arrayBuffer();
      return this.#load(buffer, file.name);
    } catch (err) {
      this.#emitError('Failed to load PDF file', err);
      this.#showLoading(false);
      throw err;
    }
  }

  /**
   * Load a PDF from an `ArrayBuffer` (e.g. from `fetch`).
   *
   * @param {ArrayBuffer} buffer
   * @param {string}      [fileName]  Optional file name for event detail.
   * @returns {{ pageCount: number }}
   */
  loadArrayBuffer(buffer, fileName) {
    this.#showLoading(true);

    try {
      return this.#load(buffer, fileName);
    } catch (err) {
      this.#emitError('Failed to load PDF buffer', err);
      this.#showLoading(false);
      throw err;
    }
  }

  /**
   * Scroll to a specific page.
   *
   * @param {number} pageIndex  Zero-based page index.
   */
  goToPage(pageIndex) {
    const wrapper = this.#wrapper(pageIndex);
    if (!wrapper) return;

    wrapper.scrollIntoView({ block: 'start', behavior: 'smooth' });
    this.#setCurrentPage(pageIndex);
  }

  /** Navigate to the previous page. */
  previousPage() {
    this.goToPage(this.#currentPage - 1);
  }

  /** Navigate to the next page. */
  nextPage() {
    this.goToPage(this.#currentPage + 1);
  }

  /**
   * Set the zoom level.
   *
   * Page wrappers are resized in place, so scroll position and the text
   * selection survive; visible pages re-render at the new size.
   *
   * @param {number|'fit-width'|'fit-page'} value
   *   A numeric scale factor (e.g. `1.5` for 150%), or one of the special
   *   strings `'fit-width'` / `'fit-page'`.
   */
  setZoom(value) {
    // Use the first page as the reference for fit calculations; fall back to
    // defaults when no PDF is loaded yet.
    const refW = this.#pageSizes[0]?.width ?? DEFAULT_PAGE_WIDTH;
    const refH = this.#pageSizes[0]?.height ?? DEFAULT_PAGE_HEIGHT;

    if (value === 'fit-width') {
      this.#zoomMode = 'fit-width';
      const available = this.#scrollContainer.clientWidth - 60;
      this.#zoom = available / refW;
    } else if (value === 'fit-page') {
      this.#zoomMode = 'fit-page';
      const availW = this.#scrollContainer.clientWidth - 60;
      const availH = this.#scrollContainer.clientHeight - 60;
      this.#zoom = Math.min(availW / refW, availH / refH);
    } else {
      this.#zoomMode = 'fixed';
      this.#zoom = Number(value) || 1;
    }

    this.#renderer.clearCache();
    for (let i = 0; i < this.#pageCount; i++) this.#layoutWrapper(i);
    this.#scheduleRender();
    this.#scheduleSelection(true);

    this.dispatchEvent(
      new CustomEvent('zoomchange', {
        detail: { zoom: this.#zoom, mode: this.#zoomMode },
      })
    );
  }

  /** Current zero-based page index. */
  getCurrentPage() {
    return this.#currentPage;
  }

  /** Number of pages in the loaded PDF (0 if none). */
  getPageCount() {
    return this.#pageCount;
  }

  /** Current numeric zoom level. */
  getZoom() {
    return this.#zoom;
  }

  /** Current zoom mode: `'fixed'`, `'fit-width'`, or `'fit-page'`. */
  getZoomMode() {
    return this.#zoomMode;
  }

  /**
   * Get the number of cached (already rendered) page images.
   *
   * Delegates to a single WASM call rather than iterating over every page,
   * making it O(1) and safe to call on every `pagechange` event.
   *
   * @returns {number}
   */
  getCachedPageCount() {
    if (!this.#renderer.isReady || this.#pageCount === 0) {
      return 0;
    }
    return this.#renderer.getCacheCount();
  }

  /**
   * Return the currently selected text, or an empty string.
   * @returns {string}
   */
  getSelectedText() {
    if (!this.#renderer.isReady || this.#pageCount === 0) return '';
    return this.#renderer.selectedText();
  }

  /** Clear the current text selection. */
  clearSelection() {
    this.#selectionDrag = null;
    if (this.#renderer.isReady && this.#pageCount > 0) this.#renderer.select([]);
    this.#scheduleSelection(true);
  }

  /**
   * Access the underlying {@link SafePdfRenderer} for advanced usage.
   * @returns {SafePdfRenderer}
   */
  getRenderer() {
    return this.#renderer;
  }

  /**
   * Tear down the viewer: remove DOM, detach listeners, free WASM memory.
   */
  destroy() {
    this.#detachEventListeners();
    this.#resetDocumentState();
    this.#renderer.destroy();
    this.#container.innerHTML = '';
  }

  // ==================================================================
  // DOM Construction
  // ==================================================================

  /** Create the viewer's internal DOM structure. */
  #buildDOM() {
    this.#container.innerHTML = '';

    // Scroll container
    this.#scrollContainer = document.createElement('div');
    this.#scrollContainer.className = 'spdf-scroll-container';
    this.#scrollContainer.tabIndex = 0;

    // Loading overlay
    this.#loadingOverlay = document.createElement('div');
    this.#loadingOverlay.className = 'spdf-loading-overlay';
    const spinner = document.createElement('div');
    spinner.className = 'spdf-spinner';
    this.#loadingOverlay.appendChild(spinner);

    // Scroll content
    this.#scrollContent = document.createElement('div');
    this.#scrollContent.className = 'spdf-scroll-content';

    const emptyMsg = document.createElement('div');
    emptyMsg.className = 'spdf-empty-message';
    emptyMsg.textContent =
      this.#options.emptyMessage ?? 'Load a PDF file to begin';
    this.#scrollContent.appendChild(emptyMsg);

    this.#scrollContainer.appendChild(this.#loadingOverlay);
    this.#scrollContainer.appendChild(this.#scrollContent);
    this.#container.appendChild(this.#scrollContainer);
  }

  /** Build the page placeholders and start observing their visibility. */
  #buildPageLayout() {
    this.#scrollContent.innerHTML = '';

    for (let i = 0; i < this.#pageCount; i++) {
      const wrapper = document.createElement('div');
      wrapper.className = 'spdf-page-wrapper';
      wrapper.dataset.pageIndex = String(i);

      const placeholder = document.createElement('div');
      placeholder.className = 'spdf-page-placeholder';
      placeholder.textContent = `Page ${i + 1}`;
      wrapper.appendChild(placeholder);

      const selectionLayer = document.createElement('div');
      selectionLayer.className = 'spdf-selection-layer';
      wrapper.appendChild(selectionLayer);

      const label = document.createElement('div');
      label.className = 'spdf-page-number';
      label.textContent = `Page ${i + 1}`;
      wrapper.appendChild(label);

      this.#scrollContent.appendChild(wrapper);
      this.#layoutWrapper(i);
      this.#observer.observe(wrapper);
    }
  }

  /** Size a page wrapper for the current zoom and mark its pixels stale. */
  #layoutWrapper(pageIndex) {
    const wrapper = this.#wrapper(pageIndex);
    if (!wrapper) return;
    const { width, height } = this.#scaledPageSizeForPage(pageIndex);
    wrapper.style.width = `${width}px`;
    wrapper.style.height = `${height}px`;
    this.#scaleSelectionLayer(pageIndex);
  }

  /** @returns {HTMLElement|undefined} */
  #wrapper(pageIndex) {
    const wrapper = this.#scrollContent.children[pageIndex];
    return wrapper?.classList.contains('spdf-page-wrapper') ? wrapper : undefined;
  }

  // ==================================================================
  // Event Listeners
  // ==================================================================

  #attachEventListeners() {
    this.#observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const index = Number(entry.target.dataset.pageIndex);
          if (entry.isIntersecting) this.#visiblePages.add(index);
          else this.#visiblePages.delete(index);
        }
        this.#scheduleRender();
        this.#scheduleSelection(true);
      },
      { root: this.#scrollContainer, rootMargin: RENDER_MARGIN }
    );

    this.#scrollContainer.addEventListener(
      'scroll',
      this.#boundHandleScroll,
      { passive: true }
    );

    document.addEventListener('keydown', this.#boundHandleKeydown, true);
    window.addEventListener('resize', this.#boundHandleResize);
    this.#scrollContent.addEventListener(
      'pointerdown',
      this.#boundHandlePointerDown
    );
    window.addEventListener('pointermove', this.#boundHandlePointerMove);
    window.addEventListener('pointerup', this.#boundHandlePointerUp);
    window.addEventListener('pointercancel', this.#boundHandlePointerUp);
    window.addEventListener('blur', this.#boundHandlePointerUp);
    document.addEventListener('copy', this.#boundHandleCopy, true);
  }

  #detachEventListeners() {
    this.#observer?.disconnect();
    this.#observer = null;
    this.#scrollContainer?.removeEventListener('scroll', this.#boundHandleScroll);
    document.removeEventListener('keydown', this.#boundHandleKeydown, true);
    window.removeEventListener('resize', this.#boundHandleResize);
    this.#scrollContent?.removeEventListener(
      'pointerdown',
      this.#boundHandlePointerDown
    );
    window.removeEventListener('pointermove', this.#boundHandlePointerMove);
    window.removeEventListener('pointerup', this.#boundHandlePointerUp);
    window.removeEventListener('pointercancel', this.#boundHandlePointerUp);
    window.removeEventListener('blur', this.#boundHandlePointerUp);
    document.removeEventListener('copy', this.#boundHandleCopy, true);
  }

  // ==================================================================
  // Loading helpers
  // ==================================================================

  /** Load a PDF buffer into the renderer, rebuild the layout, and emit `load`. */
  #load(buffer, fileName) {
    this.#resetDocumentState();
    const { pageCount } = this.#renderer.loadPdf(buffer);
    this.#pageCount = pageCount;
    this.#currentPage = 0;

    // Query each page's displayed dimensions from the WASM/PDF layer.
    this.#pageSizes = [];
    for (let i = 0; i < pageCount; i++) {
      const dims = this.#renderer.getPageDimensions(i);
      this.#pageSizes.push(
        dims ?? { width: DEFAULT_PAGE_WIDTH, height: DEFAULT_PAGE_HEIGHT }
      );
    }

    this.dispatchEvent(
      new CustomEvent('load', { detail: { pageCount, fileName } })
    );

    this.#buildPageLayout();
    this.#showLoading(false);
    return { pageCount };
  }

  /** Forget per-document DOM and selection state before a load or teardown. */
  #resetDocumentState() {
    this.#observer?.disconnect();
    this.#visiblePages.clear();
    this.#highlights.clear();
    this.#selectionDrag = null;
    this.#selectionRevision = undefined;
    this.#fullSelection = true;
    this.#prefetchQueue = [];
    if (this.#renderFrame !== null) cancelAnimationFrame(this.#renderFrame);
    if (this.#selectionFrame !== null) cancelAnimationFrame(this.#selectionFrame);
    this.#renderFrame = null;
    this.#selectionFrame = null;
    this.#pageCount = 0;
    this.#pageSizes = [];
  }

  // ==================================================================
  // Rendering Pipeline
  // ==================================================================

  /**
   * Calculate the pixel size of a specific page at the current zoom.
   *
   * @param {number} pageIndex
   * @returns {{ width: number, height: number }}
   */
  #scaledPageSizeForPage(pageIndex) {
    const size = this.#pageSizes[pageIndex];
    const w = size?.width ?? DEFAULT_PAGE_WIDTH;
    const h = size?.height ?? DEFAULT_PAGE_HEIGHT;
    return {
      width: Math.round(w * this.#zoom),
      height: Math.round(h * this.#zoom),
    };
  }

  /**
   * Render a page into its wrapper's display canvas via `drawImage`, unless
   * the canvas already shows this page at the current zoom.
   *
   * @param {number} pageIndex
   */
  #renderAndDisplay(pageIndex) {
    const wrapper = this.#wrapper(pageIndex);
    if (!wrapper) return;

    const zoomKey = this.#zoom.toFixed(4);
    if (wrapper.dataset.zoom === zoomKey) return;

    const { width, height } = this.#scaledPageSizeForPage(pageIndex);
    this.#renderer.renderPageToCanvas(pageIndex, width, height);

    let displayCanvas = wrapper.querySelector('.spdf-page-canvas');
    if (!displayCanvas) {
      wrapper.querySelector('.spdf-page-placeholder')?.remove();
      displayCanvas = document.createElement('canvas');
      displayCanvas.className = 'spdf-page-canvas spdf-page-img';
      wrapper.insertBefore(displayCanvas, wrapper.firstChild);
    }

    displayCanvas.width = width;
    displayCanvas.height = height;
    displayCanvas.getContext('2d').drawImage(this.#renderer.canvas, 0, 0);
    wrapper.dataset.zoom = zoomKey;
  }

  /** Coalesce scroll and visibility changes into one render per frame. */
  #scheduleRender() {
    if (this.#renderFrame !== null || this.#pageCount === 0) return;
    this.#renderFrame = requestAnimationFrame(() => {
      this.#renderFrame = null;
      this.#renderVisiblePages();
    });
  }

  /** Render every page intersecting the viewport (plus the render margin). */
  #renderVisiblePages() {
    if (this.#pageCount === 0 || !this.#renderer.isReady) return;

    for (const i of [...this.#visiblePages].sort((a, b) => a - b)) {
      try {
        this.#renderAndDisplay(i);
      } catch (err) {
        console.error(`Failed to render page ${i + 1}`, err);
      }
    }

    // Sync current-page state.
    const page = this.#pageNearestCenter();
    if (page !== null && page !== this.#currentPage) {
      this.#setCurrentPage(page);
    }

    // Kick off background prefetch.
    this.#schedulePrefetch(this.#currentPage);
  }

  /** The visible page whose box is closest to the viewport centre. */
  #pageNearestCenter() {
    const viewport = this.#scrollContainer.getBoundingClientRect();
    const center = viewport.top + viewport.height / 2;
    let best = null;
    let bestDistance = Number.POSITIVE_INFINITY;

    for (const index of this.#visiblePages) {
      const rect = this.#wrapper(index)?.getBoundingClientRect();
      if (!rect) continue;
      const distance =
        center < rect.top ? rect.top - center : center > rect.bottom ? center - rect.bottom : 0;
      if (distance < bestDistance) {
        bestDistance = distance;
        best = index;
      }
    }
    return best;
  }

  // ==================================================================
  // Text Selection
  // ==================================================================

  /**
   * Hit-test the page under a pointer event against the Rust text layout.
   *
   * @param {PointerEvent} e
   * @returns {number[]|null} `[page, layoutRevision, glyphIndex]`
   */
  #hitTest(e) {
    const wrapper = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest('.spdf-page-wrapper');
    if (!wrapper || !this.#scrollContent.contains(wrapper)) return null;

    const pageIndex = Number(wrapper.dataset.pageIndex);
    const rect = wrapper.getBoundingClientRect();
    const { width, height } = this.#scaledPageSizeForPage(pageIndex);
    return this.#renderer.hitTestText(
      pageIndex,
      width,
      height,
      e.clientX - rect.left,
      e.clientY - rect.top
    );
  }

  /** Coalesce selection changes into one DOM update per frame. */
  #scheduleSelection(full = false) {
    this.#fullSelection ||= full;
    if (this.#selectionFrame !== null || this.#pageCount === 0) return;
    this.#selectionFrame = requestAnimationFrame(() => {
      this.#selectionFrame = null;
      try {
        this.#updateSelectionHighlights();
      } catch (err) {
        console.error('Failed to update selection', err);
      }
    });
  }

  /** Apply changed highlight batches, reusing nodes by their stable keys. */
  #updateSelectionHighlights() {
    if (!this.#renderer.isReady || this.#pageCount === 0) return;
    const batches = this.#renderer.selectionUpdates(
      [...this.#visiblePages],
      this.#fullSelection ? undefined : this.#selectionRevision
    );

    for (const batch of batches) {
      const wrapper = this.#wrapper(batch.page);
      const layer = wrapper?.querySelector('.spdf-selection-layer');
      if (!layer) continue;

      // Highlights are in the retained layout's device space; scale the layer
      // to the displayed page size instead of recomputing every rectangle.
      layer.dataset.deviceWidth = String(batch.device_size[0]);
      layer.dataset.deviceHeight = String(batch.device_size[1]);
      this.#scaleSelectionLayer(batch.page);

      let nodes = this.#highlights.get(batch.page);
      if (!nodes) {
        nodes = new Map();
        this.#highlights.set(batch.page, nodes);
      }
      const retained = new Set(batch.keys);
      for (const [key, node] of nodes) {
        if (!retained.has(key)) {
          node.remove();
          nodes.delete(key);
        }
      }
      batch.keys.forEach((key, index) => {
        let node = nodes.get(key);
        if (!node) {
          node = document.createElement('div');
          node.className = 'spdf-selection-rect';
          layer.appendChild(node);
          nodes.set(key, node);
        }
        const [left, top, right, bottom] = batch.bounds.slice(index * 4, index * 4 + 4);
        node.style.left = `${left}px`;
        node.style.top = `${top}px`;
        node.style.width = `${right - left}px`;
        node.style.height = `${bottom - top}px`;
      });
    }

    const latest = batches.at(-1);
    if (latest) this.#selectionRevision = latest.selection_revision;
    this.#fullSelection = false;
  }

  /** Map a page's selection layer from layout device pixels to the displayed size. */
  #scaleSelectionLayer(pageIndex) {
    const layer = this.#wrapper(pageIndex)?.querySelector('.spdf-selection-layer');
    if (!layer) return;
    const deviceWidth = Number(layer.dataset.deviceWidth);
    const deviceHeight = Number(layer.dataset.deviceHeight);
    if (!deviceWidth || !deviceHeight) return;
    const { width, height } = this.#scaledPageSizeForPage(pageIndex);
    layer.style.width = `${deviceWidth}px`;
    layer.style.height = `${deviceHeight}px`;
    layer.style.transform = `scale(${width / deviceWidth}, ${height / deviceHeight})`;
  }

  // ==================================================================
  // Prefetching
  // ==================================================================

  #schedulePrefetch(currentPage) {
    const zoomKey = this.#zoom.toFixed(4);
    const newQueue = this.#renderer
      .getPrefetchPages(currentPage)
      .filter((idx) => this.#wrapper(idx)?.dataset.zoom !== zoomKey);

    // Always replace the queue so an in-flight idle callback picks up
    // pages relevant to the current scroll position, not a stale one.
    this.#prefetchQueue = newQueue;

    if (this.#isPrefetching) return; // callback already scheduled

    if (this.#prefetchQueue.length > 0) {
      this.#isPrefetching = true;
      _requestIdleCallback(
        (deadline) => this.#processPrefetchQueue(deadline),
        { timeout: PREFETCH_TIMEOUT }
      );
    }
  }

  #processPrefetchQueue(deadline) {
    while (
      this.#prefetchQueue.length > 0 &&
      deadline.timeRemaining() > PREFETCH_MIN_IDLE
    ) {
      const idx = this.#prefetchQueue.shift();
      try {
        this.#renderAndDisplay(idx);
      } catch (err) {
        console.warn(`Prefetch failed for page ${idx + 1}`, err);
      }
    }

    if (this.#prefetchQueue.length > 0) {
      _requestIdleCallback(
        (deadline) => this.#processPrefetchQueue(deadline),
        { timeout: PREFETCH_TIMEOUT }
      );
    } else {
      this.#isPrefetching = false;
    }
  }

  // ==================================================================
  // Event Handlers
  // ==================================================================

  #handlePointerDown(e) {
    if (this.#pageCount === 0 || e.button !== 0 || e.isPrimary === false) return;

    const anchor = this.#hitTest(e);
    if (!anchor) {
      this.clearSelection();
      return;
    }

    e.preventDefault();
    this.#scrollContainer.focus({ preventScroll: true });
    this.#renderer.select([...anchor, ...anchor]);
    this.#selectionDrag = { anchor, pointerId: e.pointerId };
    this.#scheduleSelection();
  }

  #handlePointerMove(e) {
    if (this.#selectionDrag?.pointerId !== e.pointerId) return;

    const focus = this.#hitTest(e);
    if (!focus) return;

    e.preventDefault();
    this.#renderer.select([...this.#selectionDrag.anchor, ...focus]);
    this.#scheduleSelection();
  }

  #handlePointerUp(e) {
    if (!(e instanceof PointerEvent) || this.#selectionDrag?.pointerId === e.pointerId) {
      this.#selectionDrag = null;
    }
  }

  #handleCopy(e) {
    if (this.#isEditableEventTarget(e.target)) return;

    const text = this.getSelectedText();
    if (!text || !e.clipboardData) return;

    e.preventDefault();
    e.clipboardData.setData('text/plain', text);
  }

  /** @param {KeyboardEvent} e */
  #handleKeydown(e) {
    if (this.#pageCount === 0 || this.#options.keyboardNav === false) return;

    // Guard: e.target can be null or a non-HTMLElement for document-level
    // key events. Also skip text-input elements to avoid hijacking typing.
    if (this.#isEditableEventTarget(e.target)) return;

    switch (e.key) {
      case 'ArrowDown':
      case 'PageDown':
        e.preventDefault();
        this.nextPage();
        break;
      case 'ArrowUp':
      case 'PageUp':
        e.preventDefault();
        this.previousPage();
        break;
      case 'Home':
        e.preventDefault();
        this.goToPage(0);
        break;
      case 'End':
        e.preventDefault();
        this.goToPage(this.#pageCount - 1);
        break;
    }
  }

  #handleResize() {
    if (this.#zoomMode !== 'fixed' && this.#pageCount > 0) {
      this.setZoom(this.#zoomMode);
    }
  }

  // ==================================================================
  // Helpers
  // ==================================================================

  #showLoading(show) {
    this.#loadingOverlay?.classList.toggle('spdf-hidden', !show);
  }

  #isEditableEventTarget(target) {
    if (!(target instanceof HTMLElement)) return false;

    const tag = target.tagName;
    return (
      tag === 'INPUT' ||
      tag === 'SELECT' ||
      tag === 'TEXTAREA' ||
      target.isContentEditable
    );
  }

  /** Update current page and dispatch event. */
  #setCurrentPage(index) {
    if (index < 0 || index >= this.#pageCount) return;
    this.#currentPage = index;
    this.dispatchEvent(
      new CustomEvent('pagechange', {
        detail: { page: index, pageNumber: index + 1 },
      })
    );
  }

  #emitError(message, error) {
    this.dispatchEvent(
      new CustomEvent('error', { detail: { message, error } })
    );
  }
}
