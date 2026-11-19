// Reading lattice.json's `overrides`, the way the Rust side matches them.

import type { PipelineTask, WorkspaceOverride } from './types.ts';

/** A key is matched against paths when it holds a `/` or a glob character. */
export const isPathKey = (key: string): boolean => key === '.' || /[/*?[{]/.test(key);

/** The command an override writes for a task, when it writes one. */
export const overrideCommand = (value: string | PipelineTask): string | undefined =>
	typeof value === 'string' ? value : value.run;

/** Whether an override task is written as the object form, which the form edits as JSON. */
export const isDetailed = (value: string | PipelineTask): value is PipelineTask =>
	typeof value !== 'string';

/** The keys of an override that carry something, for a one-line summary. */
export const overrideFacets = (override: WorkspaceOverride): string[] => {
	const facets: string[] = [];
	if (override.name) facets.push(`named ${override.name}`);
	if (override.driver) facets.push(override.driver === 'none' ? 'no driver' : `driver ${override.driver}`);
	const engines = Object.keys(override.engines ?? {});
	if (engines.length > 0) facets.push(`pins ${engines.join(', ')}`);
	if ((override.dependsOn ?? []).length > 0) facets.push(`after ${override.dependsOn?.join(', ')}`);
	const tasks = Object.keys(override.tasks ?? {});
	if (tasks.length > 0) facets.push(`${tasks.length} ${tasks.length === 1 ? 'task' : 'tasks'}`);
	return facets;
};
