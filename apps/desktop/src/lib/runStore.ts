// Run state, held outside React.
//
// Two things make this its own store rather than component state.
//
// Per-key subscriptions: a build across forty workspaces should re-render the one
// row that changed, not the list. A row subscribes to its own task, and an output
// pane subscribes separately, so a line arriving never re-renders the row around it.
//
// Frame batching: messages arrive faster than a frame, so they accumulate and apply
// once per animation frame. Combined with the backend's own batching that is two
// levels of backpressure, which is what a compiler's output volume needs.

import { LineBuffer, type Since } from './lineBuffer.ts';
import { describeMiss } from './taskStatus.ts';
import type { TaskSnapshot, TaskState } from './taskStatus.ts';
import type { GraphDump, OutputLine, RunMessage, RunOutcome, RunResult } from './types.ts';

export type RunPhase = 'idle' | 'running' | 'stopping' | 'done';

export interface TaskView extends TaskSnapshot {
	readonly key: string;
	readonly workspace: string;
	readonly task: string;
	/** The structured cache-miss reason, as component names. */
	readonly missComponents?: readonly string[];
	readonly missMessage?: string;
	readonly hasOutput: boolean;
}

export interface RunView {
	readonly phase: RunPhase;
	readonly outcome: RunOutcome | null;
	readonly result: RunResult | null;
	readonly notes: readonly string[];
	readonly warnings: readonly string[];
	readonly graph: GraphDump | null;
}

const IDLE: RunView = {
	phase: 'idle',
	outcome: null,
	result: null,
	notes: [],
	warnings: [],
	graph: null,
};

interface Slot {
	view: TaskView;
	buffer: LineBuffer;
	/** Bumped only when lines were appended, so a row never re-renders for output. */
	outputRev: number;
}

const key = (workspace: string, task: string) => `${workspace}:${task}`;

class RunStore {
	private slots = new Map<string, Slot>();
	/** Placeholder views for tasks no run has touched, kept so they stay identical. */
	private idle = new Map<string, TaskView>();
	private run: RunView = IDLE;

	private viewsRevision = 0;
	private era = 0;
	private reconnected = false;

	private viewSubs = new Map<string, Set<() => void>>();
	private outputSubs = new Map<string, Set<() => void>>();
	private runSubs = new Set<() => void>();
	private viewsSubs = new Set<() => void>();

	private pending: RunMessage[] = [];
	private scheduled = false;
	/** Overridable so a test can drain synchronously. */
	private schedule: (flush: () => void) => void =
		typeof requestAnimationFrame === 'function'
			? (flush) => requestAnimationFrame(flush)
			: (flush) => setTimeout(flush, 0);

	// ---- reading ----

	/**
	 * The current view of a task, including one that no run has touched.
	 *
	 * The returned object has to be identical between calls until something about
	 * the task actually changes. `useSyncExternalStore` re-reads this on every
	 * render and compares with `Object.is`, so building a fresh placeholder each
	 * time reads as "changed again" forever and re-renders until React gives up.
	 */
	taskView(taskKey: string): TaskView {
		const slot = this.slots.get(taskKey);
		if (slot) return slot.view;

		const cached = this.idle.get(taskKey);
		if (cached) return cached;

		const [workspace, task] = taskKey.split(':');
		const view: TaskView = {
			key: taskKey,
			workspace: workspace ?? taskKey,
			task: task ?? '',
			state: 'idle',
			hasOutput: false,
		};
		this.idle.set(taskKey, view);
		return view;
	}

	runView(): RunView {
		return this.run;
	}

	outputRev(taskKey: string): number {
		return this.slots.get(taskKey)?.outputRev ?? 0;
	}

	linesSince(taskKey: string, mark: number): Since {
		const slot = this.slots.get(taskKey);
		if (!slot) return { lines: [], dropped: 0, held: 0, produced: 0 };
		return slot.buffer.since(mark);
	}

	/**
	 * Bumped whenever any task's view changed, so a whole-graph reader can redraw
	 * without subscribing per node.
	 *
	 * Output alone does not move it: only the first line of a task is a view change,
	 * which is what keeps a graph off the per-line path.
	 */
	get viewsRev(): number {
		return this.viewsRevision;
	}

	/**
	 * Which run the store is showing. A run that ends after this moved on cannot
	 * write over what replaced it.
	 */
	get epoch(): number {
		return this.era;
	}

	/** True while showing a run this webview did not start. */
	get adopted(): boolean {
		return this.reconnected;
	}

	// ---- subscribing ----

