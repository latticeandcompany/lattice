// What the Toolchains view says about an engine.

import type { EngineStatus, EngineView, ToolchainGroup, WorkspaceView } from './types.ts';

/** Where a checked engine came from, in the words the CLI uses. */
export const engineSource = (engine: EngineStatus, checked: boolean): string => {
	if (!checked) return engine.installs ? 'installs on first run' : 'not checked yet';
	if (engine.provisioned) return 'installed by Lattice';
	if (engine.version) return 'on PATH';
	return 'unchecked';
};

const specKey = (engines: EngineView[]): string =>
	engines
		.map((e) => `${e.name}=${e.version ?? ''}|${e.installCmd ?? ''}|${e.versionCmd ?? ''}`)
		.sort()
		.join(';');

/**
 * What the project pins before anything has been checked: workspaces that pin the
 * same engines share one group, the way a check reports them.
 */
export const declaredGroups = (workspaces: readonly WorkspaceView[]): ToolchainGroup[] => {
	const groups = new Map<string, ToolchainGroup>();
	for (const workspace of workspaces) {
		if (workspace.engines.length === 0) continue;
		const key = specKey(workspace.engines);
		const group = groups.get(key);
		if (group) {
			group.workspaces.push(workspace.name);
			continue;
		}
		groups.set(key, {
			workspaces: [workspace.name],
			engines: workspace.engines.map((engine) => ({
				name: engine.name,
				wanted: engine.version,
				installs: engine.installCmd !== null,
				version: null,
				location: null,
				provisioned: false,
			})),
			error: null,
		});
	}
	return [...groups.values()];
};

/** `api, web and 3 more`, for a heading that names a group of workspaces. */
export const nameList = (names: readonly string[], shown = 3): string => {
	if (names.length <= shown) {
		if (names.length <= 1) return names.join('');
		return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
	}
	return `${names.slice(0, shown).join(', ')} and ${names.length - shown} more`;
};
