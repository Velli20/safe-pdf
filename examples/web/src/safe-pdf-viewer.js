/**
 * @module safe-pdf-viewer
 *
 * Full-featured, embeddable PDF viewer component built on top of
 * {@link SafePdfRenderer}.
 *
 * Provides continuous-scroll page viewing, zoom controls, keyboard
 * navigation, and intelligent page prefetching — all encapsulated in a
 * single class that can be dropped into any web page.
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

/**
 * Padding around the scroll content (px). Must match the `padding` value in
 * `.spdf-scroll-content` CSS so that scroll-position calculations are correct.
 */
const SCROLL_PADDING = 20;

/** Number of extra pages to render above/below the viewport. */
const RENDER_BUFFER = 1;

/** Idle-callback timeout for prefetch work (ms). */
const PREFETCH_TIMEOUT = 1000;

/** Minimum remaining idle time to start rendering a prefetch page (ms). */
const PREFETCH_MIN_IDLE = 10;

/** Scroll debounce delay (ms). */
const SCROLL_DEBOUNCE = 100;

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
  background: #fff;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
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
 * @property {string}  wasmUrl             URL to the Emscripten JS glue file.
 * @property {number}  [pageWidth=612]     Default page width in PDF points.
 * @property {number}  [pageHeight=792]    Default page height in PDF points.
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

  /**
   * Precomputed scroll-top position (px) for the top edge of each page.
   * @type {number[]}
   */
  #pageScrollOffsets = [];

  /** @type {Set<string>} Cache keys (`${pageIndex}-${zoom}`) of already-rendered pages. */
  #imageCache = new Set();

  // ---- Prefetch ----
  /** @type {number[]} */
  #prefetchQueue = [];
  #isPrefetching = false;

  // ---- Scroll ----
  #scrollTimeout = null;

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
    this.#boundHandleScroll = this.#handleScroll.bind(this);
    this.#boundHandleKeydown = this.#handleKeydown.bind(this);
    this.#boundHandleResize = this.#handleResize.bind(this);

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
      const result = this.#loadBuffer(buffer);

      this.dispatchEvent(
        new CustomEvent('load', {
          detail: { pageCount: result.pageCount, fileName: file.name },
        })
      );

      this.#afterLoad();
      return result;
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
      const result = this.#loadBuffer(buffer);

      this.dispatchEvent(
        new CustomEvent('load', {
          detail: { pageCount: result.pageCount, fileName },
        })
      );

      this.#afterLoad();
      return result;
    } catch (err) {
      this.#emitError('Failed to load PDF buffer', err);
      this.#showLoading(false);
      throw err;
    }
  }

  /**
   * Scroll to and highlight a specific page.
   *
   * @param {number} pageIndex  Zero-based page index.
   */
  goToPage(pageIndex) {
    if (pageIndex < 0 || pageIndex >= this.#pageCount) return;

    this.#scrollContainer.scrollTo({
      top: this.#pageScrollOffsets[pageIndex],
      behavior: 'smooth',
    });
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

    // Flush caches.
    this.#imageCache.clear();
    this.#renderer.clearCache();

    const savedPage = this.#currentPage;

    this.#buildPageLayout();

    // Wait for the browser to finish laying out the rebuilt page wrappers
    // before scrolling and rendering.  Two rAF calls guarantee a full
    // layout pass has occurred — more reliable than an arbitrary setTimeout.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        this.goToPage(savedPage);
        this.#renderVisiblePages();
      });
    });

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
    this.#renderer.destroy();
    this.#container.innerHTML = '';
    this.#imageCache.clear();
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

  /** Build (or rebuild) the page placeholder grid for the current zoom. */
  #buildPageLayout() {
    this.#scrollContent.innerHTML = '';

    for (let i = 0; i < this.#pageCount; i++) {
      const { width, height } = this.#scaledPageSizeForPage(i);

      const wrapper = document.createElement('div');
      wrapper.className = 'spdf-page-wrapper';
      wrapper.style.width = `${width}px`;
      wrapper.style.height = `${height}px`;
      wrapper.dataset.pageIndex = i;

      const placeholder = document.createElement('div');
      placeholder.className = 'spdf-page-placeholder';
      placeholder.textContent = `Page ${i + 1}`;
      wrapper.appendChild(placeholder);

      const label = document.createElement('div');
      label.className = 'spdf-page-number';
      label.textContent = `Page ${i + 1}`;
      wrapper.appendChild(label);

      this.#scrollContent.appendChild(wrapper);
    }

    this.#computePageScrollOffsets();
  }

  // ==================================================================
  // Event Listeners
  // ==================================================================

  #attachEventListeners() {
    this.#scrollContainer.addEventListener(
      'scroll',
      this.#boundHandleScroll,
      { passive: true }
    );

    if (this.#options.keyboardNav !== false) {
      document.addEventListener('keydown', this.#boundHandleKeydown);
    }

    window.addEventListener('resize', this.#boundHandleResize);
  }

  #detachEventListeners() {
    this.#scrollContainer?.removeEventListener('scroll', this.#boundHandleScroll);
    document.removeEventListener('keydown', this.#boundHandleKeydown);
    window.removeEventListener('resize', this.#boundHandleResize);
  }

  // ==================================================================
  // Loading helpers
  // ==================================================================

  /**
   * Common path for loading a PDF buffer into the renderer and updating
   * internal state.
   */
  #loadBuffer(buffer) {
    this.#imageCache.clear();
    const { pageCount } = this.#renderer.loadPdf(buffer);
    this.#pageCount = pageCount;
    this.#currentPage = 0;

    // Query each page's true dimensions from the WASM/PDF layer.
    this.#pageSizes = [];
    for (let i = 0; i < pageCount; i++) {
      const dims = this.#renderer.getPageDimensions(i);
      this.#pageSizes.push(
        dims ?? { width: DEFAULT_PAGE_WIDTH, height: DEFAULT_PAGE_HEIGHT }
      );
    }

    return { pageCount };
  }

  /** Runs after a successful load: builds layout, renders first pages. */
  #afterLoad() {
    this.#buildPageLayout();

    // Use a double rAF to ensure the browser has completed layout of the
    // newly inserted page wrappers before we try to read scroll geometry.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        try {
          this.#renderVisiblePages();
        } finally {
          this.#showLoading(false);
        }
      });
    });
  }

  // ==================================================================
  // Rendering Pipeline
  // ==================================================================

  /**
   * Calculate the pixel size of a specific page at the current zoom.
   * Uses the page's true PDF dimensions if available, falling back to defaults.
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
   * Precompute the scroll-top position of the top edge of every page and
   * store the results in {@link #pageScrollOffsets}.  Must be called after
   * the page layout is (re)built or after the zoom changes.
   */
  #computePageScrollOffsets() {
    this.#pageScrollOffsets = [];
    let top = SCROLL_PADDING;
    for (let i = 0; i < this.#pageCount; i++) {
      this.#pageScrollOffsets.push(top);
      const { height } = this.#scaledPageSizeForPage(i);
      top += height + PAGE_GAP;
    }
  }

  /**
   * Render a page into the corresponding page wrapper using `drawImage`,
   * bypassing the expensive encode→data-URL→decode cycle.
   * Results are cached by page index + zoom level so each page is only
   * rendered once per zoom.
   *
   * @param {number} pageIndex
   */
  #renderAndDisplay(pageIndex) {
    const key = `${pageIndex}-${this.#zoom.toFixed(4)}`;
    if (this.#imageCache.has(key)) return;

    const { width, height } = this.#scaledPageSizeForPage(pageIndex);
    this.#renderer.renderPageToCanvas(pageIndex, width, height);

    const wrapper = this.#scrollContent.children[pageIndex];
    if (!wrapper) return;

    let displayCanvas = wrapper.querySelector('.spdf-page-canvas');
    if (!displayCanvas) {
      const placeholder = wrapper.querySelector('.spdf-page-placeholder');
      if (placeholder) placeholder.remove();

      displayCanvas = document.createElement('canvas');
      displayCanvas.className = 'spdf-page-canvas spdf-page-img';
      wrapper.insertBefore(displayCanvas, wrapper.firstChild);
    }

    displayCanvas.width = width;
    displayCanvas.height = height;
    displayCanvas.getContext('2d').drawImage(this.#renderer.canvas, 0, 0);

    this.#imageCache.add(key);
  }

  // ==================================================================
  // Visible-Page Detection
  // ==================================================================

  /** Return the range of pages that are currently in (or near) the viewport. */
  #visiblePageRange() {
    const scrollTop = this.#scrollContainer.scrollTop;
    const viewportH = this.#scrollContainer.clientHeight;
    const viewBottom = scrollTop + viewportH;

    // Linear scan: find the first and last pages whose bounding boxes
    // overlap the viewport.  Works correctly for mixed page heights.
    let first = 0;
    let last = this.#pageCount - 1;
    let foundFirst = false;

    for (let i = 0; i < this.#pageCount; i++) {
      const pageTop = this.#pageScrollOffsets[i];
      const { height } = this.#scaledPageSizeForPage(i);
      const pageBottom = pageTop + height;

      if (pageBottom > scrollTop && pageTop < viewBottom) {
        if (!foundFirst) {
          first = i;
          foundFirst = true;
        }
        last = i;
      } else if (foundFirst) {
        // Pages are ordered, so once we leave the viewport we're done.
        break;
      }
    }

    return {
      first: Math.max(0, first - RENDER_BUFFER),
      last: Math.min(this.#pageCount - 1, last + RENDER_BUFFER),
    };
  }

  /** Determine which page is "current" based on scroll position. */
  #pageFromScroll() {
    const scrollTop = this.#scrollContainer.scrollTop;
    const viewportH = this.#scrollContainer.clientHeight;
    const center = scrollTop + viewportH / 2;

    // Binary search for the last page whose top edge is at or above center.
    let lo = 0;
    let hi = this.#pageCount - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (this.#pageScrollOffsets[mid] <= center) {
        lo = mid;
      } else {
        hi = mid - 1;
      }
    }
    return lo;
  }

  /** Render every page that is currently visible (or nearly visible). */
  #renderVisiblePages() {
    if (this.#pageCount === 0 || !this.#renderer.isReady) return;

    const { first, last } = this.#visiblePageRange();

    for (let i = first; i <= last; i++) {
      try {
        this.#renderAndDisplay(i);
      } catch (err) {
        console.error(`Failed to render page ${i + 1}`, err);
      }
    }

    // Sync current-page state.
    const page = this.#pageFromScroll();
    if (page !== this.#currentPage) {
      this.#setCurrentPage(page);
    }

    // Kick off background prefetch.
    this.#schedulePrefetch(this.#currentPage);
  }

  // ==================================================================
  // Prefetching
  // ==================================================================

  #schedulePrefetch(currentPage) {
    const pages = this.#renderer.getPrefetchPages(currentPage);
    const newQueue = pages.filter((idx) => {
      const key = `${idx}-${this.#zoom.toFixed(4)}`;
      return !this.#imageCache.has(key);
    });

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
    if (this.#prefetchQueue.length === 0) {
      this.#isPrefetching = false;
      return;
    }

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

  #handleScroll() {
    if (this.#scrollTimeout) clearTimeout(this.#scrollTimeout);

    // Render immediately for a responsive feel.
    this.#renderVisiblePages();

    // …and once more after scrolling settles.
    this.#scrollTimeout = setTimeout(() => {
      this.#renderVisiblePages();
    }, SCROLL_DEBOUNCE);
  }

  /** @param {KeyboardEvent} e */
  #handleKeydown(e) {
    if (this.#pageCount === 0) return;

    // Guard: e.target can be null or a non-HTMLElement for document-level
    // key events. Also skip text-input elements to avoid hijacking typing.
    if (e.target instanceof HTMLElement) {
      const tag = e.target.tagName;
      if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
      if (e.target.isContentEditable) return;
    }

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

  /** Update current page and dispatch event. */
  #setCurrentPage(index) {
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
