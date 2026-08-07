// What a task's state looks like.
//
// Every state pairs an icon with a word. That is deliberate: colour is never the
// only carrier here, so the list is readable to someone who cannot distinguish two
// of the hues, and legible in a screenshot with no legend.

import { fmtSecs, shortKey } from './format.ts';
import type { CacheMiss } from './types.ts';

export type TaskState =
	| 'idle'
	| 'queued'
	| 'running'
	| 'cached'
	| 'done'
	| 'failed'
	| 'skipped'
	| 'exited';

export interface TaskStatusView {
	icon: string;
	/** The word beside the icon. Never omitted. */
	label: string;
	/** Announced to assistive tech on a state change. */
	announcement: string;
}

export interface TaskSnapshot {
	state: TaskState;
	durationMs?: number;
	cacheKey?: string;
	exitCode?: number | null;
	reason?: string;
}

export const statusView = (
	snapshot: TaskSnapshot,
	label: string,
): TaskStatusView => {
	const { state } = snapshot;
	switch (state) {
		case 'queued':
			return { icon: 'bi-hourglass', label: 'queued', announcement: `${label} queued` };
		case 'running':
			// The spinner is a Bootstrap component rather than a glyph, so the icon
			// slot is empty and the component fills it.
			return { icon: '', label: 'running', announcement: `${label} running` };
		case 'cached': {
			const key = snapshot.cacheKey ? ` [${shortKey(snapshot.cacheKey)}]` : '';
			return {
				icon: 'bi-lightning-charge',
				label: `cached${key}`,
				announcement: `${label} came back from cache`,
			};
		}
		case 'done': {
			const took = snapshot.durationMs === undefined ? '' : ` (${fmtSecs(snapshot.durationMs)})`;
			return {
				icon: 'bi-check-lg',
				label: `done${took}`,
				announcement: `${label} done${took}`,
			};
		}
		case 'failed':
			return { icon: 'bi-x-lg', label: 'failed', announcement: `${label} failed` };
		case 'skipped':
			return {
				icon: 'bi-slash-circle',
				label: snapshot.reason ? `skipped — ${snapshot.reason}` : 'skipped',
				announcement: `${label} skipped`,
			};
		case 'exited': {
			// A persistent task that ended on its own. No code means a signal took it.
			const how =
				snapshot.exitCode === null || snapshot.exitCode === undefined
					? 'killed by signal'
					: `exited code ${snapshot.exitCode}`;
			return { icon: 'bi-exclamation-triangle', label: how, announcement: `${label} ${how}` };
		}
		default:
			return { icon: 'bi-dash-circle', label: 'idle', announcement: '' };
	}
};

/** A run is in flight for this task, so its controls should be unavailable. */
export const isBusy = (state: TaskState): boolean => state === 'queued' || state === 'running';

/** Failure opens its own output pane: the reason is the first thing wanted. */
export const opensOnFailure = (state: TaskState): boolean => state === 'failed' || state === 'exited';

/** The sentence the CLI prints for a miss, rebuilt from the structured reason. */
export const describeMiss = (miss: CacheMiss): string => {
	switch (miss.kind) {
		case 'firstRun':
			return 'cache miss (nothing cached for this task yet)';
		case 'entryEvicted':
			return 'cache miss (the entry for this key is no longer in the cache)';
		default:
			return `cache miss: ${miss.components.join(', ')} changed`;
	}
};
