// Durations and keys, formatted the way the CLI formats them.
//
// Ported rather than reinvented: seeing `4350.00s` here where `lattice run` says
// `1:12:30` would be a visible inconsistency between two views of one product, for
// no reason at all. The tests are a table taken from the Rust behaviour.

/** Under a minute reads as seconds; past that, clock time. */
export const fmtSecs = (ms: number): string => {
	const seconds = ms / 1000;
	if (seconds < 60) return `${seconds.toFixed(2)}s`;

	const whole = Math.floor(seconds);
	const hours = Math.floor(whole / 3600);
	const minutes = Math.floor((whole % 3600) / 60);
	const rest = whole % 60;
	const pad = (value: number) => String(value).padStart(2, '0');

	return hours > 0 ? `${hours}:${pad(minutes)}:${pad(rest)}` : `${minutes}:${pad(rest)}`;
};

/** The leading chunk of a cache key, which is all a person needs to compare two. */
export const shortKey = (key: string): string => key.slice(0, 8);

/** `4 tasks, 1 cached, 0 failed, 0.39s` — the CLI's summary line, verbatim. */
export const runSummary = (result: {
	total: number;
	cached: number;
	failed: number;
	elapsedMs: number;
}): string => {
	const noun = result.total === 1 ? 'task' : 'tasks';
	return `${result.total} ${noun}, ${result.cached} cached, ${result.failed} failed, ${fmtSecs(result.elapsedMs)}`;
};

/** A run where everything came back from cache is worth saying out loud. */
export const isFullCache = (result: { total: number; cached: number; failed: number }): boolean =>
	result.total > 0 && result.cached === result.total && result.failed === 0;

/** Keep a long path readable in a fixed-width rail without losing the tail. */
export const shortenPath = (path: string, keep = 3): string => {
	const parts = path.split(/[\\/]/).filter(Boolean);
	if (parts.length <= keep) return path;
	return `…/${parts.slice(-keep).join('/')}`;
};
