// The run the /desktop hero plays.
//
// The hero renders the desktop app's own components, so this file's job is only to be
// the backend they would otherwise talk to: a fixture project and a scripted sequence
// of real `RunMessage`s fed to the app's real store. Every state the rows move through
// is the app's state machine deciding, not this file describing.
//
// Paths reach into apps/desktop on purpose. Copying these components here is what this
// is meant to avoid — a status word changed in the app should change on the site.

import { runStore } from '../../../desktop/src/lib/runStore.ts';
import type {
	GraphDump,
	RunMessage,
	RunResult,
	WorkspaceView,
} from '../../../desktop/src/lib/types.ts';

const declared = { kind: 'declaration' } as const;

const engine = (name: string, version: string) => ({
	name,
	version,
	versionCmd: null,
	installCmd: null,
	bin: null,
	wellKnown: true,
});

export const workspaces: WorkspaceView[] = [
	{
		name: 'api',
		path: 'services/api',
		auto: true,
		dependsOn: [],
		driver: { tool: 'cargo', role: 'build', language: 'rust', via: declared },
		engines: [engine('rust', '1.84.0')],
		tasks: [
			{ task: 'build', command: 'cargo build --release', persistent: false, cacheable: true },
			{ task: 'test', command: 'cargo test --all-features', persistent: false, cacheable: true },
		],
	},
	{
		name: 'web',
		path: 'apps/web',
		auto: true,
		dependsOn: ['api'],
		driver: { tool: 'npm', role: 'build', language: 'node', via: declared },
		engines: [engine('node', '24.4.0')],
		tasks: [
			{ task: 'build', command: 'npm run build', persistent: false, cacheable: true },
			{ task: 'lint', command: 'npm run lint', persistent: false, cacheable: true },
		],
	},
	{
		name: 'worker',
		path: 'services/worker',
		auto: true,
		dependsOn: ['api'],
		driver: { tool: 'pytest', role: 'test', language: 'python', via: declared },
		engines: [engine('python', '3.13.1')],
		tasks: [{ task: 'test', command: 'pytest -q', persistent: false, cacheable: true }],
	},
];

// Topological order, as the backend emits it, so a layered drawing can read the list
// straight through.
const node = (id: string, command: string) => {
	const [workspace, task] = id.split(':');
	return { id, workspace, task, command, persistent: false, pulledIn: false };
};

export const graph: GraphDump = {
	nodes: [
		node('api:build', 'cargo build --release'),
		node('api:test', 'cargo test --all-features'),
		node('web:build', 'npm run build'),
		node('worker:test', 'pytest -q'),
		node('web:lint', 'npm run lint'),
	],
	edges: [
		{ from: 'api:build', to: 'api:test', crossWorkspace: false },
		{ from: 'api:build', to: 'web:build', crossWorkspace: true },
		{ from: 'api:build', to: 'worker:test', crossWorkspace: true },
		{ from: 'web:build', to: 'web:lint', crossWorkspace: false },
	],
};

interface Beat {
	at: number;
	message: RunMessage;
}

const line = (workspace: string, task: string, text: string): RunMessage => ({
	kind: 'outputBatch',
	lines: [{ workspace, task, line: text, stderr: false }],
});

// Two hits, two misses that name what moved, one first run: the shape of an ordinary
// second build, where most of the work is already done.
const beats: Beat[] = [
	{ at: 400, message: { kind: 'event', event: { type: 'cacheHit', workspace: 'api', task: 'build', key: 'a3f1b97c2e4088d1', savedMs: 42_800 } } },
	{ at: 900, message: { kind: 'event', event: { type: 'cacheMiss', workspace: 'api', task: 'test', miss: { kind: 'changed', components: ['inputs'] } } } },
	{ at: 1000, message: { kind: 'event', event: { type: 'started', workspace: 'api', task: 'test' } } },
	{ at: 1400, message: line('api', 'test', 'running 214 tests') },
	{ at: 2100, message: line('api', 'test', 'test result: ok. 214 passed; 0 failed') },
	{ at: 2400, message: { kind: 'event', event: { type: 'finished', workspace: 'api', task: 'test', durationMs: 1_400 } } },

	{ at: 2600, message: { kind: 'event', event: { type: 'cacheHit', workspace: 'web', task: 'lint', key: '5f0c118ab3d92e64', savedMs: 6_100 } } },
	{ at: 3000, message: { kind: 'event', event: { type: 'cacheMiss', workspace: 'web', task: 'build', miss: { kind: 'changed', components: ['toolchain', 'inputs'] } } } },
	{ at: 3100, message: { kind: 'event', event: { type: 'started', workspace: 'web', task: 'build' } } },
	{ at: 3600, message: line('web', 'build', 'vite building for production...') },
	{ at: 4600, message: line('web', 'build', 'built in 2.31s') },
	{ at: 5000, message: { kind: 'event', event: { type: 'finished', workspace: 'web', task: 'build', durationMs: 2_310 } } },

	{ at: 5200, message: { kind: 'event', event: { type: 'cacheMiss', workspace: 'worker', task: 'test', miss: { kind: 'firstRun' } } } },
	{ at: 5300, message: { kind: 'event', event: { type: 'started', workspace: 'worker', task: 'test' } } },
	{ at: 5900, message: line('worker', 'test', 'collected 96 items') },
	{ at: 6900, message: line('worker', 'test', '96 passed in 1.42s') },
	{ at: 7200, message: { kind: 'event', event: { type: 'finished', workspace: 'worker', task: 'test', durationMs: 1_420 } } },
];

const RESULT: RunResult = {
	total: 5,
	cached: 2,
	failed: 0,
	elapsedMs: 7_200,
	savedMs: 48_900,
};

/** When the last beat lands, so a caller knows when the run is over. */
export const RUN_MS = 7_600;

let timers: ReturnType<typeof setTimeout>[] = [];

export const stopDemo = (): void => {
	timers.forEach(clearTimeout);
	timers = [];
};

export const startDemo = (): void => {
	stopDemo();
	// The store is a module singleton and Astro's view transitions keep modules alive
	// across navigations, so a second visit has to start from empty.
	runStore.reset();
	runStore.begin(graph);

	for (const beat of beats) {
		timers.push(setTimeout(() => runStore.ingest(beat.message), beat.at));
	}
	timers.push(
		setTimeout(() => {
			runStore.ingest({ kind: 'summary', result: RESULT });
			runStore.settle({ status: 'completed', result: RESULT });
		}, RUN_MS),
	);
};

/** The finished state with no motion, for anyone who asked not to see any. */
export const settleDemo = (): void => {
	stopDemo();
	runStore.reset();
	runStore.begin(graph);
	for (const beat of beats) runStore.ingest(beat.message);
	runStore.ingest({ kind: 'summary', result: RESULT });
	runStore.flush();
	runStore.settle({ status: 'completed', result: RESULT });
};
