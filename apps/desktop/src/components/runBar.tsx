import { useRunView } from '../hooks/useRunStore.ts';
import { isFullCache, runSummary } from '../lib/format.ts';
import { CACHE_MODES, type CacheMode } from '../lib/runOptions.ts';
import Spinner from './spinner.tsx';

export interface RunBarState {
	selected: string[];
	mode: CacheMode;
	filter: string;
	concurrency: string;
	keepGoing: boolean;
	sequentially: boolean;
}

interface RunBarProps {
	tasks: string[];
	state: RunBarState;
	onChange: (next: Partial<RunBarState>) => void;
	onRun: () => void;
	onStop: () => void;
}

const RunBar = ({ tasks, state, onChange, onRun, onStop }: RunBarProps) => {
	const run = useRunView();
	const inFlight = run.phase === 'running' || run.phase === 'stopping';

	const toggleTask = (task: string, additive: boolean) => {
		if (!additive) {
			onChange({ selected: [task] });
			return;
		}
		// Stacked roots are a real thing the engine supports, so cmd-click builds one.
		const next = state.selected.includes(task)
			? state.selected.filter((candidate) => candidate !== task)
			: [...state.selected, task];
		onChange({ selected: next.length > 0 ? next : [task] });
	};

	const hint = CACHE_MODES.find((option) => option.mode === state.mode)?.hint ?? '';

	return (
		<div className="run-bar">
			<div className="run-bar__row">
				<div className="command-tabs" role="group" aria-label="Tasks to run">
					{tasks.map((task) => (
						<button
							key={task}
							type="button"
							className={`command-tab${state.selected.includes(task) ? ' command-tab--active' : ''}`}
							onClick={(event) => toggleTask(task, event.metaKey || event.ctrlKey)}
							disabled={inFlight}
							aria-pressed={state.selected.includes(task)}
						>
							{task}
						</button>
					))}
				</div>

				{inFlight ? (
					<button
						type="button"
						className="btn btn-outline-secondary btn-sm d-inline-flex align-items-center gap-2"
						onClick={onStop}
						disabled={run.phase === 'stopping'}
					>
						<i className="bi bi-stop-circle" aria-hidden="true" />
						{run.phase === 'stopping' ? 'Stopping…' : 'Stop'}
					</button>
				) : (
					<button
						type="button"
						className="btn btn-primary btn-sm d-inline-flex align-items-center gap-2 px-3 py-2"
						onClick={onRun}
						disabled={state.selected.length === 0}
					>
						<i className="bi bi-play-circle" aria-hidden="true" />
						Run
					</button>
				)}

				<div className="ms-auto run-bar__summary" aria-live="polite">
					{inFlight ? (
						<Spinner label={run.phase === 'stopping' ? 'stopping…' : 'running…'} />
					) : (
						run.result &&
						`${runSummary(run.result)}${isFullCache(run.result) ? ' · full power, nothing to run' : ''}`
					)}
				</div>
			</div>

			<div className="run-bar__row">
				<div className="command-tabs" role="group" aria-label="Cache mode">
					{CACHE_MODES.map((option) => (
						<button
							key={option.mode}
							type="button"
							className={`command-tab${state.mode === option.mode ? ' command-tab--active' : ''}`}
							onClick={() => onChange({ mode: option.mode })}
							disabled={inFlight}
							title={option.hint}
							aria-pressed={state.mode === option.mode}
						>
							{option.label}
						</button>
					))}
				</div>
				<span className="command-tab__hint">{hint}</span>

				<div className="input-group input-group-sm" style={{ maxWidth: '15rem' }}>
					<span className="input-group-text">
						<i className="bi bi-funnel" aria-hidden="true" />
					</span>
					<input
						type="text"
						className="form-control"
						placeholder="Filter by workspace name"
						aria-label="Filter workspaces by name"
						title="Runs in the workspaces whose name contains this, plus what they depend on."
						value={state.filter}
						onChange={(event) => onChange({ filter: event.target.value })}
						disabled={inFlight}
					/>
				</div>

				<select
					className="form-select form-select-sm"
					style={{ maxWidth: '11rem' }}
					aria-label="Concurrency"
					value={state.concurrency}
					onChange={(event) => onChange({ concurrency: event.target.value })}
					disabled={inFlight}
				>
					<option value="">One per CPU</option>
					{[1, 2, 4, 8, 16].map((value) => (
						<option key={value} value={String(value)}>
							{value} at once
						</option>
					))}
				</select>

				<div className="form-check form-switch mb-0">
					<input
						className="form-check-input"
						type="checkbox"
						id="keep-going"
						checked={state.keepGoing}
						onChange={(event) => onChange({ keepGoing: event.target.checked })}
						disabled={inFlight}
					/>
					<label className="form-check-label small" htmlFor="keep-going">
						Keep going after a failure
					</label>
				</div>

				<div className="form-check form-switch mb-0">
					<input
						className="form-check-input"
						type="checkbox"
						id="sequentially"
						checked={state.sequentially}
						onChange={(event) => onChange({ sequentially: event.target.checked })}
						disabled={inFlight}
					/>
					<label className="form-check-label small" htmlFor="sequentially">
						Finish each task before starting the next
					</label>
				</div>
			</div>
		</div>
	);
};

export default RunBar;
