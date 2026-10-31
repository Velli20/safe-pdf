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
  onError?(error: unknown): void;
}
