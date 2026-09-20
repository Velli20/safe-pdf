/**
 * @module safe-pdf-renderer
 *
 * Low-level PDF rendering engine backed by the Safe-PDF WASM module.
 *
 * Provides a clean, DOM-minimal API for loading and rendering PDF pages
 * using WebGL and Skia (compiled to WebAssembly via Emscripten), plus the
 * document-wide text selection owned by the Rust side.
 *
 * @example
 * ```js
 * import { SafePdfRenderer } from './safe-pdf-renderer.js';
 *
 * const renderer = new SafePdfRenderer();
 * await renderer.init('./dist/emscripten.js');
 *
 * const { pageCount } = renderer.loadPdf(pdfArrayBuffer);
 * renderer.renderPageToCanvas(0, 800, 600);
 * context.drawImage(renderer.canvas, 0, 0);
 *
 * renderer.freePdf();
 * renderer.destroy();
 * ```
 */

/** Default WebGL context attributes for Emscripten/Skia compatibility. */
const DEFAULT_GL_ATTRIBUTES = {
  alpha: true,
  depth: true,
  stencil: false,
  antialias: true,
  preserveDrawingBuffer: true,
  preferLowPowerToHighPerformance: false,
  failIfMajorPerformanceCaveat: false,
  majorVersion: 2,
  minorVersion: 0,
  enableExtensionsByDefault: true,
  explicitSwapControl: false,
  proxyContextToMainThread: 0,
  renderViaOffscreenBackBuffer: false,
};

const utf8 = new TextDecoder('utf-8');

/**
 * @typedef {object} SelectionBatch
 * @property {number}   page               Zero-based page index.
 * @property {number}   layout_revision    Revision of the retained text layout.
 * @property {number}   selection_revision Selection revision this batch reflects.
 * @property {number[]} device_size        `[width, height]` the layout was recorded at.
 * @property {number[]} keys               Stable highlight identities, one per rectangle.
 * @property {number[]} bounds             Packed `[left, top, right, bottom]` in layout device space.
 */

/**
 * Low-level PDF rendering engine.
 *
 * Manages a hidden `<canvas>` element, a WebGL context, and the Safe-PDF
 * WASM module. Renders individual PDF pages into that canvas and exposes
 * the Rust-owned text selection.
 */
export class SafePdfRenderer {
  /** @type {HTMLCanvasElement} */
  #canvas;

  /** @type {boolean} Whether the canvas was created internally. */
  #ownsCanvas;

  /** @type {number|null} Emscripten GL context handle. */
  #glHandle = null;

  /** @type {object|null} The instantiated Emscripten module. */
  #wasmModule = null;

  /** @type {boolean} */
  #initialized = false;

  /** @type {number|null} Pointer to PDF data in WASM linear memory. */
  #pdfDataPtr = null;

  /** @type {number} Number of pages in the loaded PDF. */
  #pageCount = 0;

  // ---- WASM function bindings ----
  #sk_load_pdf = null;
  #sk_get_page_count = null;
  #sk_render_page = null;
  #sk_free_pdf = null;
  #sk_get_cache_count = null;
  #sk_clear_cache = null;
  #sk_reset_gpu = null;
  #sk_get_prefetch_count = null;
  #sk_get_prefetch_page = null;
  #sk_get_page_width = null;
  #sk_get_page_height = null;
  #sk_get_scratch_ptr = null;
  #sk_hit_test_text = null;
  #sk_select = null;
  #sk_build_selection_updates = null;
  #sk_build_selected_text = null;

