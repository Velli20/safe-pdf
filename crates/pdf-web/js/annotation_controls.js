import { ControlKind } from './annotation_models.js';
import { AnnotationComponent, element, LINE_SPACING } from './annotation_dom.js';
import { EditPhase } from './annotation_sessions.js';

/** @type {Record<import('./annotation_contract').ControlTag, keyof HTMLElementTagNameMap>} */
const TAGS = { input: 'input', text_area: 'textarea', select: 'select', button: 'button' };

/** Owns form drafts, composition, option nodes, and control listeners for a Core-derived control. */
export class FormControl extends AnnotationComponent {
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry
   * @param {import('./annotation_layer.js').NativeAnnotationLayer} layer */
  constructor(entry, layer) {
    const model = entry.control;
    if (!model) throw new Error('Expected a resolved widget field');
    super(entry, TAGS[model.tag]);
    this.layer = layer;
    /** @type {import('./annotation_sessions.js').EditState} */
    this.phase = EditPhase.Clean;
    /** @type {string | undefined} */
    this.revision = undefined;
    /** @type {number | undefined} */
    this.viewportRevision = undefined;
    /** @type {HTMLDataListElement | undefined} */
    this.datalist = undefined;
    this.optionsRevision = '';
    this.initialScroll = false;
    this.commitAfterComposition = false;
    const node = this.node;
    if (node instanceof HTMLInputElement && model.input_type) node.type = model.input_type;
    if (node instanceof HTMLButtonElement) node.type = 'button';
    this.bindDraftEvents();
    this.listen(node, 'click', event => {
      if (this.control().kind === ControlKind.Button) { event.preventDefault(); layer.onAction(this.entry); }
    });
  }

  /** @returns {import('./annotation_contract').WidgetControl} */
  control() {
    if (!this.entry.control) throw new Error('Widget lost its resolved field');
    return this.entry.control;
  }

  /** Captures revisions at draft creation; composition defers change submission. */
  bindDraftEvents() {
    const node = this.node;
    this.listen(node, 'focus', () => {
      if (this.phase === EditPhase.Clean) {
        this.revision = this.layer.snapshot?.annotationRevision;
        this.viewportRevision = this.layer.snapshot?.viewportRevision;
      }
    });
    this.listen(node, 'input', () => {
      this.revision ??= this.layer.snapshot?.annotationRevision;
      this.viewportRevision ??= this.layer.snapshot?.viewportRevision;
      if (this.phase !== EditPhase.Composing) this.phase = EditPhase.Draft;
    });
    this.listen(node, 'compositionstart', () => { this.phase = EditPhase.Composing; });
    this.listen(node, 'compositionend', () => {
      this.phase = EditPhase.Draft;
      if (this.commitAfterComposition) { this.commitAfterComposition = false; void this.commit(); }
    });
    this.listen(node, 'change', () => {
      if (this.phase === EditPhase.Composing) this.commitAfterComposition = true;
      else void this.commit();
    });
    this.listen(node, 'keydown', event => {
      if (event.key === 'Escape' && this.phase !== EditPhase.Committing && this.phase !== EditPhase.Composing) {
        this.revision = undefined;
        this.phase = EditPhase.Clean;
        this.update();
      }
    });
  }

