// What an output pane has to do to its DOM to catch up with its buffer.
//
// Separate from the component because this arithmetic is the whole of it, and getting
// it wrong is invisible until a build is long enough to wrap the buffer. Two rules:
// the mark a pane keeps is a count of lines *produced*, never of spans it holds, and
// the pane ends up holding exactly what the buffer still holds.

import type { Since } from './lineBuffer.ts';
import type { OutputLine } from './types.ts';

export interface PaneDraw {
	append: readonly OutputLine[];
	/** Spans to remove from the front, once `append` has landed. */
	trim: number;
	/** What the pane records as drawn. */
	mark: number;
}

export const paneDraw = (spans: number, since: Since): PaneDraw => ({
	append: since.lines,
	trim: Math.max(0, spans + since.lines.length - since.held),
	mark: since.produced,
});

export const nothingToDraw = (draw: PaneDraw): boolean => draw.append.length === 0 && draw.trim === 0;
