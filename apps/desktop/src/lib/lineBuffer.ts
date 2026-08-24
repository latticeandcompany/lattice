// A bounded buffer of output lines.
//
// Bounded because a compiler will happily produce more lines than a window can hold,
// and the interesting ones are at the end. What is dropped is counted, so a pane can
// say so rather than silently showing a partial log.

import type { OutputLine } from './types.ts';

export const DEFAULT_CAPACITY = 2000;

export interface Since {
	lines: readonly OutputLine[];
	dropped: number;
	held: number;
	produced: number;
}

export class LineBuffer {
	private lines: OutputLine[] = [];
	private droppedCount = 0;
	// An explicit field, not a parameter property: Node runs these tests by stripping
	// types rather than compiling them, and it cannot strip a parameter property.
	private readonly capacity: number;

	constructor(capacity: number = DEFAULT_CAPACITY) {
		this.capacity = capacity;
	}

	push(line: OutputLine): void {
		this.lines.push(line);
		if (this.lines.length > this.capacity) {
			const excess = this.lines.length - this.capacity;
			this.lines.splice(0, excess);
			this.droppedCount += excess;
		}
	}

	/** Total lines ever pushed, dropped ones included. */
	get produced(): number {
		return this.droppedCount + this.lines.length;
	}

	get dropped(): number {
		return this.droppedCount;
	}

	get length(): number {
		return this.lines.length;
	}

	all(): readonly OutputLine[] {
		return this.lines;
	}

	/**
	 * The lines produced since `mark`, plus how many were dropped in the meantime.
	 *
	 * `mark` counts lines *produced*, not lines held, so a caller that has already
	 * drawn N lines stays correct across a wrap: it learns it needs to discard
	 * `dropped` from the front and append `lines` to the back. `held` is what the
	 * caller's own view should end up holding, and `produced` is its next mark.
	 */
	since(mark: number): Since {
		const firstHeld = this.droppedCount;
		if (mark <= firstHeld) {
			return {
				lines: this.lines,
				dropped: Math.max(0, mark - 0),
				held: this.lines.length,
				produced: this.produced,
			};
		}
		return {
			lines: this.lines.slice(mark - firstHeld),
			dropped: 0,
			held: this.lines.length,
			produced: this.produced,
		};
	}

	clear(): void {
		this.lines = [];
		this.droppedCount = 0;
	}
}
