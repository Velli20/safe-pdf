import { AnnotationComponent, rgb } from './annotation_dom.js';
import { AnnotationKind } from './annotation_models.js';
const SVG = 'http://www.w3.org/2000/svg';

/** @param {string} tag
 * @param {Record<string, string | number>} attributes */
const svgElement = (tag, attributes) => {
  const node = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, String(value));
  return node;
};

/** @param {import('./annotation_contract').ShapePaint} paint */
const paintAttributes = paint => ({
  fill: rgb(paint.fill), stroke: rgb(paint.stroke), 'stroke-width': paint.width, 'stroke-dasharray': paint.dash.join(' '),
  'fill-opacity': paint.fill?.a ?? 1, 'stroke-opacity': paint.stroke?.a ?? 1,
  ...(paint.round ? { 'stroke-linecap': 'round', 'stroke-linejoin': 'round' } : {}),
});

/** Draws Core-provided local-unit shapes; no PDF geometry is interpreted here.
 * @param {import('./annotation_contract').WebAnnotationEntry} entry */
export function drawSvg(entry) {
  const [w, h] = entry.size;
  const svg = svgElement('svg', { viewBox: `0 0 ${w} ${h}`, width: '100%', height: '100%', 'aria-hidden': 'true' });
  for (const shape of entry.shapes) {
    const paint = paintAttributes(shape.paint);
    switch (shape.shape) {
      case 'polyline': {
        const { points, closed } = shape.path;
        svg.append(svgElement(closed ? 'polygon' : 'polyline', { ...paint, points: points.map(p => `${p.x},${p.y}`).join(' ') }));
        break;
      }
      case 'ellipse': {
        const { left, top, right, bottom } = shape.bounds;
        svg.append(svgElement('ellipse', { ...paint, cx: (left + right) / 2, cy: (top + bottom) / 2, rx: (right - left) / 2, ry: (bottom - top) / 2 }));
        break;
      }
      case 'rect': {
        const { left, top, right, bottom } = shape.bounds;
        svg.append(svgElement('rect', { ...paint, x: left, y: top, width: right - left, height: bottom - top }));
        break;
      }
    }
  }
  return svg;
}

export class VisualComponent extends AnnotationComponent {
  /** @param {import('./annotation_contract').WebAnnotationEntry} entry
   * @param {import('./annotation_layer.js').NativeAnnotationLayer} layer */
  constructor(entry, layer) {
    super(entry, entry.content.kind === AnnotationKind.Link ? 'a' : 'div');
    this.signature = '';
    if (entry.content.kind === AnnotationKind.Link) this.listen(this.node, 'click', event => {
      if (this.suppressClick) {
        event.preventDefault();
        this.suppressClick = false;
        return;
      }
      if (!this.node.hasAttribute('href')) event.preventDefault();
      layer.onAction(this.entry);
    });
  }
  update() {
    const { entry, node } = this, kind = entry.content.kind;
    if (entry.unsupported != null) {
      node.textContent = entry.text || entry.unsupported; node.title = entry.unsupported; node.classList.add('native-unsupported');
    } else if (kind === AnnotationKind.Link) {
      if (entry.href && node instanceof HTMLAnchorElement) {
        node.href = entry.href;
        node.target = '_blank';
        node.rel = 'noopener noreferrer';
      } else node.removeAttribute('href');
      node.tabIndex = 0; node.classList.add('native-link');
    } else if (kind === AnnotationKind.Stamp) {
      node.textContent = entry.label; node.classList.add('native-stamp');
    } else if (this.signature !== entry.modified_revision) {
      node.replaceChildren(drawSvg(entry));
      this.signature = entry.modified_revision;
    }
  }
}
