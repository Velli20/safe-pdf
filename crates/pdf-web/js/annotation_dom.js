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
  }
  /** @template {keyof HTMLElementEventMap} K
   * @param {HTMLElement} node
   * @param {K} name
   * @param {(event: HTMLElementEventMap[K]) => void} callback */
  listen(node, name, callback) {
    node.addEventListener(name, callback, { signal: this.abort.signal });
  }
  /** Subclasses reconcile their native control or visual from the current entry. */
  update() {}
  dispose() {
    this.abort.abort();
    this.container.remove();
  }
}
