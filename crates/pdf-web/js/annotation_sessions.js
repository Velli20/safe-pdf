import { AnnotationComponent, element } from './annotation_dom.js';

export const EditPhase = Object.freeze({ Clean: 'clean', Draft: 'draft', Composing: 'composing', Committing: 'committing', Rejected: 'rejected' });
/** @typedef {typeof EditPhase[keyof typeof EditPhase]} EditState */

/** Owns a free-text draft and its listeners. The host owns persisted text. */
export class FreeTextComponent extends AnnotationComponent {
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry
   * @param {import('./annotation_layer.js').NativeAnnotationLayer} layer */
  constructor(entry, layer) {
    super(entry);
    this.layer = layer;
    /** @type {EditState} */
    this.phase = EditPhase.Clean;
    this.listen(this.container, 'dblclick', event => {
      if (this.entry.editable) { event.preventDefault(); this.begin(); }
    });
    this.listen(this.container, 'keydown', event => {
      if (!this.editor && this.entry.editable && ['Enter', 'F2'].includes(event.key)) { event.preventDefault(); this.begin(); }
    });
  }
  update() { if (!this.editor) this.node.textContent = this.entry.text; }
  begin() {
    if (!this.layer.snapshot) return;
    if (this.editor) { this.editor.focus(); return; }
    this.layer.drag.cancel();
    this.revision = this.layer.snapshot.annotationRevision;
    this.viewportRevision = this.layer.snapshot.viewportRevision;
    this.phase = EditPhase.Draft;
    this.sessionAbort = new AbortController();
    const editor = element('textarea', { className: 'native-free-text-editor', value: this.entry.text });
    editor.setAttribute('aria-label', 'Edit annotation text');
    this.editor = editor;
    this.node.hidden = true;
    this.container.append(editor);
    const signal = this.sessionAbort.signal;
    const listen = (name, callback) => editor.addEventListener(name, callback, { signal });
    listen('compositionstart', () => { this.phase = EditPhase.Composing; });
    listen('compositionend', () => {
      this.phase = EditPhase.Draft;
      if (this.commitAfterComposition) { this.commitAfterComposition = false; void this.commit(); }
    });
    listen('input', () => { if (this.phase === EditPhase.Rejected) this.phase = EditPhase.Draft; });
    listen('blur', () => {
      if (this.phase === EditPhase.Composing) this.commitAfterComposition = true;
      else void this.commit();
    });
    listen('keydown', event => {
      if (event.isComposing) return;
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); this.cancel(); }
      else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) { event.preventDefault(); void this.commit(); }
    });
    editor.focus();
  }
  async commit() {
    if (!this.editor || this.phase === EditPhase.Committing || this.phase === EditPhase.Composing) return;
    this.phase = EditPhase.Committing;
    this.editor.readOnly = true;
    const accepted = await this.layer.submit(this, { operation: 'set_free_text_text', id: this.entry.id, text: this.editor.value }, this.revision, this.viewportRevision);
    if (this.abort.signal.aborted) return;
    if (accepted) this.close();
    else {
      this.phase = EditPhase.Rejected;
      this.editor.readOnly = false;
      this.editor.setAttribute('aria-invalid', 'true');
      // A retry is a new command against the latest authoritative snapshot.
      this.revision = this.layer.snapshot?.annotationRevision;
      this.viewportRevision = this.layer.snapshot?.viewportRevision;
    }
  }
  cancel() {
    if (this.phase === EditPhase.Committing) return;
    this.close();
  }
  close() {
    this.sessionAbort?.abort();
    this.editor?.remove(); this.editor = undefined;
    this.commitAfterComposition = false;
    this.phase = EditPhase.Clean;
    this.node.hidden = false; this.update();
  }
  dispose() { this.close(); super.dispose(); }
}
