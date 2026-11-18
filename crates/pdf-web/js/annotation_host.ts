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
  /** Reports a painted widget held down (`true`) or released (`false`); the host selects its
   * down appearance, and the layer then redraws the widget's artwork. */
  onPress?(entry: WebAnnotationEntry, pressed: boolean): void;
  /** Starts or ends editing a painted text entry; the host leaves the entry's value out of
   * its artwork while `editing`, which the layer then redraws, so only the control shows it. */
  onEdit?(entry: WebAnnotationEntry, editing: boolean): void;
  /** Returns the font a painted text entry paints its value in, with the distance
   * between its baselines in ems, or nothing when no browser font can be built for it.
   * Called once per entry; the layer registers each family once as a `FontFace` and edits
   * the value in it, spacing lines as the artwork does. */
  loadFont?(entry: WebAnnotationEntry): { family: string, data: Uint8Array<ArrayBuffer>, lineHeight: number } | undefined;
  /** Draws the source artwork of an entry with `artwork` into `canvas`, sizing it to the
   * entry's local box at the page's pixel density, and may place `canvas` with inline
   * styles so its pixels land on the page canvas's pixel grid. */
  renderArtwork?(entry: WebAnnotationEntry, canvas: HTMLCanvasElement): void;
  onError?(error: unknown): void;
}
