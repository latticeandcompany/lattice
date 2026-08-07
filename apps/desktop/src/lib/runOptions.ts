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

/** The hints say what actually differs, in the CLI's own terms. */
export const CACHE_MODES: CacheModeOption[] = [
	{ mode: 'normal', label: 'Normal', hint: 'Read and write the cache.' },
	{ mode: 'force', label: 'Force', hint: 'Re-run, then refresh the cache entry.' },
	{ mode: 'ignore', label: 'Ignore cache', hint: 'Neither read nor write the cache.' },
];

export const cacheModeHint = (mode: CacheMode): string =>
	CACHE_MODES.find((option) => option.mode === mode)?.hint ?? '';
