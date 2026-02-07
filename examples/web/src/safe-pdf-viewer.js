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
  #pageWidth;
  #pageHeight;

  /** @type {Map<string, string>} Cache key → data URL */
  #imageCache = new Map();

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
    this.#pageWidth = options.pageWidth ?? DEFAULT_PAGE_WIDTH;
    this.#pageHeight = options.pageHeight ?? DEFAULT_PAGE_HEIGHT;
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

    const { height } = this.#scaledPageSize();
    const targetScroll = pageIndex * (height + PAGE_GAP);

    this.#scrollContainer.scrollTo({ top: targetScroll, behavior: 'smooth' });
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
    if (value === 'fit-width') {
      this.#zoomMode = 'fit-width';
      const available = this.#scrollContainer.clientWidth - 60;
      this.#zoom = available / this.#pageWidth;
    } else if (value === 'fit-page') {
      this.#zoomMode = 'fit-page';
      const availW = this.#scrollContainer.clientWidth - 60;
      const availH = this.#scrollContainer.clientHeight - 60;
      this.#zoom = Math.min(availW / this.#pageWidth, availH / this.#pageHeight);
    } else {
      this.#zoomMode = 'fixed';
      this.#zoom = Number(value) || 1;
    }

    // Flush caches.
    this.#imageCache.clear();
    this.#renderer.clearCache();

    const savedPage = this.#currentPage;

    this.#buildPageLayout();

    setTimeout(() => {
      this.goToPage(savedPage);
      this.#renderVisiblePages();
    }, 50);

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

    const { width, height } = this.#scaledPageSize();

    for (let i = 0; i < this.#pageCount; i++) {
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
    return { pageCount };
  }

  /** Runs after a successful load: builds layout, renders first pages. */
  #afterLoad() {
    this.#buildPageLayout();

    setTimeout(() => {
      try {
        this.#renderVisiblePages();
      } finally {
        this.#showLoading(false);
      }
    }, 50);
  }

  // ==================================================================
  // Rendering Pipeline
  // ==================================================================

  /** Calculate the pixel size of a page at the current zoom. */
  #scaledPageSize() {
    return {
      width: Math.round(this.#pageWidth * this.#zoom),
      height: Math.round(this.#pageHeight * this.#zoom),
    };
  }

  /**
   * Render a page (or serve from cache) and return its data URL.
   * @param {number} pageIndex
   * @returns {string}
   */
  #renderPageCached(pageIndex) {
    const key = `${pageIndex}-${this.#zoom}`;

    if (this.#imageCache.has(key)) {
      return this.#imageCache.get(key);
    }

    const { width, height } = this.#scaledPageSize();
    const dataUrl = this.#renderer.renderPage(pageIndex, width, height);
    this.#imageCache.set(key, dataUrl);
    return dataUrl;
  }

  /** Show a rendered image in the corresponding page wrapper. */
  #displayPageImage(pageIndex, dataUrl) {
    const wrapper = this.#scrollContent.children[pageIndex];
    if (!wrapper) return;

    let img = wrapper.querySelector('.spdf-page-img');
    if (!img) {
      const placeholder = wrapper.querySelector('.spdf-page-placeholder');
      if (placeholder) placeholder.remove();

      img = document.createElement('img');
      img.className = 'spdf-page-img';
      wrapper.insertBefore(img, wrapper.firstChild);
    }

    img.src = dataUrl;
  }

  // ==================================================================
  // Visible-Page Detection
  // ==================================================================

  /** Return the range of pages that are currently in (or near) the viewport. */
  #visiblePageRange() {
    const scrollTop = this.#scrollContainer.scrollTop;
    const viewportH = this.#scrollContainer.clientHeight;
    const { height } = this.#scaledPageSize();
    const step = height + PAGE_GAP;

    const first = Math.max(0, Math.floor(scrollTop / step) - RENDER_BUFFER);
    const last = Math.min(
      this.#pageCount - 1,
      Math.ceil((scrollTop + viewportH) / step) + RENDER_BUFFER
    );

    return { first, last };
  }

  /** Determine which page is "current" based on scroll position. */
  #pageFromScroll() {
    const scrollTop = this.#scrollContainer.scrollTop;
    const viewportH = this.#scrollContainer.clientHeight;
    const { height } = this.#scaledPageSize();
    const step = height + PAGE_GAP;
    const center = scrollTop + viewportH / 2;

    return Math.max(0, Math.min(this.#pageCount - 1, Math.floor(center / step)));
  }

  /** Render every page that is currently visible (or nearly visible). */
  #renderVisiblePages() {
    if (this.#pageCount === 0 || !this.#renderer.isReady) return;

    const { first, last } = this.#visiblePageRange();

    for (let i = first; i <= last; i++) {
      try {
        const dataUrl = this.#renderPageCached(i);
        this.#displayPageImage(i, dataUrl);
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
    if (this.#isPrefetching) return;

    const pages = this.#renderer.getPrefetchPages(currentPage);
    this.#prefetchQueue = pages.filter((idx) => {
      const key = `${idx}-${this.#zoom}`;
      return !this.#imageCache.has(key);
    });

    if (this.#prefetchQueue.length > 0) {
      // Set the flag immediately so rapid scroll events don't schedule
      // multiple overlapping idle callbacks.
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
        const dataUrl = this.#renderPageCached(idx);
        this.#displayPageImage(idx, dataUrl);
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
