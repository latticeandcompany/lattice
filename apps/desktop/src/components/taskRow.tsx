import { useEffect, useState } from 'react';

import { useTaskView } from '../hooks/useRunStore.ts';
import { isBusy, opensOnFailure, statusView } from '../lib/taskStatus.ts';
import type { WorkspaceTaskView } from '../lib/types.ts';
import IconButton from './iconButton.tsx';
import OutputPane from './outputPane.tsx';

interface TaskRowProps {
	workspace: string;
	task: WorkspaceTaskView;
	onRun: (mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

// Mono is for values. The cache-miss components are names out of the cache key, so
// they get it; "persistent" and "never cached" are words about the task, so they do not.
const VALUE = 'badge border bg-body-tertiary text-body-secondary fw-normal font-monospace';
const LABEL = 'badge border bg-body-tertiary text-body-secondary fw-normal';

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
							<span className={`${LABEL} ms-2`} title="Declared persistent: true">
								persistent
							</span>
						)}
						{!task.cacheable && (
							<span className={`${LABEL} ms-2`} title="Declared cache: false">
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
							className="spinner-border spinner-border-sm tw:h-[0.8rem] tw:w-[0.8rem] tw:border-[0.12em]"
							role="status"
							aria-hidden="true"
						/>
					) : (
						status.icon && <i className={`bi ${status.icon}`} aria-hidden="true" />
					)}
					<span>{status.label}</span>
				</div>

				<div className="task-row__actions">
					{view.hasOutput && (
						<IconButton
							icon={open ? 'bi-chevron-up' : 'bi-chevron-down'}
							label={open ? `Hide output for ${taskKey}` : `Show output for ${taskKey}`}
							onClick={() => setOpen((value) => !value)}
							expanded={open}
						/>
					)}
					<IconButton
						icon="bi-play-fill"
						label={`Run ${taskKey}`}
						onClick={() => onRun('normal')}
						disabled={runInFlight}
					/>
					<IconButton
						icon="bi-arrow-clockwise"
						label={`Run ${taskKey} again, ignoring the cache`}
						onClick={() => onRun('force')}
						disabled={runInFlight}
					/>
				</div>
			</div>

			{view.missComponents && view.missComponents.length > 0 && (
				<div className="miss-chips" aria-label={view.missMessage}>
					<span className="text-body-secondary tw:text-[0.72rem]">cache miss:</span>
					{view.missComponents.map((component) => (
						<span key={component} className={VALUE}>
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
