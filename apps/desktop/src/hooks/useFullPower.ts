// Confetti when a run comes back entirely from cache.
//
// The escalation and the lag ceiling live in lib/confetti.ts, which is where they are
// tested. This is the part that needs a browser: it watches the run settle, loads
// canvas-confetti on the first hit rather than in the main bundle, and times the frames
// after each burst so the ceiling can be found.

import { useEffect, useRef } from 'react';

import { burstFor, nextCeiling, totalParticles, WATCH_MS } from '../lib/confetti.ts';
import { isFullCache } from '../lib/format.ts';
import type { RunOutcome } from '../lib/types.ts';
import { useReducedMotion } from './useReducedMotion.ts';
import { useRunView } from './useRunStore.ts';

/** The longest frame drawn over the next stretch of wall clock. */
const worstFrame = (windowMs: number): Promise<number> =>
	new Promise((resolve) => {
		let worst = 0;
		let last = performance.now();
		const started = last;
		const tick = (now: number) => {
			worst = Math.max(worst, now - last);
			last = now;
			if (now - started >= windowMs) resolve(worst);
			else requestAnimationFrame(tick);
		};
		requestAnimationFrame(tick);
	});

export const useFullPower = () => {
	const run = useRunView();
	const reducedMotion = useReducedMotion();
	const streak = useRef(0);
	const ceiling = useRef<number | null>(null);
	// `settle` builds a fresh outcome per run, so identity is enough to fire once.
	const fired = useRef<RunOutcome | null>(null);

	useEffect(() => {
		const outcome = run.outcome;
		if (run.phase !== 'done' || !outcome || outcome === fired.current) return;
		fired.current = outcome;

		// A run that matched no workspace neither earns a streak nor breaks one.
		if (outcome.status === 'nothing') return;

		if (!isFullCache(outcome.result)) {
			streak.current = 0;
			return;
		}

		streak.current += 1;
		if (reducedMotion) return;

		const shots = burstFor(streak.current, ceiling.current);
		void (async () => {
			const { default: confetti } = await import('canvas-confetti');
			for (const shot of shots) void confetti(shot);
			const worst = await worstFrame(WATCH_MS);
			ceiling.current = nextCeiling(ceiling.current, totalParticles(shots), worst);
		})();
	}, [run, reducedMotion]);
};
