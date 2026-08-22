import { useEffect, useState } from 'react';

import { useTaskView } from '../hooks/useRunStore.ts';
import { isBusy, opensOnFailure, statusView } from '../lib/taskStatus.ts';
import type { WorkspaceTaskView } from '../lib/types.ts';
import OutputPane from './outputPane.tsx';

interface TaskRowProps {
	workspace: string;
	task: WorkspaceTaskView;
	onRun: (mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

const TaskRow = ({ workspace, task, onRun, runInFlight }: TaskRowProps) => {
	const taskKey = `${workspace}:${task.task}`;
	const view = useTaskView(taskKey);
	const [open, setOpen] = useState(false);
	const status = statusView(view, taskKey);

	// A failure opens its own pane: the reason is the first thing anyone wants.
	useEffect(() => {
		if (opensOnFailure(view.state)) setOpen(true);
	}, [view.state]);

	const busy = isBusy(view.state);
	const rowClass = [
		'task-row',
		busy ? 'task-row--running' : '',
		view.state === 'failed' || view.state === 'exited' ? 'task-row--failed' : '',
	]
		.filter(Boolean)
		.join(' ');

	return (
		<>
			<div className={rowClass}>
				<div>
					<div className="task-row__name">
						{task.task}
						{task.persistent && (
							<span className="chip ms-2" title="Declared persistent: true">
								persistent
							</span>
						)}
						{!task.cacheable && (
							<span className="chip ms-2" title="Declared cache: false">
								never cached
							</span>
						)}
					</div>
					<div className="task-row__command selectable" title={task.command}>
						{task.command}
					</div>
				</div>

				<div className={`task-status task-status--${view.state}`}>
					{view.state === 'running' ? (
						<span
							className="spinner-border spinner-border-sm"
							role="status"
							aria-hidden="true"
							style={{ width: '0.8rem', height: '0.8rem', borderWidth: '0.12em' }}
						/>
					) : (
						status.icon && <i className={`bi ${status.icon}`} aria-hidden="true" />
					)}
					<span>{status.label}</span>
				</div>

				<div className="task-row__actions">
					{view.hasOutput && (
						<button
							type="button"
							className="icon-btn"
							onClick={() => setOpen((value) => !value)}
							aria-expanded={open}
							title={open ? 'Hide output' : 'Show output'}
							aria-label={open ? `Hide output for ${taskKey}` : `Show output for ${taskKey}`}
						>
							<i className={`bi ${open ? 'bi-chevron-up' : 'bi-chevron-down'}`} aria-hidden="true" />
						</button>
					)}
					<button
						type="button"
						className="icon-btn"
						onClick={() => onRun('normal')}
						disabled={runInFlight}
						title={`Run ${taskKey}`}
						aria-label={`Run ${taskKey}`}
					>
						<i className="bi bi-play-fill" aria-hidden="true" />
					</button>
					<button
						type="button"
						className="icon-btn"
						onClick={() => onRun('force')}
						disabled={runInFlight}
						title={`Run ${taskKey} again, ignoring the cache`}
						aria-label={`Run ${taskKey} again, ignoring the cache`}
					>
						<i className="bi bi-arrow-clockwise" aria-hidden="true" />
					</button>
				</div>
			</div>

			{view.missComponents && view.missComponents.length > 0 && (
				<div className="miss-chips" aria-label={view.missMessage}>
					<span style={{ fontSize: '0.72rem', color: 'var(--text-muted)' }}>
						cache miss:
					</span>
					{view.missComponents.map((component) => (
						<span key={component} className="chip">
							{component}
						</span>
					))}
				</div>
			)}

			{open && (
				<div className="task-row__log">
					<OutputPane taskKey={taskKey} />
				</div>
			)}
		</>
	);
};

export default TaskRow;
