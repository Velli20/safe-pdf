import { ControlKind } from './annotation_models.js';
import { AnnotationComponent, element, LINE_SPACING } from './annotation_dom.js';
import { EditPhase } from './annotation_sessions.js';

/** @type {Record<import('./annotation_contract').ControlTag, keyof HTMLElementTagNameMap>} */
const TAGS = { input: 'input', text_area: 'textarea', select: 'select', button: 'button' };

/** DOM events that run a widget's pointer and focus trigger actions.
 * @type {Array<[keyof HTMLElementEventMap, import('./annotation_contract').ActionTrigger]>} */
const TRIGGER_EVENTS = [
  ['pointerenter', 'cursor_enter'], ['pointerleave', 'cursor_exit'], ['pointerdown', 'mouse_down'],
  ['pointerup', 'mouse_up'], ['focus', 'focus'], ['blur', 'blur'],
];

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
    /** Whether the host was told the widget is held down or edited. */
    this.presented = false;
    /** @type {HTMLCanvasElement | undefined} */
    this.artwork = undefined;
    const node = this.node;
    if (node instanceof HTMLInputElement && model.input_type) node.type = model.input_type;
    if (node instanceof HTMLButtonElement) node.type = 'button';
    this.bindDraftEvents();
    this.bindPressEvents();
    this.bindEditEvents();
    this.bindTriggerEvents();
    this.listen(node, 'click', event => {
      if (this.control().kind === ControlKind.Button) { event.preventDefault(); layer.onAction(this.entry, 'activate'); }
    });
  }

  /** Reports the pointer and focus triggers the entry has an action for. */
  bindTriggerEvents() {
    for (const [name, trigger] of TRIGGER_EVENTS) {
      this.listen(this.node, name, () => { if (this.entry.triggers.includes(trigger)) this.layer.onAction(this.entry, trigger); });
    }
  }

  /** Reports pointer presses on a painted widget so the host selects its down appearance,
   * then redraws its artwork. */
  bindPressEvents() {
    let pressed = false;
    const release = () => {
      if (!pressed) return;
      pressed = false;
      this.present(false, () => this.layer.onPress(this.entry, false));
    };
    this.listen(this.node, 'pointerdown', event => {
      const control = this.control();
      // Text entries have no down appearance; only buttons are pressed.
      if (event.button !== 0 || !control.painted || FormControl.isTextEntry(control) || !this.entry.editable && control.kind !== ControlKind.Button) return;
      pressed = true;
      this.present(true, () => this.layer.onPress(this.entry, true));
    });
    for (const name of /** @type {const} */ (['pointerup', 'pointercancel', 'pointerleave'])) this.listen(this.node, name, release);
  }

  /** Reports focus on a painted text entry so the host draws its artwork without its value
   * while the control shows the value being edited. `change` commits before `blur`. */
  bindEditEvents() {
    const edits = () => FormControl.isPaintedTextEntry(this.control());
    this.listen(this.node, 'focus', () => { if (edits()) this.present(true, () => this.layer.onEdit(this.entry, true)); });
    this.listen(this.node, 'blur', () => { if (edits()) this.present(false, () => this.layer.onEdit(this.entry, false)); });
  }

  /** Reports a changed presentation through `report`, then redraws the artwork in it.
   * @param {boolean} presented whether the widget is now held down or edited
   * @param {() => void} report */
  present(presented, report) {
    this.presented = presented;
    report();
    this.drawWidgetArtwork();
  }

  /** Draws the widget's appearance under its transparent control, for the live field state
   * of the current annotation revision and the presentation the host was told of. */
  drawWidgetArtwork() {
    if (this.entry.artwork) this.drawArtwork(`${this.layer.snapshot?.annotationRevision}:${this.presented}`);
    else { this.artwork?.remove(); this.artwork = undefined; this.signature = ''; }
  }

  /** Shows the artwork behind the control, which keeps its own content.
   * @param {HTMLCanvasElement} canvas */
  placeArtwork(canvas) {
    this.artwork?.remove();
    this.container.prepend(canvas);
    this.artwork = canvas;
  }

  /** Whether `control` takes typed text: a text field or a combo box.
   * @param {import('./annotation_contract').WidgetControl} control */
  static isTextEntry(control) { return control.kind === ControlKind.Text || control.kind === ControlKind.ComboBox; }

  /** Whether the artwork paints `control`'s value, which the control shows only while edited.
   * @param {import('./annotation_contract').WidgetControl | null | undefined} control */
  static isPaintedTextEntry(control) { return !!control?.painted && FormControl.isTextEntry(control); }

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
      this.updateCombStart(this.control());
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
    // A committed value activates the widget, running its action as a button click does.
    if (accepted && this.entry.triggers.includes('activate')) this.layer.onAction(this.entry, 'activate');
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
    this.node.classList.toggle('native-painted', p.painted);
    this.drawWidgetArtwork();
    this.setPending(this.phase === EditPhase.Committing);
    if (p.options) this.updateChoice(p);
    else if (p.input_type === 'checkbox' || p.input_type === 'radio' || p.tag === 'button') this.updateButton(p);
    else this.updateText(p);
    if (this.node instanceof HTMLInputElement || this.node instanceof HTMLTextAreaElement || this.node instanceof HTMLSelectElement) this.node.required = p.required;
    this.updateSpacing(p);
    this.updateRotation(p);
  }

  /** Spaces a painted text entry's text as its artwork paints its value, so it stays
   * in place while edited: inset by the control's `text_inset`, with lines the field font's
   * line height apart. Other controls keep the default spacing.
   * @param {import('./annotation_contract').WidgetControl} p */
  updateSpacing(p) {
    const style = this.node.style;
    if (!FormControl.isPaintedTextEntry(p)) {
      style.lineHeight = String(LINE_SPACING);
      style.padding = '';
      return;
    }
    style.lineHeight = String(this.layer.fieldFont(this.entry)?.lineHeight ?? LINE_SPACING);
    style.padding = `${p.text_inset}px`;
    style.fontKerning = p.comb == null ? '' : 'normal';
    this.updateCombStart(p);
  }

  /** Starts a comb field's characters in the cell the artwork paints the first one in: the
   * first cell, the cell that leaves the remaining cells after the value, or the middle of
   * those, by the field's alignment. The field font spaces the characters one per cell.
   * @param {import('./annotation_contract').WidgetControl} p */
  updateCombStart(p) {
    const node = this.node, cells = p.comb;
    if (cells == null || !(node instanceof HTMLInputElement)) return;
    const cell = (this.entry.size.width - 2 * p.text_inset) / cells;
    const free = Math.max(cells - [...node.value].length, 0);
    const start = { left: 0, center: Math.floor(free / 2), right: free }[this.entry.style.alignment];
    node.style.textAlign = 'left';
    node.style.paddingLeft = `${p.text_inset + start * cell}px`;
  }

  /** Rotates the control by its `/MK /R` angle; quarter turns swap its sides so the rotated box still covers the widget rectangle.
   * @param {import('./annotation_contract').WidgetControl} p */
  updateRotation(p) {
    const angle = ((p.rotation % 360) + 360) % 360, style = this.node.style;
    const { width, height } = this.entry.size, quarter = angle === 90 || angle === 270;
    style.width = quarter ? `${height}px` : '';
    style.height = quarter ? `${width}px` : '';
    style.transformOrigin = 'center';
    const shift = quarter ? `translate(${(width - height) / 2}px, ${(height - width) / 2}px) ` : '';
    style.transform = angle ? `${shift}rotate(${-angle}deg)` : '';
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
    } else if (node instanceof HTMLButtonElement) {
      node.textContent = p.painted ? '' : p.caption || p.label || 'Button';
      node.title = p.caption || p.label;
    }
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
    node.size = p.list_box ? Math.max(2, Math.floor(this.entry.size.height / rowHeight)) : 1;
    for (const option of node.options) option.selected = p.selected.includes(option.index);
    if (!p.selected.length) node.selectedIndex = -1;
    if (!this.initialScroll && p.list_box) { node.scrollTop = p.top_index * rowHeight; this.initialScroll = true; }
  }
}
