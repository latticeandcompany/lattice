// The rosette as a progress mark.
//
// The mark is eight woven ellipses. During a run each settled eighth of the work
// draws one of them in the accent, so the logo is literally woven as the run goes.
// Failure is not a stroke count: it changes what colour the drawn strokes take.

export const STROKES = 8;

export interface RosetteProgress {
	/** Strokes drawn in the run colour, 0 to 8. */
	drawn: number;
	tone: 'idle' | 'live' | 'failed';
}

export const rosetteProgress = (
	progress: { total: number; settled: number; failed: number },
	running: boolean,
): RosetteProgress => {
	if (!running && progress.total === 0) return { drawn: STROKES, tone: 'idle' };
	const share = progress.total === 0 ? 0 : progress.settled / progress.total;
	// A run that has started but settled nothing still shows it is under way.
	const drawn = running ? Math.max(1, Math.floor(share * STROKES)) : Math.round(share * STROKES);
	const tone = progress.failed > 0 ? 'failed' : running ? 'live' : 'idle';
	return { drawn: Math.min(STROKES, drawn), tone };
};
