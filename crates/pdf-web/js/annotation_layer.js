import { AnnotationKind } from './annotation_models.js';
import { AnnotationDrag } from './annotation_drag.js';
import { FormControl } from './annotation_controls.js';
import { EditPhase, FreeTextComponent } from './annotation_sessions.js';
import { NoteComponent } from './annotation_notes.js';
import { VisualComponent } from './annotation_visuals.js';

/**
 * Owns mounted components and document-order reconciliation.
 * Snapshots carry placed entries with ready-to-assign CSS; commands carry PDF values
 * and captured revisions. onCommand may return a promise and must reject failed edits.
 * submit consumes that promise and reports each failure once through onError,
 * returning acceptance to its owner.
 */
export class NativeAnnotationLayer {
  /** @param {HTMLElement} root
   * @param {import('./annotation_host').AnnotationLayerCallbacks} callbacks */
  constructor(root, { onCommand, toPageDelta, onAction = () => { }, onError = () => { } }) {
    this.root = root; this.onCommand = onCommand; this.toPageDelta = toPageDelta; this.onAction = onAction; this.onError = onError;
    /** @type {Map<string, import('./annotation_dom.js').AnnotationComponent>} */
    this.nodes = new Map();
    /** @type {import('./annotation_host').AnnotationLayerSnapshot | undefined} */
    this.snapshot = undefined;
    root.classList.add('native-annotations');
    this.drag = new AnnotationDrag(this);
  }
  /** Advances document revision without projecting entries or reconciling DOM. */
  advanceRevision(revision) {
    if (this.snapshot) this.snapshot.annotationRevision = revision;
    for (const component of this.nodes.values()) {
      if (component instanceof FormControl && component.phase === EditPhase.Clean) component.revision = revision;
    }
  }
  /** @param {import('./annotation_host').AnnotationLayerSnapshot} snapshot */
  reconcile(snapshot) {
    if (this.snapshot && snapshot.viewportRevision !== this.snapshot.viewportRevision) this.drag.cancel();
    this.snapshot = snapshot;
    const entries = snapshot.entries.filter(entry => entry.visible);
    const retained = new Set();
    for (const entry of entries) {
      const key = entry.id;
      const type = entry.unsupported != null ? 'unsupported' : entry.control ? `control:${entry.control.tag}:${entry.control.input_type ?? ''}` : entry.content.kind;
      retained.add(key);
      let component = this.nodes.get(key);
      if (component && component.type !== type) { this.remove(component); this.nodes.delete(key); component = undefined; }
      if (!component) {
        component = this.create(entry);
        component.type = type;
        this.nodes.set(key, component); this.root.append(component.container);
        this.drag.attach(component);
      }
      component.entry = entry;
      this.update(component);
    }
    for (const [key, component] of this.nodes) if (!retained.has(key)) { this.remove(component); this.nodes.delete(key); }
    let cursor = this.root.firstElementChild;
    for (const entry of entries) {
      const node = this.nodes.get(entry.id)?.container;
      if (!node) continue;
      if (node !== cursor) this.root.insertBefore(node, cursor);
      cursor = node.nextElementSibling;
    }
    this.updateNotes();
  }
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry */
  create(entry) {
    if (entry.unsupported != null) return new VisualComponent(entry, this);
    if (entry.control) return new FormControl(entry, this);
    switch (entry.content.kind) {
      case AnnotationKind.FreeText: return new FreeTextComponent(entry, this);
      case AnnotationKind.Note: case AnnotationKind.Popup: return new NoteComponent(entry, this);
      default: return new VisualComponent(entry, this);
    }
  }
  /** @param {import('./annotation_dom.js').AnnotationComponent} component */
  update(component) {
    const { entry, node, container } = component, { style, css } = entry;
    component.baseTransform = css.transform;
    Object.assign(container.style, { transform: css.transform, width: `${entry.size[0]}px`, height: `${entry.size[1]}px`, opacity: style.opacity, color: css.color });
    container.dataset.annotationId = String(entry.id); container.dataset.kind = entry.content.kind;
    container.tabIndex = entry.editable || entry.draggable ? 0 : -1;
    container.setAttribute('aria-label', entry.text || entry.subtype);
    node.setAttribute('aria-label', entry.control?.label || entry.text || entry.subtype);
    const font = { fontSize: `${style.font_size}px`, fontFamily: style.font_family, textAlign: style.alignment };
    Object.assign(container.style, font);
    Object.assign(node.style, { ...font, color: css.color, backgroundColor: css.background, border: css.border });
    component.update();
  }
  /** Notes inherit a linked popup's initial open state once, then every note reconciles. */
  updateNotes() {
    const notes = [...this.nodes.values()].filter(component => component instanceof NoteComponent);
    for (const note of notes) note.inheritOpen();
    for (const note of notes) note.update();
  }
  /** @param {import('./annotation_dom.js').AnnotationComponent} component
   * @param {import('./annotation_contract').Operation} command */
  async submit(component, command, revision = this.snapshot?.annotationRevision, viewportRevision = this.snapshot?.viewportRevision) {
    try {
      if (revision == null || viewportRevision == null) throw new Error('Annotation layer is not mounted');
      await this.onCommand(component.entry, command, revision, viewportRevision);
      component.node.removeAttribute('aria-invalid');
      return true;
    } catch (error) {
      component.node.setAttribute('aria-invalid', 'true');
      this.onError(error);
      return false;
    }
  }
  remove(component) { this.drag.remove(component); component.dispose(); }
  /** Releases pointer capture, draft sessions, all component listeners, and nodes. */
  dispose() {
    this.drag.dispose();
    for (const component of this.nodes.values()) component.dispose();
    this.nodes.clear(); this.snapshot = undefined;
  }
}
