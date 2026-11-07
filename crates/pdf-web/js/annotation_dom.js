export const LINE_SPACING = 1.2;
/** @param {import('./annotation_contract').Color | null} color */
export const rgb = color => color ? `rgb(${[color.r, color.g, color.b].map(v => Math.round(v * 255)).join(' ')})` : 'none';
/**
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Partial<HTMLElementTagNameMap[K]>} [attributes]
 * @returns {HTMLElementTagNameMap[K]}
 */
export const element = (tag, attributes = {}) => Object.assign(document.createElement(tag), attributes);

/** Owns a component's DOM nodes and listeners until dispose. */
export class AnnotationComponent {
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry
   * @param {keyof HTMLElementTagNameMap} [tag] */
  constructor(entry, tag = 'div') {
    this.entry = entry;
    this.abort = new AbortController();
    this.container = element('div', { className: 'native-annotation annotation' });
    this.node = element(tag, { className: 'native-control' });
    this.container.append(this.node);
    /** @type {HTMLTextAreaElement | undefined} */
    this.editor = undefined;
    /** CSS transform placing the container; drags prepend a translation. */
    this.baseTransform = '';
    this.suppressClick = false;
    this.type = '';
    /** Identifies the drawn content, so unchanged content is not redrawn. */
    this.signature = '';
    /** @type {import('./annotation_layer.js').NativeAnnotationLayer | undefined} */
    this.layer = undefined;
  }
  /** @template {keyof HTMLElementEventMap} K
   * @param {HTMLElement} node
   * @param {K} name
   * @param {(event: HTMLElementEventMap[K]) => void} callback */
  listen(node, name, callback) {
    node.addEventListener(name, callback, { signal: this.abort.signal });
  }
  /** Draws the source artwork into a canvas that moves with the node. Moves keep the
   * drawing; a new size or viewport draws a fresh canvas before swapping it in. */
  drawArtwork() {
    const { entry, node, layer } = this;
    if (!layer) return;
    const signature = `${entry.size.width}x${entry.size.height}@${layer.snapshot?.viewportRevision}`;
    if (this.signature === signature) return;
    const canvas = element('canvas', { className: 'native-artwork' });
    try {
      layer.renderArtwork(entry, canvas);
    } catch (error) { layer.onError(error); return; }
    node.classList.remove('native-stamp');
    node.replaceChildren(canvas);
    this.signature = signature;
  }
  /** Subclasses reconcile their native control or visual from the current entry. */
  update() {}
  dispose() {
    this.abort.abort();
    this.container.remove();
  }
}
