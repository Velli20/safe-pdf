const DRAG_THRESHOLD_CSS_PX = 3;

/** Owns one layer's pointer capture and transient CSS translation. */
export class AnnotationDrag {
  /** @param {import('./annotation_layer.js').NativeAnnotationLayer} layer */
  constructor(layer) {
    this.layer = layer;
    this.abort = new AbortController();
    window.addEventListener('keydown', event => { if (event.key === 'Escape') this.cancel(); }, { signal: this.abort.signal });
    window.addEventListener('blur', () => this.cancel(), { signal: this.abort.signal });
  }
  attach(component) {
    const node = component.container;
    component.listen(node, 'pointerdown', event => this.start(component, event));
    component.listen(node, 'pointermove', event => this.move(event));
    component.listen(node, 'pointerup', event => { void this.finish(event); });
    component.listen(node, 'pointercancel', () => this.cancel());
    component.listen(node, 'lostpointercapture', () => this.cancel());
  }
  start(component, event) {
    if (!this.layer.snapshot) return;
    if (!component.entry.draggable || component.editor || event.button !== 0 || !event.isPrimary || event.target.closest('textarea, input, select, .native-popup')) return;
    this.cancel();
    this.session = { component, id: event.pointerId, x: event.clientX, y: event.clientY, active: false, delta: [0, 0], revision: this.layer.snapshot.annotationRevision, viewportRevision: this.layer.snapshot.viewportRevision };
    component.container.setPointerCapture(event.pointerId);
  }
  move(event) {
    const drag = this.session;
    if (!drag || drag.id !== event.pointerId) return;
    const dx = event.clientX - drag.x, dy = event.clientY - drag.y;
    if (!drag.active && Math.hypot(dx, dy) < DRAG_THRESHOLD_CSS_PX) return;
    drag.active = true;
    drag.delta = [dx, dy];
    drag.component.container.style.transform = `translate(${dx}px, ${dy}px) ${drag.component.baseTransform}`;
    event.preventDefault();
  }
  async finish(event) {
    const drag = this.session;
    if (!drag || drag.id !== event.pointerId) return;
    this.move(event);
    this.cancel();
    if (!drag.active) return;
    drag.component.suppressClick = true;
    let delta;
    try {
      delta = this.layer.toPageDelta(drag.component.entry, drag.delta[0], drag.delta[1]);
    } catch (error) {
      this.layer.onError(error);
      return;
    }
    await this.layer.submit(drag.component, { operation: 'translate', id: drag.component.entry.id, delta: { x: delta[0], y: delta[1] } }, drag.revision, drag.viewportRevision);
  }
  cancel() {
    const drag = this.session;
    this.session = undefined;
    if (!drag) return;
    drag.component.container.style.transform = drag.component.baseTransform;
    if (drag.component.container.hasPointerCapture(drag.id)) drag.component.container.releasePointerCapture(drag.id);
  }
  remove(component) { if (this.session?.component === component) this.cancel(); }
  dispose() { this.cancel(); this.abort.abort(); }
}
