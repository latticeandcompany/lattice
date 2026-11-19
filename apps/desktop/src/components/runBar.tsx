import { useState } from 'react';

import { useRunView, useTaskViewsRev } from '../hooks/useRunStore.ts';
import { isFullCache, runSummary } from '../lib/format.ts';
import { rosetteProgress } from '../lib/progress.ts';
import { CACHE_MODES, splitPicker, type CacheMode } from '../lib/runOptions.ts';
import { runStore } from '../lib/runStore.ts';
import Rosette from './rosette.tsx';

export interface RunBarState {
	selected: string[];
	mode: CacheMode;
	filter: string;
	concurrency: string;
	keepGoing: boolean;
	sequentially: boolean;
	args: string;
}

interface RunBarProps {
	tasks: string[];
	state: RunBarState;
	onChange: (next: Partial<RunBarState>) => void;
	onRun: () => void;
	onStop: () => void;
}

// The mark, woven as the run settles, and the line the CLI would print beside it.
const RunStatus = ({ inFlight }: { inFlight: boolean }) => {
	useTaskViewsRev();
	const run = useRunView();
	const progress = runStore.progress();
	const mark = rosetteProgress(progress, inFlight);

	let line = '';
	if (inFlight) {
		line = `${progress.settled}/${progress.total}`;
		if (progress.cached > 0) line += ` · ${progress.cached} cached`;
		if (progress.failed > 0) line += ` · ${progress.failed} failed`;
		if (run.phase === 'stopping') line += ' · stopping';
	} else if (run.result) {
		line = runSummary(run.result);
		if (isFullCache(run.result)) line += ' · full power, nothing to run';
	}

	return (
		<div className="run-bar__status" aria-live="polite">
			{line && <span className="run-bar__summary">{line}</span>}
			<Rosette size="1.75rem" progress={mark} title={inFlight ? `Run progress: ${line}` : 'Lattice'} />
		</div>
	);
};

const RunBar = ({ tasks, state, onChange, onRun, onStop }: RunBarProps) => {
	const run = useRunView();
	const [options, setOptions] = useState(false);
	const inFlight = run.phase === 'running' || run.phase === 'stopping';
	const { direct, more } = splitPicker(tasks, state.selected);

	const toggleTask = (task: string, additive: boolean) => {
		if (!additive) {
			onChange({ selected: [task] });
			return;
		}
		// Stacked roots are something the engine supports, so cmd-click builds one.
		const next = state.selected.includes(task)
			? state.selected.filter((candidate) => candidate !== task)
			: [...state.selected, task];
		onChange({ selected: next.length > 0 ? next : [task] });
	};

	const hint = CACHE_MODES.find((option) => option.mode === state.mode)?.hint ?? '';

	return (
		<div className="run-bar">
			<div className="run-bar__row">
				<div className="picker" role="group" aria-label="Tasks to run">
					{direct.map((task) => (
						<button
							key={task}
							type="button"
							className="picker__option"
							onClick={(event) => toggleTask(task, event.metaKey || event.ctrlKey)}
							disabled={inFlight}
							aria-pressed={state.selected.includes(task)}
							title="Cmd-click to run several at once"
						>
							{task}
						</button>
					))}
					{more.length > 0 && (
						<select
							className="form-select form-select-sm picker__more"
							aria-label="Another task"
							value=""
							onChange={(event) => event.target.value && onChange({ selected: [event.target.value] })}
							disabled={inFlight}
						>
							<option value="">{more.length} more…</option>
							{more.map((task) => (
								<option key={task} value={task}>
									{task}
								</option>
							))}
						</select>
					)}
				</div>

				{inFlight ? (
					<button
						type="button"
						className="btn btn-outline-secondary btn-sm d-inline-flex align-items-center gap-2"
						onClick={onStop}
						disabled={run.phase === 'stopping'}
					>
						<i className="bi bi-stop-fill" aria-hidden="true" />
						{run.phase === 'stopping' ? 'Stopping…' : 'Stop'}
					</button>
				) : (
					<button
						type="button"
						className="btn btn-primary btn-sm d-inline-flex align-items-center gap-2"
						onClick={onRun}
						disabled={state.selected.length === 0}
					>
						<i className="bi bi-play-fill" aria-hidden="true" />
						Run {state.selected.join(' ')}
					</button>
				)}

				<button
					type="button"
					className="disclosure"
					onClick={() => setOptions((value) => !value)}
					aria-expanded={options}
					aria-controls="run-options"
				>
					<i className="bi bi-chevron-right" aria-hidden="true" />
					Options
				</button>

				<RunStatus inFlight={inFlight} />
			</div>

			{options && (
				<div className="run-options" id="run-options">
					<div className="picker" role="group" aria-label="Cache">
						{CACHE_MODES.map((option) => (
							<button
								key={option.mode}
								type="button"
								className="picker__option"
								onClick={() => onChange({ mode: option.mode })}
								disabled={inFlight}
								aria-pressed={state.mode === option.mode}
							>
								{option.label}
							</button>
						))}
					</div>

					<label className="run-options__field">
						<span>Only in</span>
						<input
							type="text"
							className="form-control form-control-sm"
							placeholder="workspace name contains…"
							value={state.filter}
							onChange={(event) => onChange({ filter: event.target.value })}
							disabled={inFlight}
						/>
					</label>

					<label className="run-options__field">
						<span>Arguments</span>
						<input
							type="text"
							className="form-control form-control-sm font-monospace"
							placeholder="--nocapture"
							value={state.args}
							onChange={(event) => onChange({ args: event.target.value })}
							disabled={inFlight}
							title="Passed to the tasks you picked, as after -- on the command line"
						/>
					</label>

					<select
						className="form-select form-select-sm w-auto"
						aria-label="How many tasks run at once"
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
						<label className="form-check-label" htmlFor="keep-going">
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
						<label className="form-check-label" htmlFor="sequentially">
							Finish each task before the next
						</label>
					</div>

					<p className="run-options__hint">{hint}</p>
				</div>
			)}

			{(run.notes.length > 0 || run.warnings.length > 0) && (
				<ul className="run-bar__notes selectable">
					{run.warnings.map((warning, index) => (
						<li className="run-bar__warn" key={`w${index}`}>
							<i className="bi bi-exclamation-triangle me-1" aria-hidden="true" />
							{warning}
						</li>
					))}
					{run.notes.map((note, index) => (
						<li key={`n${index}`}>{note}</li>
					))}
				</ul>
			)}
		</div>
	);
};

export default RunBar;
