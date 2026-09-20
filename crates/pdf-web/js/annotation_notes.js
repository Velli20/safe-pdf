import { AnnotationKind } from './annotation_models.js';
import { AnnotationComponent, element } from './annotation_dom.js';

/** Owns note visibility; linked popups resolve parent identities through the layer. */
export class NoteComponent extends AnnotationComponent {
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry
   * @param {import('./annotation_layer.js').NativeAnnotationLayer} layer */
  constructor(entry, layer) {
    const content = entry.content;
    if (content.kind !== AnnotationKind.Note && content.kind !== AnnotationKind.Popup) throw new Error('Expected note or popup');
    super(entry, content.kind === AnnotationKind.Note ? 'button' : 'div');
    this.layer = layer;
    this.open = content.kind === AnnotationKind.Popup ? content.data.open : content.data.properties?.open ?? false;
    /** Whether a popup has propagated its initial open state to its parent note. */
    this.parentInitialized = false;
    /** @type {HTMLDivElement | undefined} */
    this.popup = undefined;
    if (content.kind === AnnotationKind.Note) {
      if (this.node instanceof HTMLButtonElement) this.node.type = 'button';
      this.listen(this.node, 'click', event => {
        if (this.suppressClick) { event.preventDefault(); this.suppressClick = false; return; }
        this.open = !this.open;
        layer.updateNotes();
      });
    }
  }
  /** Parent note component for popups, when mounted on this layer. */
  parent() {
    const content = this.entry.content;
    if (content.kind !== AnnotationKind.Popup || content.data.parent == null) return undefined;
    const parent = this.layer.nodes.get(content.data.parent);
    return parent instanceof NoteComponent ? parent : undefined;
  }
  /** Lets a popup that opens in the source open its parent note once. */
  inheritOpen() {
    const parent = this.parent();
    if (this.parentInitialized || !parent) return;
    parent.open ||= this.open;
    this.parentInitialized = true;
  }
  update() {
    const content = this.entry.content;
    if (content.kind === AnnotationKind.Popup) {
      const parent = this.parent();
      this.container.hidden = parent ? !parent.open : !this.open;
      this.node.classList.add('native-popup');
      this.node.setAttribute('role', 'note');
      this.node.textContent = this.entry.text || parent?.entry.text || '';
      return;
    }
    if (content.kind !== AnnotationKind.Note) return;
    this.node.textContent = this.entry.label || 'Note';
    this.node.setAttribute('aria-expanded', String(this.open));
    if (this.entry.has_popup) {
      this.popup?.remove(); this.popup = undefined; return;
    }
    if (!this.popup) {
      this.popup = element('div', { className: 'native-popup' });
      this.popup.setAttribute('role', 'note'); this.container.append(this.popup);
    }
    this.popup.hidden = !this.open;
    this.popup.textContent = this.entry.text;
  }
}
