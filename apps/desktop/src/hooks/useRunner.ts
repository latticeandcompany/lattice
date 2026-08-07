// Starting and stopping a run.
//
// Every message from the channel goes into the store, which coalesces them onto the
// next animation frame. The store is the only thing that holds run state; this hook
// only starts things and reports what came back.

import { useCallback, useState } from 'react';

import * as api from '../lib/api.ts';
import { runStore } from '../lib/runStore.ts';
import { cacheFlags, type CacheMode } from '../lib/runOptions.ts';

export interface RunSettings {
	tasks: string[];
	filter?: string;
	sequentially: boolean;
	concurrency?: number;
	keepGoing: boolean;
	mode: CacheMode;
}

export const useRunner = () => {
	const [error, setError] = useState<string | null>(null);

	const start = useCallback(async (settings: RunSettings) => {
		setError(null);
		const flags = cacheFlags(settings.mode);

		// Seed the store before the first message arrives, so the list switches to
		// queued immediately rather than after the graph comes back.
		runStore.begin(null);

		try {
			// The graph is fetched separately so the list can show every task the run
			// will touch, including the ones pulled in as dependencies.
			const graph = await api.graphDump({
				tasks: settings.tasks,
				filter: settings.filter,
				sequentially: settings.sequentially,
			});
			runStore.begin(graph);

			const outcome = await api.runStart(
				{
					tasks: settings.tasks,
					filter: settings.filter,
					sequentially: settings.sequentially,
					concurrency: settings.concurrency,
					keepGoing: settings.keepGoing,
					noCache: flags.noCache,
					force: flags.force,
				},
				(message) => runStore.ingest(message),
			);

			// Anything still queued arrived after the run ended, so drain before settling.
			runStore.flush();
			runStore.settle(outcome);
			return outcome;
		} catch (caught) {
			const message = caught instanceof Error ? caught.message : String(caught);
			setError(message);
			runStore.reset();
			return null;
		}
	}, []);

	const stop = useCallback(async () => {
		runStore.stopping();
		await api.runStop();
	}, []);

	return { start, stop, error, dismissError: () => setError(null) };
};