	subscribeView(taskKey: string, listener: () => void): () => void {
		return this.add(this.viewSubs, taskKey, listener);
	}

	subscribeOutput(taskKey: string, listener: () => void): () => void {
		return this.add(this.outputSubs, taskKey, listener);
	}

	subscribeRun(listener: () => void): () => void {
		this.runSubs.add(listener);
		return () => this.runSubs.delete(listener);
	}

	/** For a reader of every task at once, such as the graph. */
	subscribeViews(listener: () => void): () => void {
		this.viewsSubs.add(listener);
		return () => this.viewsSubs.delete(listener);
	}

	private add(map: Map<string, Set<() => void>>, taskKey: string, listener: () => void) {
		const set = map.get(taskKey) ?? new Set();
		set.add(listener);
		map.set(taskKey, set);
		return () => {
			set.delete(listener);
			if (set.size === 0) map.delete(taskKey);
		};
	}

	// ---- writing ----

	/** Reset for a new run, seeding every node in the graph as queued. */
	begin(graph: GraphDump | null): void {
		this.slots = new Map();
		this.idle = new Map();
		this.era += 1;
		this.reconnected = false;
		if (graph) {
			for (const node of graph.nodes) {
				this.slots.set(node.id, {
					view: {
						key: node.id,
						workspace: node.workspace,
						task: node.task,
						state: 'queued',
						hasOutput: false,
					},
					buffer: new LineBuffer(),
					outputRev: 0,
				});
			}
		}
		this.run = { ...IDLE, phase: 'running', graph };
		this.notifyAll();
	}

	stopping(): void {
		this.run = { ...this.run, phase: 'stopping' };
		this.notifyRun();
	}

	/**
	 * Show a run this webview did not start.
	 *
	 * A reload loses the channel but not the run: without this the store reads idle,
	 * so nothing offers a Stop button and the next Run is refused by the backend.
	 */
	adopt(): void {
		if (this.reconnected) return;
		this.era += 1;
		this.reconnected = true;
		this.run = { ...IDLE, phase: 'running' };
		this.notifyAll();
	}

	/**
	 * An adopted run is over. Its summary went to the window that started it, so the
	 * view keeps what it managed to draw and only stops claiming to be running.
	 */
	release(): void {
		if (!this.reconnected) return;
		this.reconnected = false;
		this.run = { ...this.run, phase: 'idle' };
		this.notifyRun();
	}

	/**
	 * Fill a pane from the tail the backend kept, for a run this window adopted.
	 *
	 * Ignored once the task has lines of its own, so a repeated reconnect cannot
	 * draw the same log twice.
	 */
	seed(workspace: string, task: string, lines: readonly OutputLine[]): void {
		if (lines.length === 0) return;
		const taskKey = key(workspace, task);
		if (this.slot(taskKey).buffer.produced > 0) return;
		for (const line of lines) this.append(taskKey, line.stderr, line.line);
		this.viewsRevision += 1;
		this.notify(this.viewSubs, taskKey);
		this.notify(this.outputSubs, taskKey);
		this.notifyViews();
	}

	settle(outcome: RunOutcome, epoch: number = this.era): void {
		if (epoch !== this.era) return;
		this.run = {
			...this.run,
			phase: 'done',
			outcome,
			result: outcome.status === 'nothing' ? null : outcome.result,
		};
		// Anything still queued never ran: the run ended before reaching it.
		for (const [taskKey, slot] of this.slots) {
			if (slot.view.state === 'queued' || slot.view.state === 'running') {
				this.patch(taskKey, { state: 'idle' });
			}
		}
		this.viewsRevision += 1;
		this.notifyRun();
		this.notifyViews();
	}

	reset(): void {
		this.slots = new Map();
		this.idle = new Map();
		this.era += 1;
		this.reconnected = false;
		this.run = IDLE;
		this.notifyAll();
	}

	/** Queue a message. Applied on the next frame with everything else that arrived. */
	ingest(message: RunMessage): void {
		this.pending.push(message);
		if (this.scheduled) return;
		this.scheduled = true;
		this.schedule(() => this.flush());
	}