  /**
   * Create a new SafePdfRenderer.
   *
   * @param {object}            [options]
   * @param {HTMLCanvasElement}  [options.canvas]  Existing canvas element to
   *   render into. If omitted a hidden canvas is created and appended to the
   *   document body automatically.
   */
  constructor(options = {}) {
    if (options.canvas instanceof HTMLCanvasElement) {
      this.#canvas = options.canvas;
      this.#ownsCanvas = false;
    } else {
      this.#canvas = document.createElement('canvas');
      this.#canvas.width = 800;
      this.#canvas.height = 600;
      this.#canvas.style.position = 'fixed';
      this.#canvas.style.top = '-9999px';
      this.#canvas.style.left = '-9999px';
      this.#canvas.style.pointerEvents = 'none';
      document.body.appendChild(this.#canvas);
      this.#ownsCanvas = true;
    }
  }

  // ==================================================================
  // Public API
  // ==================================================================

  /**
   * Instantiate the WASM module and create the WebGL context.
   *
   * @param {string} wasmUrl  URL to the Emscripten-generated ES module
   *   (e.g. `'./dist/emscripten.js'`).
   * @returns {Promise<void>}  Resolves when the renderer is ready.
   * @throws {Error} If the WebGL context cannot be created or the WASM
   *   module fails to load.
   */
  async init(wasmUrl) {
    if (this.#initialized) {
      throw new Error('SafePdfRenderer is already initialized');
    }

    // Resolve like a script tag would: relative to the document, not this module.
    const moduleUrl = new URL(wasmUrl, document.baseURI).href;
    const { default: createModule } = await import(moduleUrl);
    this.#wasmModule = await createModule({ canvas: this.#canvas, noInitialRun: true });
    this.#bindWasmFunctions();
    this.#initWebGL();
    this.#initialized = true;
  }

  /**
   * Whether the renderer has been initialised and is ready to use.
   * @returns {boolean}
   */
  get isReady() {
    return this.#initialized;
  }

  /**
   * The internal `<canvas>` element used for off-screen rendering.
   * @returns {HTMLCanvasElement}
   */
  get canvas() {
    return this.#canvas;
  }

  /**
   * Load a PDF document from raw bytes.
   *
   * Any previously loaded PDF is freed first.
   *
   * @param {ArrayBuffer} arrayBuffer  Raw PDF file data.
   * @returns {{ pageCount: number }}  Basic information about the document.
   * @throws {Error} If the PDF cannot be parsed.
   */
  loadPdf(arrayBuffer) {
    this.#assertReady();
    this.freePdf();

    const bytes = new Uint8Array(arrayBuffer);
    this.#pdfDataPtr = this.#wasmModule._malloc(bytes.length);
    this.#wasmModule.HEAPU8.set(bytes, this.#pdfDataPtr);

    const result = this.#sk_load_pdf(this.#pdfDataPtr, bytes.length);
    if (result < 0) {
      this.#wasmModule._free(this.#pdfDataPtr);
      this.#pdfDataPtr = null;
      throw new Error(`Failed to parse PDF (error code: ${result})`);
    }

    this.#pageCount = this.#sk_get_page_count();
    return { pageCount: this.#pageCount };
  }

  /**
   * Number of pages in the currently loaded PDF, or `0` if none is loaded.
   * @returns {number}
   */
  getPageCount() {
    return this.#pageCount;
  }

  /**
   * Render a single page to the internal canvas. Callers read the canvas
   * pixels directly, e.g. via `drawImage(renderer.canvas, 0, 0)`.
   *
   * @param {number} pageIndex  Zero-based page index.
   * @param {number} width      Target width in device pixels.
   * @param {number} height     Target height in device pixels.
   * @throws {RangeError} If `pageIndex` is out of bounds.
   * @throws {Error}      If the WASM render call fails.
   */
  renderPageToCanvas(pageIndex, width, height) {
    this.#assertPage(pageIndex);

    // Resize the canvas (and reinitialise WebGL) when the target size changes.
    if (this.#canvas.width !== width || this.#canvas.height !== height) {
      // Drop the Skia DirectContext BEFORE the GL context is invalidated by
      // the canvas resize.  Skia caches GPU resources (textures, programs,
      // buffers) that become stale when the WebGL context is reset.
      this.#sk_reset_gpu();

      this.#canvas.width = width;
      this.#canvas.height = height;
      this.#initWebGL();
    }

    this.#wasmModule.GL.makeContextCurrent(this.#glHandle);

    const result = this.#sk_render_page(width, height, pageIndex);
    if (result !== 0) {
      throw new Error(`Render failed for page ${pageIndex} (code ${result})`);
    }
  }

  /**
   * Return the displayed dimensions of a PDF page in PDF points, honoring
   * the CropBox and `/Rotate` exactly as rendering does.
   *
   * @param {number} pageIndex  Zero-based page index.
   * @returns {{ width: number, height: number } | null}
   *   Page dimensions, or `null` if the page has no page box or the index
   *   is out of range.
   */
  getPageDimensions(pageIndex) {
    this.#assertReady();
    const width = this.#sk_get_page_width(pageIndex);
    const height = this.#sk_get_page_height(pageIndex);
    if (width === 0 || height === 0) return null;
    return { width, height };
  }

  /**
   * Return the number of pages currently in the WASM-level render cache.
   * This is a single O(1) WASM call.
   *
   * @returns {number}
   */
  getCacheCount() {
    this.#assertReady();
    return this.#sk_get_cache_count();
  }

  /** Clear the WASM-level render cache for all pages. */
  clearCache() {
    this.#assertReady();
    this.#sk_clear_cache();
  }

  /**
   * Hit-test a point on a page displayed at `width`×`height` device pixels.
   *
   * @param {number} pageIndex
   * @param {number} width
   * @param {number} height
   * @param {number} x  Page-local x in device pixels.
   * @param {number} y  Page-local y in device pixels.
   * @returns {number[]|null} `[page, layoutRevision, glyphIndex]`, or `null`
   *   when no text is near the point.
   */
  hitTestText(pageIndex, width, height, x, y) {
    this.#assertPage(pageIndex);
    const length = this.#sk_hit_test_text(pageIndex, Math.round(width), Math.round(height), x, y);
    return length ? JSON.parse(this.#readScratch(length)) : null;
  }

  /**
   * Set the selection from two hit-test triples, or clear it with an empty array.
   *
   * @param {number[]} endpoints  `[]` or `[...anchor, ...focus]`.
   * @returns {number} The new selection revision.
   */
  select(endpoints) {
    this.#assertReady();
    this.#assertPdfLoaded();
    const bytes = endpoints.length * 4;
    const ptr = bytes ? this.#wasmModule._malloc(bytes) : 0;
    try {
      if (bytes) this.#wasmModule.HEAPU32.set(endpoints, ptr / 4);
      const revision = this.#sk_select(ptr, endpoints.length);
      if (revision < 0) throw new Error('Invalid selection endpoints');
      return revision;
    } finally {
      if (ptr) this.#wasmModule._free(ptr);
    }
  }

  /**
   * Return highlight batches for the visible pages whose selection changed.
   *
   * @param {number[]} visible  Visible page indices.
   * @param {number} [since]    Only pages changed after this selection revision;
   *   omit for a full snapshot.
   * @returns {SelectionBatch[]}
   */
  selectionUpdates(visible, since) {
    this.#assertReady();
    this.#assertPdfLoaded();
    const bytes = visible.length * 4;
    const ptr = bytes ? this.#wasmModule._malloc(bytes) : 0;
    try {
      if (bytes) this.#wasmModule.HEAPU32.set(visible, ptr / 4);
      // The revision parameter is a 64-bit integer on the C ABI.
      const length = this.#sk_build_selection_updates(ptr, visible.length, BigInt(since ?? -1));
      return length ? JSON.parse(this.#readScratch(length)) : [];
    } finally {
      if (ptr) this.#wasmModule._free(ptr);
    }
  }

  /**
   * Return the selected text, or an empty string.
   * @returns {string}
   */
  selectedText() {
    this.#assertReady();
    if (this.#pdfDataPtr === null) return '';
    return this.#readScratch(this.#sk_build_selected_text());
  }

  /**
   * Return an ordered list of page indices that should be prefetched given
   * the user's current reading position.
   *
   * @param {number} currentPage  Zero-based index of the currently visible page.
   * @returns {number[]}
   */
  getPrefetchPages(currentPage) {
    this.#assertReady();

    const count = this.#sk_get_prefetch_count(currentPage);
    const pages = [];

    for (let i = 0; i < count; i++) {
      const idx = this.#sk_get_prefetch_page(currentPage, i);
      if (idx !== 0xFFFFFFFF && idx < this.#pageCount) {
        pages.push(idx);
      }
    }

    return pages;
  }

  /**
   * Free the currently loaded PDF and release its WASM memory.
   * Safe to call even when no PDF is loaded.
   */
  freePdf() {
    if (this.#pdfDataPtr !== null && this.#wasmModule) {
      this.#sk_free_pdf();
      this.#wasmModule._free(this.#pdfDataPtr);
      this.#pdfDataPtr = null;
      this.#pageCount = 0;
    }
  }

  /**
   * Destroy the renderer and release **all** resources (WASM memory, canvas,
   * WebGL context).  The instance cannot be reused after this call.
   */
  destroy() {
    this.freePdf();
    this.#deleteGLContext();

    if (this.#ownsCanvas && this.#canvas?.parentNode) {
      this.#canvas.parentNode.removeChild(this.#canvas);
    }

    this.#wasmModule = null;
    this.#initialized = false;
  }

  // ==================================================================
  // Private helpers
  // ==================================================================

  #assertReady() {
    if (!this.#initialized) {
      throw new Error('SafePdfRenderer not initialized — call init() first.');
    }
  }

  #assertPdfLoaded() {
    if (this.#pdfDataPtr === null) {
      throw new Error('No PDF loaded — call loadPdf() first.');
    }
  }

  #assertPage(pageIndex) {
    this.#assertReady();
    this.#assertPdfLoaded();
    if (pageIndex < 0 || pageIndex >= this.#pageCount) {
      throw new RangeError(
        `Page index ${pageIndex} out of range [0, ${this.#pageCount - 1}]`
      );
    }
  }

  /** Decode the WASM-side result buffer produced by the last `sk_build_*` call. */
  #readScratch(length) {
    if (!length) return '';
    const ptr = this.#sk_get_scratch_ptr();
    return utf8.decode(this.#wasmModule.HEAPU8.subarray(ptr, ptr + length));
  }

  /** Release the Emscripten GL handle so the context registry does not leak. */
  #deleteGLContext() {
    if (this.#glHandle !== null && this.#wasmModule) {
      this.#wasmModule.GL.deleteContext(this.#glHandle);
      this.#glHandle = null;
    }
  }

  /** Create (or recreate) the WebGL context on the internal canvas. */
  #initWebGL() {
    this.#deleteGLContext();

    const gl =
      this.#canvas.getContext('webgl2', DEFAULT_GL_ATTRIBUTES) ||
      this.#canvas.getContext('webgl', DEFAULT_GL_ATTRIBUTES);

    if (!gl) {
      throw new Error('Unable to create WebGL context');
    }

    const GL = this.#wasmModule.GL;
    this.#glHandle = GL.registerContext(gl, DEFAULT_GL_ATTRIBUTES);
    GL.makeContextCurrent(this.#glHandle);
  }

  /** Bind cwrap'd WASM exports to private fields. */
  #bindWasmFunctions() {
    const M = this.#wasmModule;
    this.#sk_load_pdf = M.cwrap('sk_load_pdf', 'number', ['number', 'number']);
    this.#sk_get_page_count = M.cwrap('sk_get_page_count', 'number', []);
    this.#sk_render_page = M.cwrap('sk_render_page', 'number', ['number', 'number', 'number']);
    this.#sk_free_pdf = M.cwrap('sk_free_pdf', null, []);
    this.#sk_get_cache_count = M.cwrap('sk_get_cache_count', 'number', []);
    this.#sk_clear_cache = M.cwrap('sk_clear_cache', null, []);
    this.#sk_reset_gpu = M.cwrap('sk_reset_gpu', null, []);
    this.#sk_get_prefetch_count = M.cwrap('sk_get_prefetch_count', 'number', ['number']);
    this.#sk_get_prefetch_page = M.cwrap('sk_get_prefetch_page', 'number', ['number', 'number']);
    this.#sk_get_page_width = M.cwrap('sk_get_page_width', 'number', ['number']);
    this.#sk_get_page_height = M.cwrap('sk_get_page_height', 'number', ['number']);
    this.#sk_get_scratch_ptr = M.cwrap('sk_get_scratch_ptr', 'number', []);
    this.#sk_hit_test_text = M.cwrap('sk_hit_test_text', 'number', ['number', 'number', 'number', 'number', 'number']);
    this.#sk_select = M.cwrap('sk_select', 'number', ['number', 'number']);
    this.#sk_build_selection_updates = M.cwrap('sk_build_selection_updates', 'number', ['number', 'number', 'number']);
    this.#sk_build_selected_text = M.cwrap('sk_build_selected_text', 'number', []);
  }
}
