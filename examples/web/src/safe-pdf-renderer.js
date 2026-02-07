/**
 * @module safe-pdf-renderer
 *
 * Low-level PDF rendering engine backed by the Safe-PDF WASM module.
 *
 * Provides a clean, DOM-minimal API for loading and rendering PDF pages
 * using WebGL and Skia (compiled to WebAssembly via Emscripten).
 *
 * @example
 * ```js
 * import { SafePdfRenderer } from './safe-pdf-renderer.js';
 *
 * const renderer = new SafePdfRenderer();
 * await renderer.init('./dist/emscripten.js');
 *
 * const { pageCount } = renderer.loadPdf(pdfArrayBuffer);
 * console.log(`Loaded ${pageCount} pages`);
 *
 * const dataUrl = renderer.renderPage(0, 800, 600);
 * document.querySelector('img').src = dataUrl;
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

/**
 * Low-level PDF rendering engine.
 *
 * Manages a hidden `<canvas>` element, a WebGL context, and the Safe-PDF
 * WASM module. Renders individual PDF pages and returns image data that
 * callers can display however they like.
 */
export class SafePdfRenderer {
  /** @type {HTMLCanvasElement} */
  #canvas;

  /** @type {boolean} Whether the canvas was created internally. */
  #ownsCanvas;

  /** @type {number|null} Emscripten GL context handle. */
  #glHandle = null;

  /** @type {object|null} Reference to the Emscripten `Module` object. */
  #wasmModule = null;

  /** @type {boolean} */
  #initialized = false;

  /** @type {number|null} Pointer to PDF data in WASM linear memory. */
  #pdfDataPtr = null;

  /** @type {number} Length of PDF data in bytes. */
  #pdfDataLength = 0;

  /** @type {number} Number of pages in the loaded PDF. */
  #pageCount = 0;

