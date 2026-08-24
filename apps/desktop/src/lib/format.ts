// Durations and keys, formatted the way the CLI formats them.
//
// Ported rather than reinvented: seeing `4350.00s` here where `lattice run` says
// `1:12:30` would be a visible inconsistency between two views of one product, for
// no reason at all. The tests are a table taken from the Rust behaviour.

/** Under a minute reads as seconds. Past that, clock time. */
export const fmtSecs = (ms: number): string => {
	// The half-second is what picks the branch, so 59.6s reads as 1:00 rather than
	// as 60.00s, and 59:59.6 reads as 1:00:00 rather than as 59:60.
	const total = Math.floor((ms + 500) / 1000);
	if (total < 60) return `${(ms / 1000).toFixed(2)}s`;

	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const rest = total % 60;
	const pad = (value: number) => String(value).padStart(2, '0');

	return hours > 0 ? `${hours}:${pad(minutes)}:${pad(rest)}` : `${minutes}:${pad(rest)}`;
};

/**
 * A stretch of saved time: `4.27s`, `4m 07s`, `14h 22m`.
 *
 * Under a minute this is `fmtSecs`, so it agrees with the elapsed time beside it.
 * Past that it says minutes and hours instead of clock time — `14:22:00 saved`
 * reads like a timestamp.
 */
export const fmtSpan = (ms: number): string => {
	const total = Math.floor((ms + 500) / 1000);
	if (total < 60) return fmtSecs(ms);

	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const rest = total % 60;
	const pad = (value: number) => String(value).padStart(2, '0');

	return hours > 0 ? `${hours}h ${pad(minutes)}m` : `${minutes}m ${pad(rest)}s`;
};

/** The leading chunk of a cache key, which is all a person needs to compare two. */
export const shortKey = (key: string): string => key.slice(0, 8);

/**
 * `4 tasks, 1 cached, 0 failed, 0.39s` — the CLI's summary line, verbatim. A run
 * whose hits saved measurable time carries the same tail the CLI adds:
 * `4 tasks, 3 cached, 0 failed, 0.39s, 2m 51s saved`.
 *
 * The CLI does not singularize `tasks`, so neither does this. A one-task run reads
 * `1 tasks` in both places, which is wrong in both places and has to be fixed in
 * `lattice-output` first.
 */
export const runSummary = (result: {
	total: number;
	cached: number;
	failed: number;
	elapsedMs: number;
	savedMs: number;
}): string => {
	const saved = result.savedMs > 0 ? `, ${fmtSpan(result.savedMs)} saved` : '';
	return `${result.total} tasks, ${result.cached} cached, ${result.failed} failed, ${fmtSecs(result.elapsedMs)}${saved}`;
};

/**
 * A run where everything came back from cache is worth saying out loud. The CLI
 * calls it full power; it deliberately does not call it a full cache, which is
 * how a disk running out of room is described.
 */
export const isFullCache = (result: { total: number; cached: number; failed: number }): boolean =>
	result.total > 0 && result.cached === result.total && result.failed === 0;

/** Keep a long path readable in a fixed-width rail without losing the tail. */
export const shortenPath = (path: string, keep = 3): string => {
	const parts = path.split(/[\\/]/).filter(Boolean);
	if (parts.length <= keep) return path;
	return `…/${parts.slice(-keep).join('/')}`;
};
