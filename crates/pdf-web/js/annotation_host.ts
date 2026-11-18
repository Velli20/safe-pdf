// Browser-owned lifecycle types. Wire fields come exclusively from the generated contract.
import type { ActionTrigger, Operation, WebAnnotationEntry } from './annotation_contract';

export interface AnnotationLayerSnapshot {
  entries: WebAnnotationEntry[];
  annotationRevision: string;
  viewportRevision: number;
}

export interface AnnotationLayerCallbacks {
  onCommand(entry: WebAnnotationEntry, command: Operation, revision: string, viewportRevision: number): void | Promise<void>;
  /** Converts a container CSS displacement into a PDF page displacement for drags. */
  toPageDelta(entry: WebAnnotationEntry, dx: number, dy: number): [number, number];
  /** Reports an interaction that runs the entry's action; only listed `entry.triggers`
   * are reported, except `activate`, which buttons and links always report. */
  onAction?(entry: WebAnnotationEntry, trigger: ActionTrigger): void;
  /** Reports a canvas-painted widget held down (`true`) or released (`false`). */
  onPress?(entry: WebAnnotationEntry, pressed: boolean): void;
  /** Starts or ends editing a canvas-painted text entry; the host repaints its page with
   * the entry's value left out while `editing`, so only the control shows it. */
  onEdit?(entry: WebAnnotationEntry, editing: boolean): void;
  /** Returns the font a canvas-painted text entry paints its value in, with the distance
   * between its baselines in ems, or nothing when no browser font can be built for it.
   * Called once per entry; the layer registers each family once as a `FontFace` and edits
   * the value in it, spacing lines as the canvas does. */
  loadFont?(entry: WebAnnotationEntry): { family: string, data: Uint8Array<ArrayBuffer>, lineHeight: number } | undefined;
  /** Draws the source artwork of an entry with `artwork` into `canvas`, sizing it to the
   * entry's local box at the page's pixel density. */
  renderArtwork?(entry: WebAnnotationEntry, canvas: HTMLCanvasElement): void;
  onError?(error: unknown): void;
}