  /** @returns {import('./annotation_contract').Operation | undefined} */
  command() {
    const node = this.node, p = this.control(), field = p.field;
    if (node instanceof HTMLInputElement && (p.input_type === 'checkbox' || p.input_type === 'radio')) {
      if (p.kind === ControlKind.Checkbox) return { operation: 'set_checkbox', field, checked: node.checked };
      if (p.option == null) return;
      return { operation: p.kind === ControlKind.Radio ? 'set_radio_selection' : 'set_checkbox_group_selection', field, selected: node.checked ? p.option : null };
    }
    if (node instanceof HTMLSelectElement && p.options) {
      const options = p.options;
      const selected = Array.from(node.selectedOptions, option => options[option.index].id);
      if (p.kind === ControlKind.ComboBox) return { operation: 'set_combo_value', field, value: selected.length ? { kind: 'option', value: selected[0] } : { kind: 'empty' } };
      return { operation: 'set_list_selection', field, selection: p.multiple ? { mode: 'multiple', selected } : { mode: 'single', selected: selected[0] ?? null } };
    }
    if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
      if (p.kind === ControlKind.ComboBox) {
        const matches = (p.options ?? []).filter(option => option.export_value === node.value);
        return { operation: 'set_combo_value', field, value: matches.length === 1 ? { kind: 'option', value: matches[0].id } : { kind: 'text', value: node.value } };
      }
      return { operation: 'set_text', field, value: node.value };
    }
  }

  /** Owns the command promise; rejected text remains a draft, other controls restore model state. */
  async commit() {
    if (this.phase === EditPhase.Committing) return;
    const command = this.command();
    if (!command) return;
    this.phase = EditPhase.Committing;
    this.setPending(true);
    const accepted = await this.layer.submit(this, command, this.revision, this.viewportRevision);
    if (this.abort.signal.aborted) return;
    this.phase = accepted || !['set_text', 'set_combo_value'].includes(command.operation) ? EditPhase.Clean : EditPhase.Rejected;
    this.revision = this.layer.snapshot?.annotationRevision;
    this.viewportRevision = this.layer.snapshot?.viewportRevision;
    this.update();
  }

  /** Locks the native control while a submitted value is awaiting the host. */
  setPending(pending) {
    const node = this.node, readOnly = !this.entry.editable;
    if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) node.readOnly = readOnly || pending;
    if (node instanceof HTMLButtonElement || node instanceof HTMLSelectElement || (node instanceof HTMLInputElement && (node.type === 'checkbox' || node.type === 'radio'))) node.disabled = readOnly || pending;
  }

  update() {
    const p = this.control();
    this.setPending(this.phase === EditPhase.Committing);
    if (p.options) this.updateChoice(p);
    else if (p.input_type === 'checkbox' || p.input_type === 'radio' || p.tag === 'button') this.updateButton(p);
    else this.updateText(p);
    if (this.node instanceof HTMLInputElement || this.node instanceof HTMLTextAreaElement || this.node instanceof HTMLSelectElement) this.node.required = p.required;
    const angle = ((p.rotation % 360) + 360) % 360;
    this.node.style.lineHeight = String(LINE_SPACING);
    this.node.style.transformOrigin = 'center';
    this.node.style.transform = angle ? `rotate(${-angle}deg)` : '';
  }

  /** @param {import('./annotation_contract').WidgetControl} p */
  updateText(p) {
    const node = this.node;
    if (!(node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement)) return;
    if (this.phase === EditPhase.Clean && node.value !== p.value) node.value = p.value;
    if (p.max_length != null) node.maxLength = p.max_length;
    else node.removeAttribute('maxlength');
  }

  /** @param {import('./annotation_contract').WidgetControl} p */
  updateButton(p) {
    const node = this.node;
    if (node instanceof HTMLInputElement) {
      node.checked = p.checked;
      // PDF radios in unison may have several checked members; HTML grouping must not override /AS.
      node.name = `pdf-${this.entry.page}-${this.entry.id}`;
    } else if (node instanceof HTMLButtonElement) node.textContent = p.caption || p.label || 'Button';
  }

  /** @param {import('./annotation_contract').WidgetControl} p */
  updateChoice(p) {
    const node = this.node, options = p.options ?? [];
    if (!(node instanceof HTMLSelectElement || node instanceof HTMLInputElement)) return;
    let optionsNode;
    if (node instanceof HTMLInputElement) {
      this.updateText(p);
      if (!this.datalist) {
        this.datalist = element('datalist', { id: `pdf-options-${this.entry.page}-${this.entry.id}` });
        this.container.append(this.datalist); node.setAttribute('list', this.datalist.id);
      }
      optionsNode = this.datalist;
    } else optionsNode = node;
    if (this.optionsRevision !== this.entry.modified_revision) {
      optionsNode.replaceChildren(...options.map(option => element('option', { value: option.export_value, textContent: option.label })));
      this.optionsRevision = this.entry.modified_revision;
    }
    if (node instanceof HTMLSelectElement) this.updateSelection(node, p);
  }

  /** @param {HTMLSelectElement} node
   * @param {import('./annotation_contract').WidgetControl} p */
  updateSelection(node, p) {
    node.multiple = p.multiple;
    const rowHeight = this.entry.style.font_size * LINE_SPACING;
    node.size = p.list_box ? Math.max(2, Math.floor(this.entry.size[1] / rowHeight)) : 1;
    for (const option of node.options) option.selected = p.selected.includes(option.index);
    if (!p.selected.length) node.selectedIndex = -1;
    if (!this.initialScroll && p.list_box) { node.scrollTop = p.top_index * rowHeight; this.initialScroll = true; }
  }
}
