import assert from 'node:assert/strict';
import { test } from 'node:test';
import { RunStore } from '../src/lib/runStore.ts';
import type { GraphDump, RunMessage } from '../src/lib/types.ts';

const graph: GraphDump = {
	nodes: [
		{ id: 'api:build', workspace: 'api', task: 'build', command: 'make', persistent: false, pulledIn: false },
		{ id: 'web:build', workspace: 'web', task: 'build', command: 'npm run build', persistent: false, pulledIn: false },
	],
	edges: [{ from: 'api:build', to: 'web:build', crossWorkspace: true }],
};

const store = () => {
	const s = new RunStore();
	// Drain synchronously; the real one waits for an animation frame.
	s.setScheduler((flush) => flush());
	return s;
};

const event = (e: RunMessage['kind'] extends never ? never : any): RunMessage => e;

test('a graph seeds every node as queued', () => {
	const s = store();
	s.begin(graph);
	assert.equal(s.taskView('api:build').state, 'queued');
	assert.equal(s.taskView('web:build').state, 'queued');
	assert.equal(s.runView().phase, 'running');
});

test('a task walks from started to done', () => {
	const s = store();
	s.begin(graph);
	s.ingest(event({ kind: 'event', event: { type: 'started', workspace: 'api', task: 'build' } }));
	assert.equal(s.taskView('api:build').state, 'running');

	s.ingest(
		event({
			kind: 'event',
			event: { type: 'finished', workspace: 'api', task: 'build', durationMs: 210 },
		}),
	);
	assert.equal(s.taskView('api:build').state, 'done');
	assert.equal(s.taskView('api:build').durationMs, 210);
});

test('a cache hit is cached, not done', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({
			kind: 'event',
			event: { type: 'cacheHit', workspace: 'api', task: 'build', key: 'abcdef1234' },
		}),
	);
	const view = s.taskView('api:build');
	assert.equal(view.state, 'cached');
	assert.equal(view.cacheKey, 'abcdef1234');
});

test('a miss keeps the components rather than only the sentence', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({
			kind: 'event',
			event: {
				type: 'cacheMiss',
				workspace: 'api',
				task: 'build',
				miss: { kind: 'changed', components: ['inputs', 'command'] },
			},
		}),
	);
	const view = s.taskView('api:build');
	assert.deepEqual(view.missComponents, ['inputs', 'command']);
	assert.equal(view.missMessage, 'cache miss: inputs, command changed');
});

test('a skipped task carries its reason', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({
			kind: 'event',
			event: { type: 'skipped', workspace: 'web', task: 'build', reason: 'prerequisite failed' },
		}),
	);
	assert.equal(s.taskView('web:build').reason, 'prerequisite failed');
});

test('a persistent task that exited cleanly is not a failure', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({
			kind: 'event',
			event: {
				type: 'persistentExited',
				workspace: 'web',
				task: 'dev',
				code: 0,
				durationMs: 900,
			},
		}),
	);
	const view = s.taskView('web:dev');
	assert.equal(view.state, 'exited');
	assert.equal(view.exitCode, 0);
});

test('output lands in the buffer and marks the task as having some', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({
			kind: 'outputBatch',
			lines: [
				{ workspace: 'api', task: 'build', stderr: false, line: 'one' },
				{ workspace: 'api', task: 'build', stderr: true, line: 'two' },
			],
		}),
	);
	assert.ok(s.taskView('api:build').hasOutput);
	const since = s.linesSince('api:build', 0);
	assert.deepEqual(
		since.lines.map((l) => l.line),
		['one', 'two'],
	);
});

test('only the task that changed notifies its subscribers', () => {
	const s = store();
	s.begin(graph);
	let api = 0;
	let web = 0;
	s.subscribeView('api:build', () => (api += 1));
	s.subscribeView('web:build', () => (web += 1));

	s.ingest(event({ kind: 'event', event: { type: 'started', workspace: 'api', task: 'build' } }));
	assert.equal(api, 1);
	assert.equal(web, 0, 'a forty-workspace build must not re-render the whole list');
});

test('output notifies the pane but not the row', () => {
	const s = store();
	s.begin(graph);
	// The first line is a view change too, because the row grows a pane toggle.
	s.ingest(
		event({
			kind: 'outputBatch',
			lines: [{ workspace: 'api', task: 'build', stderr: false, line: 'first' }],
		}),
	);

	let views = 0;
	let outputs = 0;
	s.subscribeView('api:build', () => (views += 1));
	s.subscribeOutput('api:build', () => (outputs += 1));

	s.ingest(
		event({
			kind: 'outputBatch',
			lines: [{ workspace: 'api', task: 'build', stderr: false, line: 'second' }],
		}),
	);
	assert.equal(outputs, 1);
	assert.equal(views, 0, 'a line arriving must not re-render the row around it');
});

test('everything queued is coalesced into one notification per key', () => {
	const s = new RunStore();
	let flush = () => {};
	s.setScheduler((f) => {
		flush = f;
	});
	s.begin(graph);

	let notifications = 0;
	s.subscribeView('api:build', () => (notifications += 1));

	s.ingest(event({ kind: 'event', event: { type: 'started', workspace: 'api', task: 'build' } }));
	s.ingest(
		event({
			kind: 'event',
			event: { type: 'finished', workspace: 'api', task: 'build', durationMs: 10 },
		}),
	);
	assert.equal(notifications, 0, 'nothing applies before the frame');

	flush();
	assert.equal(notifications, 1, 'two messages, one re-render');
	assert.equal(s.taskView('api:build').state, 'done');
});

test('a run that ends leaves nothing claiming to be running', () => {
	const s = store();
	s.begin(graph);
	s.ingest(event({ kind: 'event', event: { type: 'started', workspace: 'api', task: 'build' } }));
	s.settle({ status: 'interrupted', result: { total: 2, cached: 0, failed: 0, elapsedMs: 50 } });

	assert.equal(s.runView().phase, 'done');
	assert.equal(s.taskView('api:build').state, 'idle');
	assert.equal(s.taskView('web:build').state, 'idle');
});

test('a summary is recorded for the run, not for a task', () => {
	const s = store();
	s.begin(graph);
	s.ingest(
		event({ kind: 'summary', result: { total: 2, cached: 1, failed: 0, elapsedMs: 390 } }),
	);
	assert.deepEqual(s.runView().result, { total: 2, cached: 1, failed: 0, elapsedMs: 390 });
});

test('notes and warnings accumulate separately', () => {
	const s = store();
	s.begin(graph);
	s.ingest(event({ kind: 'note', workspace: null, task: null, message: 'provisioning' }));
	s.ingest(event({ kind: 'warn', workspace: null, task: null, message: 'cache lookup failed' }));
	assert.deepEqual(s.runView().notes, ['provisioning']);
	assert.deepEqual(s.runView().warnings, ['cache lookup failed']);
});

test('a task nobody mentioned reads as idle rather than undefined', () => {
	const s = store();
	const view = s.taskView('ghost:build');
	assert.equal(view.state, 'idle');
	assert.equal(view.workspace, 'ghost');
	assert.equal(view.task, 'build');
});
