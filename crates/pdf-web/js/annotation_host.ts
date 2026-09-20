// Browser-owned lifecycle types. Wire fields come exclusively from the generated contract.
import type { Operation, WebAnnotationEntry } from './annotation_contract';

export interface AnnotationLayerSnapshot {
  entries: WebAnnotationEntry[];
  annotationRevision: string;
  viewportRevision: number;
}

export interface AnnotationLayerCallbacks {
  onCommand(entry: WebAnnotationEntry, command: Operation, revision: string, viewportRevision: number): void | Promise<void>;
  /** Converts a container CSS displacement into a PDF page displacement for drags. */
  toPageDelta(entry: WebAnnotationEntry, dx: number, dy: number): [number, number];
  onAction?(entry: WebAnnotationEntry): void;
  onError?(error: unknown): void;
}
