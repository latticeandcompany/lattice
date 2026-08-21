// Reading the run store from React.
//
// Per-key subscriptions through useSyncExternalStore: a row watches its own task and
// an output pane watches its own output, so a forty-workspace build re-renders the one
// row that changed rather than the list.

import { useCallback, useSyncExternalStore } from 'react';

import { runStore, type RunView, type TaskView } from '../lib/runStore.ts';

export const useTaskView = (taskKey: string): TaskView =>
	useSyncExternalStore(
		useCallback((listener: () => void) => runStore.subscribeView(taskKey, listener), [taskKey]),
		useCallback(() => runStore.taskView(taskKey), [taskKey]),
	);

/** Only a mounted output pane subscribes, so a collapsed one costs nothing. */
export const useTaskOutputRev = (taskKey: string): number =>
	useSyncExternalStore(
		useCallback((listener: () => void) => runStore.subscribeOutput(taskKey, listener), [taskKey]),
		useCallback(() => runStore.outputRev(taskKey), [taskKey]),
	);

export const useRunView = (): RunView =>
	useSyncExternalStore(
		useCallback((listener: () => void) => runStore.subscribeRun(listener), []),
		useCallback(() => runStore.runView(), []),
	);
