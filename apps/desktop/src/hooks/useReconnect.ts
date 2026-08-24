// Picking up a run this window did not start.
//
// A reload — Cmd-R, or the error boundary remounting the tree — throws away the
// channel a run reports on, but not the run: the backend still has it, still has its
// child processes, and still refuses a second one. So the window asks whether there
// is one, adopts it so a Stop button comes back, and redraws each pane from the tail
// the backend kept for exactly this.
//
// The end of an adopted run can only be learned by asking, because the channel that
// would have said so belongs to the window that is gone. That poll starts only if a
// run was actually found, so an ordinary launch costs one command and stops.

import { useEffect, useRef } from 'react';

import * as api from '../lib/api.ts';
import { runStore } from '../lib/runStore.ts';
import type { ProjectView } from '../lib/types.ts';

const POLL_MS = 1000;

export const useReconnect = (project: ProjectView | null) => {
	const open = useRef<ProjectView | null>(project);

	useEffect(() => {
		open.current = project;
	}, [project]);

	useEffect(() => {
		let cancelled = false;
		let adopted = false;
		let seeded = false;
		let timer: ReturnType<typeof setTimeout> | undefined;

		const redrawPanes = async (view: ProjectView) => {
			for (const workspace of view.workspaces) {
				for (const task of workspace.tasks) {
					if (cancelled) return;
					try {
						const lines = await api.runLog(workspace.name, task.task);
						if (!cancelled) runStore.seed(workspace.name, task.task, lines);
					} catch {
						// A pane with no log is the state it was already in.
					}
				}
			}
		};

		const check = async (first: boolean) => {
			let active: string | null = null;
			try {
				active = await api.runActive();
			} catch {
				return;
			}
			if (cancelled) return;

			if (!active) {
				// On the first look there was nothing to adopt; later it means the run
				// this window adopted has ended.
				if (!first) runStore.release();
				return;
			}

			// Closing or switching the project discards the adopted view, and it is not
			// this window's to put back.
			if (adopted && !runStore.adopted) return;

			runStore.adopt();
			adopted = true;
			// The project arrives from its own command, so the panes are seeded on
			// whichever tick first has something to seed them from.
			if (!seeded && open.current) {
				seeded = true;
				void redrawPanes(open.current);
			}
			timer = setTimeout(() => void check(false), POLL_MS);
		};

		void check(true);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	}, []);
};
