import { useEffect, useState } from 'react';

import { useTaskOutputRev, useTaskView, useTaskViewsRev } from '../hooks/useRunStore.ts';
import { fmtSecs } from '../lib/format.ts';
import { runStore } from '../lib/runStore.ts';
import { isBusy, opensOnFailure, statusView } from '../lib/taskStatus.ts';
import type { WorkspaceTaskView } from '../lib/types.ts';
import IconButton from './iconButton.tsx';
import OutputPane from './outputPane.tsx';

const TAIL_LINES = 3;

interface TaskRowProps {
	workspace: string;
	task: WorkspaceTaskView;
	/** Absent where there is nothing to run, as on the website's recording. */
	onRun?: (mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

// The last lines a running task printed. Its own component so only a running row
// subscribes to output, and a finished list re-renders for nothing.
const TaskTail = ({ taskKey }: { taskKey: string }) => {
	useTaskOutputRev(taskKey);
	const lines = runStore.tail(taskKey, TAIL_LINES);
	if (lines.length === 0) return null;
	return (
		<div className="task-tail selectable" aria-hidden="true">
			{lines.map((line, index) => (
				<span key={index}>{line.line || ' '}</span>
			))}
		</div>
	);
};

// Which of its prerequisites a queued task is still waiting on, the way the CLI's
// live display says it.
const WaitingOn = ({ waitsOn }: { waitsOn: readonly string[] }) => {
	useTaskViewsRev();
	const pending = waitsOn.filter((dep) => runStore.unsettled(dep));
	return <span>{pending.length > 0 ? `waiting on ${pending.join(', ')}` : 'queued'}</span>;
};

const TaskRow = ({ workspace, task, onRun, runInFlight }: TaskRowProps) => {
	const taskKey = `${workspace}:${task.task}`;
	const view = useTaskView(taskKey);
	const [open, setOpen] = useState(false);
	const status = statusView(view, taskKey);

	// A failure opens its own pane: the reason is the first thing anyone wants.
	useEffect(() => {
		if (opensOnFailure(view.state)) setOpen(true);
	}, [view.state]);

	const failed = view.state === 'failed' || view.state === 'exited';
	const word =
		view.state === 'done' || view.state === 'idle' || view.state === 'failed'
			? { done: 'done', idle: 'not run', failed: 'failed' }[view.state]
			: status.label;

	return (
		<>
			<div className="task-row">
				<div className="task-row__name" title={task.task}>
					{task.task}
					{task.persistent && <span className="task-row__tag">keeps running</span>}
					{!task.cacheable && !task.persistent && <span className="task-row__tag">never cached</span>}
				</div>
				<div className="task-row__command selectable" title={`${task.command}\nfrom ${task.source}`}>
					{task.command}
				</div>
				<div className="task-row__right">
					<span className={`task-status task-status--${view.state}`}>
						{view.state === 'running' ? (
							<span className="spinner-border" role="status" aria-hidden="true" />
						) : (
							status.icon && <i className={`bi ${status.icon}`} aria-hidden="true" />
						)}
						{view.state === 'queued' && view.waitsOn ? <WaitingOn waitsOn={view.waitsOn} /> : <span>{word}</span>}
						{view.durationMs !== undefined && (view.state === 'done' || failed) && (
							<span className="task-status__time">
								{view.exitCode !== undefined && view.exitCode !== null ? `code ${view.exitCode} · ` : ''}
								{fmtSecs(view.durationMs)}
							</span>
						)}
					</span>
					<span className="task-row__actions">
						{view.hasOutput && (
							<IconButton
								icon={open ? 'bi-chevron-up' : 'bi-terminal'}
								label={open ? `Hide output for ${taskKey}` : `Show output for ${taskKey}`}
								onClick={() => setOpen((value) => !value)}
								expanded={open}
							/>
						)}
						{onRun && (
							<>
								<IconButton
									icon="bi-play-fill"
									label={`Run ${taskKey}`}
									onClick={() => onRun('normal')}
									disabled={runInFlight || isBusy(view.state)}
								/>
								<IconButton
									icon="bi-arrow-clockwise"
									label={`Run ${taskKey} again, ignoring the cache`}
									onClick={() => onRun('force')}
									disabled={runInFlight || isBusy(view.state)}
								/>
							</>
						)}
					</span>
				</div>
			</div>

			{view.state === 'running' && !open && <TaskTail taskKey={taskKey} />}

			{view.missComponents && view.missComponents.length > 0 && view.state !== 'cached' && (
				<div className="task-miss">missed the cache: {view.missComponents.join(', ')} changed</div>
			)}

			{open && (
				<div className="task-log">
					<OutputPane taskKey={taskKey} failed={failed} />
				</div>
			)}
		</>
	);
};

export default TaskRow;
