// Starting and stopping a run.
//
// Every message from the channel goes into the store, which coalesces them onto the
// next animation frame. The store is the only thing that holds run state; this hook
// only starts things and reports what came back.

import { useCallback, useState } from 'react';

import * as api from '../lib/api.ts';
import { message } from '../lib/guard.ts';
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
		// Closing or switching the project discards this run's view while the run
		// itself is still unwinding, so nothing it reports afterwards may land on top
		// of what replaced it.
		let epoch = runStore.epoch;

		try {
			// The graph is fetched separately so the list can show every task the run
			// will touch, including the ones pulled in as dependencies.
			const graph = await api.graphDump({
				tasks: settings.tasks,
				filter: settings.filter,
				sequentially: settings.sequentially,
			});
			runStore.begin(graph);
			epoch = runStore.epoch;

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
			runStore.settle(outcome, epoch);
			return outcome;
		} catch (caught) {
			setError(message(caught));
			if (runStore.epoch === epoch) runStore.reset();
			return null;
		}
	}, []);

	const stop = useCallback(async () => {
		runStore.stopping();
		await api.runStop();
	}, []);

	return { start, stop, error, dismissError: () => setError(null) };
};
