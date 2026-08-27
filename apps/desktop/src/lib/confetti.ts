// FULL POWER, escalating.
//
// A run where every task came back from cache is the one the CLI celebrates with a
// banner. Here it gets confetti, and every consecutive full-power run gets more than
// the last — the curve is exponential, so this stops being tasteful around the fourth
// hit and stops being defensible around the sixth. That is the intent.
//
// It does not escalate forever, because a window that has stopped painting is not
// funny. The frames after each burst are measured, and the first burst that drags the
// window under about 30fps sets a ceiling one step below itself. From then on the
// confetti holds at the largest amount this machine drew comfortably. The ceiling is
// discovered rather than guessed because the honest number depends on the machine, the
// window size and the webview, none of which are knowable from here.
//
// A run that is not full power puts the streak back to zero.

export interface Shot {
	particleCount: number;
	angle: number;
	spread: number;
	startVelocity: number;
	decay: number;
	scalar: number;
	ticks: number;
	origin: { x: number; y: number };
}

/** Particles in a first, still-reasonable burst. */
export const FIRST_BURST = 90;

/** How much bigger each consecutive full-power run gets. */
export const GROWTH = 1.85;

/** A frame this long is the window dropping under roughly 30fps. */
export const LAG_FRAME_MS = 34;

/** How long after a burst to watch frames for. */
export const WATCH_MS = 1500;

// Where the particles come from, in the order they are brought in. Bottom centre
// alone, then the bottom corners, then the top corners raining down, then the sides
// firing inward — by which point there is nowhere on the screen left to look.
const LAUNCHERS: readonly { origin: { x: number; y: number }; angle: number }[] = [
	{ origin: { x: 0.5, y: 1.05 }, angle: 90 },
	{ origin: { x: 0, y: 0.9 }, angle: 55 },
	{ origin: { x: 1, y: 0.9 }, angle: 125 },
	{ origin: { x: 0, y: -0.05 }, angle: 300 },
	{ origin: { x: 1, y: -0.05 }, angle: 240 },
	{ origin: { x: -0.05, y: 0.45 }, angle: 0 },
	{ origin: { x: 1.05, y: 0.45 }, angle: 180 },
];

/** What this streak would spend with nothing holding it back. */
export const rawCount = (streak: number): number =>
	Math.round(FIRST_BURST * GROWTH ** Math.max(0, streak - 1));

/** What it actually gets to spend, once a ceiling has been found. */
export const budget = (streak: number, ceiling: number | null): number =>
	ceiling === null ? rawCount(streak) : Math.min(rawCount(streak), ceiling);

/** How many launchers this streak has earned. */
export const launcherCount = (streak: number): number =>
	Math.max(1, Math.min(LAUNCHERS.length, streak));

/** The shots to fire for a streak, sharing the budget between the launchers. */
export const burstFor = (streak: number, ceiling: number | null): Shot[] => {
	const used = launcherCount(streak);
	const per = Math.max(1, Math.round(budget(streak, ceiling) / used));
	// Everything else about a shot ramps alongside the count, so a late burst is not
	// just more paper but bigger, faster paper that stays up longer.
	const ramp = Math.min(1, (streak - 1) / 6);

	return LAUNCHERS.slice(0, used).map((launcher) => ({
		particleCount: per,
		angle: launcher.angle,
		origin: launcher.origin,
		spread: 60 + 70 * ramp,
		startVelocity: 35 + 35 * ramp,
		decay: 0.9,
		scalar: 0.9 + 0.45 * ramp,
		ticks: Math.round(200 + 250 * ramp),
	}));
};

export const totalParticles = (shots: readonly Shot[]): number =>
	shots.reduce((sum, shot) => sum + shot.particleCount, 0);

/**
 * The ceiling after a burst of `spent` particles whose worst frame took
 * `worstFrameMs`.
 *
 * Once set it never moves again: a ceiling that could be raised would be rediscovered
 * on every good run and the window would keep being driven back into the ground to
 * find it. Backing off one growth step lands on the last size that did draw smoothly.
 */
export const nextCeiling = (
	ceiling: number | null,
	spent: number,
	worstFrameMs: number,
): number | null => {
	if (ceiling !== null) return ceiling;
	if (worstFrameMs <= LAG_FRAME_MS) return null;
	return Math.max(FIRST_BURST, Math.round(spent / GROWTH));
};