  // ---- WASM function bindings ----
  #sk_load_pdf = null;
  #sk_get_page_count = null;
  #sk_render_page = null;
  #sk_free_pdf = null;
  #sk_is_page_cached = null;
  #sk_get_cache_count = null;
  #sk_clear_cache = null;
  #sk_reset_gpu = null;
  #sk_get_prefetch_count = null;
  #sk_get_prefetch_page = null;

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
   * Initialise the WASM module and create the WebGL context.
   *
   * @param {string} wasmUrl  URL to the Emscripten-generated JS glue file
   *   (e.g. `'./dist/emscripten.js'`).
   * @returns {Promise<void>}  Resolves when the renderer is ready.
   * @throws {Error} If the WebGL context cannot be created or the WASM
   *   module fails to load.
   */
  async init(wasmUrl) {
    if (this.#initialized) {
      throw new Error('SafePdfRenderer is already initialized');
    }

    await this.#loadWasmModule(wasmUrl);
    this.#bindWasmFunctions();
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

    const uint8Array = new Uint8Array(arrayBuffer);
    this.#pdfDataLength = uint8Array.length;
    this.#pdfDataPtr = this.#wasmModule._malloc(this.#pdfDataLength);
    this.#wasmModule.HEAPU8.set(uint8Array, this.#pdfDataPtr);

    const result = this.#sk_load_pdf(this.#pdfDataPtr, this.#pdfDataLength);
    if (result < 0) {
      this.#wasmModule._free(this.#pdfDataPtr);
      this.#pdfDataPtr = null;
      this.#pdfDataLength = 0;
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
   * Render a single page to the internal canvas and return its contents as
   * a PNG data-URL string.
   *
   * @param {number} pageIndex  Zero-based page index.
   * @param {number} width      Target width in device pixels.
   * @param {number} height     Target height in device pixels.
   * @returns {string}          PNG data-URL of the rendered page.
   * @throws {RangeError} If `pageIndex` is out of bounds.
   * @throws {Error}      If the WASM render call fails.
   */
  renderPage(pageIndex, width, height) {
    this.#assertReady();
    this.#assertPdfLoaded();

    if (pageIndex < 0 || pageIndex >= this.#pageCount) {
      throw new RangeError(
        `Page index ${pageIndex} out of range [0, ${this.#pageCount - 1}]`
      );
    }

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

    this.#makeGLCurrent();

    const result = this.#sk_render_page(width, height, pageIndex);
    if (result !== 0) {
      throw new Error(`Render failed for page ${pageIndex} (code ${result})`);
    }

    return this.#canvas.toDataURL('image/png');
  }

  /**
   * Check whether a page is present in the WASM-level render cache.
   *
   * @param {number} pageIndex  Zero-based page index.
   * @returns {boolean}
   */
  isPageCached(pageIndex) {
    this.#assertReady();
    return this.#sk_is_page_cached(pageIndex) === 1;
  }

  /**
   * Return the number of pages currently in the WASM-level render cache.
   *
   * This is a single WASM call (O(1)), much cheaper than iterating over
   * all pages with {@link isPageCached}.
   *
   * @returns {number}
   */
  getCacheCount() {
    this.#assertReady();
    return this.#sk_get_cache_count();
  }

  /**
   * Clear the WASM-level render cache for all pages.
   */
  clearCache() {
    this.#assertReady();
    this.#sk_clear_cache();
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
      this.#pdfDataLength = 0;
      this.#pageCount = 0;
    }
  }

  /**
   * Destroy the renderer and release **all** resources (WASM memory, canvas,
   * WebGL context).  The instance cannot be reused after this call.
   */
  destroy() {
    this.freePdf();

    // Release the Emscripten GL context to avoid leaking entries in the
    // GL context registry and keeping GPU resources alive after teardown.
    if (
      this.#glHandle !== null &&
      typeof GL !== 'undefined' &&
      GL.deleteContext
    ) {
      GL.deleteContext(this.#glHandle);
      this.#glHandle = null;
    }

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

  /**
   * Dynamically load the Emscripten-generated JS file and wait for the
   * WASM module to finish initialising.
   */
  #loadWasmModule(wasmUrl) {
    return new Promise((resolve, reject) => {
      /* eslint-disable no-undef */
      window.Module = {
        noInitialRun: true,
        canvas: this.#canvas,
        onRuntimeInitialized: () => {
          this.#wasmModule = window.Module;
          this.#initWebGL();
          resolve();
        },
      };
      /* eslint-enable no-undef */

      const script = document.createElement('script');
      script.src = wasmUrl;
      script.onerror = () =>
        reject(new Error(`Failed to load WASM module from: ${wasmUrl}`));
      document.head.appendChild(script);
    });
  }

  /** Create (or recreate) the WebGL context on the internal canvas. */
  #initWebGL() {
    // Clean up the previous Emscripten GL handle so we don't leak entries
    // in the GL context registry.
    if (
      this.#glHandle !== null &&
      typeof GL !== 'undefined' &&
      GL.deleteContext
    ) {
      GL.deleteContext(this.#glHandle);
      this.#glHandle = null;
    }

    const gl =
      this.#canvas.getContext('webgl2', DEFAULT_GL_ATTRIBUTES) ||
      this.#canvas.getContext('webgl', DEFAULT_GL_ATTRIBUTES);

    if (!gl) {
      throw new Error('Unable to create WebGL context');
    }

    if (typeof GL !== 'undefined' && GL.registerContext && GL.makeContextCurrent) {
      this.#glHandle = GL.registerContext(gl, DEFAULT_GL_ATTRIBUTES);
      GL.makeContextCurrent(this.#glHandle);
    }
  }

  /** Ensure the Emscripten GL context is current. */
  #makeGLCurrent() {
    if (this.#glHandle !== null && typeof GL !== 'undefined') {
      GL.makeContextCurrent(this.#glHandle);
    }
  }

  /** Bind cwrap'd WASM exports to private fields. */
  #bindWasmFunctions() {
    const M = this.#wasmModule;
    this.#sk_load_pdf        = M.cwrap('sk_load_pdf',        'number', ['number', 'number']);
    this.#sk_get_page_count  = M.cwrap('sk_get_page_count',  'number', []);
    this.#sk_render_page     = M.cwrap('sk_render_page',     'number', ['number', 'number', 'number']);
    this.#sk_free_pdf        = M.cwrap('sk_free_pdf',        null,     []);
    this.#sk_is_page_cached  = M.cwrap('sk_is_page_cached',  'number', ['number']);
    this.#sk_get_cache_count = M.cwrap('sk_get_cache_count', 'number', []);
    this.#sk_clear_cache     = M.cwrap('sk_clear_cache',     null,     []);
    this.#sk_reset_gpu       = M.cwrap('sk_reset_gpu',       null,     []);
    this.#sk_get_prefetch_count = M.cwrap('sk_get_prefetch_count', 'number', ['number']);
    this.#sk_get_prefetch_page  = M.cwrap('sk_get_prefetch_page',  'number', ['number', 'number']);
  }
}