	/** Apply everything queued now. Used by tests, and by a final drain. */
	flush(): void {
		this.scheduled = false;
		const batch = this.pending;
		this.pending = [];

		const views = new Set<string>();
		const outputs = new Set<string>();
		let runTouched = false;

		for (const message of batch) {
			switch (message.kind) {
				case 'runStarted':
					this.begin(message.graph);
					break;
				case 'event': {
					const event = message.event;
					const taskKey = key(event.workspace, event.task);
					switch (event.type) {
						case 'started':
							this.patch(taskKey, { state: 'running' });
							break;
						case 'cacheHit':
							this.patch(taskKey, { state: 'cached', cacheKey: event.key });
							break;
						case 'cacheMiss':
							this.patch(taskKey, {
								missComponents:
									event.miss.kind === 'changed' ? event.miss.components : undefined,
								missMessage: describeMiss(event.miss),
							});
							break;
						case 'finished':
							this.patch(taskKey, { state: 'done', durationMs: event.durationMs });
							break;
						case 'failed':
							this.patch(taskKey, {
								state: 'failed',
								exitCode: event.code,
								durationMs: event.durationMs ?? undefined,
							});
							break;
						case 'persistentExited':
							this.patch(taskKey, {
								state: 'exited',
								exitCode: event.code,
								durationMs: event.durationMs,
							});
							break;
						case 'skipped':
							this.patch(taskKey, { state: 'skipped', reason: event.reason });
							break;
						case 'output':
							// Output only ever arrives batched; this is here for completeness.
							this.append(taskKey, event.stderr, event.line);
							outputs.add(taskKey);
							break;
					}
					views.add(taskKey);
					break;
				}
				case 'outputBatch':
					for (const line of message.lines) {
						const taskKey = key(line.workspace, line.task);
						const first = this.append(taskKey, line.stderr, line.line);
						outputs.add(taskKey);
						// The row shows whether a pane has anything in it, so the first
						// line of a task is also a view change.
						if (first) views.add(taskKey);
					}
					break;
				case 'failureOutput':
					for (const line of message.lines) {
						const taskKey = key(message.workspace, message.task);
						this.append(taskKey, line.stderr, line.line);
						outputs.add(taskKey);
						views.add(taskKey);
					}
					break;
				case 'note':
					this.run = { ...this.run, notes: [...this.run.notes, message.message] };
					runTouched = true;
					break;
				case 'warn':
					this.run = { ...this.run, warnings: [...this.run.warnings, message.message] };
					runTouched = true;
					break;
				case 'summary':
					this.run = { ...this.run, result: message.result };
					runTouched = true;
					break;
				case 'finished':
					break;
			}
		}

		for (const taskKey of views) this.notify(this.viewSubs, taskKey);
		for (const taskKey of outputs) this.notify(this.outputSubs, taskKey);
		if (views.size > 0) {
			this.viewsRevision += 1;
			this.notifyViews();
		}
		if (runTouched) this.notifyRun();
	}

	/** Replace the scheduler. Tests apply synchronously; nothing else calls this. */
	setScheduler(schedule: (flush: () => void) => void): void {
		this.schedule = schedule;
	}

	// ---- internals ----

	private slot(taskKey: string): Slot {
		let slot = this.slots.get(taskKey);
		if (!slot) {
			const [workspace, task] = taskKey.split(':');
			slot = {
				view: {
					key: taskKey,
					workspace: workspace ?? taskKey,
					task: task ?? '',
					state: 'idle',
					hasOutput: false,
				},
				buffer: new LineBuffer(),
				outputRev: 0,
			};
			this.slots.set(taskKey, slot);
			this.idle.delete(taskKey);
		}
		return slot;
	}

	/** A new object each time, so useSyncExternalStore's identity check fires. */
	private patch(taskKey: string, change: Partial<TaskView> & { state?: TaskState }): void {
		const slot = this.slot(taskKey);
		slot.view = { ...slot.view, ...change };
	}

	/** True when this was the task's first line. */
	private append(taskKey: string, stderr: boolean, line: string): boolean {
		const slot = this.slot(taskKey);
		const first = slot.buffer.length === 0 && slot.buffer.dropped === 0;
		slot.buffer.push({ stderr, line });
		slot.outputRev += 1;
		if (first) slot.view = { ...slot.view, hasOutput: true };
		return first;
	}

	private notify(map: Map<string, Set<() => void>>, taskKey: string): void {
		const set = map.get(taskKey);
		if (!set) return;
		for (const listener of set) listener();
	}

	private notifyRun(): void {
		for (const listener of this.runSubs) listener();
	}

	private notifyViews(): void {
		for (const listener of this.viewsSubs) listener();
	}

	private notifyAll(): void {
		this.viewsRevision += 1;
		for (const [taskKey] of this.viewSubs) this.notify(this.viewSubs, taskKey);
		for (const [taskKey] of this.outputSubs) this.notify(this.outputSubs, taskKey);
		this.notifyViews();
		this.notifyRun();
	}
}

export const runStore = new RunStore();
export { RunStore };
