// How the three cache choices map onto the two flags the engine takes.
//
// They are offered as three exclusive modes rather than two checkboxes because the
// flags overlap: `--force` re-runs *and* refreshes the entry, `--no-cache` neither
// reads nor writes. Two independent switches would let someone ask for "write but
// don't read", which the engine has no way to express.

export type CacheMode = 'normal' | 'force' | 'ignore';

export interface CacheFlags {
	noCache: boolean;
	force: boolean;
}

/** Mirrors what `lattice run` does with --force and --no-cache. */
export const cacheFlags = (mode: CacheMode): CacheFlags => {
	switch (mode) {
		case 'force':
			return { noCache: false, force: true };
		case 'ignore':
			return { noCache: true, force: false };
		default:
			return { noCache: false, force: false };
	}
};

/** What the engine resolves those flags to: (skip lookups, skip writes). */
export const effectiveCache = (flags: CacheFlags): { skipRead: boolean; skipWrite: boolean } => ({
	skipRead: flags.noCache || flags.force,
	skipWrite: flags.noCache,
});

export interface CacheModeOption {
	mode: CacheMode;
	label: string;
	hint: string;
}

/** Each hint says what happens, then names the flag that does it. */
export const CACHE_MODES: CacheModeOption[] = [
	{
		mode: 'normal',
		label: 'Use cache',
		hint: 'Reuse a stored result when the cache key matches, and store what runs.',
	},
	{
		mode: 'force',
		label: 'Force',
		hint: 'Run every task and replace its stored result, like --force.',
	},
	{
		mode: 'ignore',
		label: 'No cache',
		hint: 'Run every task and store nothing, like --no-cache.',
	},
];

export const cacheModeHint = (mode: CacheMode): string =>
	CACHE_MODES.find((option) => option.mode === mode)?.hint ?? '';

/** What a project starts with picked: build if it has one, otherwise its first task. */
export const defaultSelection = (tasks: readonly string[]): string[] =>
	tasks.includes('build') ? ['build'] : tasks.slice(0, 1);

/**
 * The selection to actually act on.
 *
 * The picked tasks belong to whichever project was open when they were picked, and
 * the views that hold them are not remounted across a switch. Anything the project
 * now open does not define would be sent to a run as an unknown task.
 */
export const effectiveSelection = (
	selected: readonly string[],
	tasks: readonly string[],
): string[] => {
	const kept = selected.filter((task) => tasks.includes(task));
	return kept.length > 0 ? kept : defaultSelection(tasks);
};
