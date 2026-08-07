import { useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useRunView } from '../hooks/useRunStore.ts';
import { useRunner } from '../hooks/useRunner.ts';
import RunBar, { type RunBarState } from './runBar.tsx';
import WorkspaceCard from './workspaceCard.tsx';

// One card per workspace, in declaration order, with one row per task that actually
// resolves to a command there. Not a workspaces-by-tasks matrix: with six tasks and
// forty workspaces that is unreadable, and the run bar already scopes the view.
const TaskListView = () => {
	const { project } = useApp();
	const run = useRunView();
	const runner = useRunner();

	const taskNames = project?.tasks.map((task) => task.name) ?? [];
	const [bar, setBar] = useState<RunBarState>({
		selected: taskNames.includes('build') ? ['build'] : taskNames.slice(0, 1),
		mode: 'normal',
		filter: '',
		concurrency: '',
		keepGoing: false,
		sequentially: false,
	});

	if (!project) return null;

	const inFlight = run.phase === 'running' || run.phase === 'stopping';

	const settings = () => ({
		tasks: bar.selected,
		filter: bar.filter.trim() || undefined,
		sequentially: bar.sequentially,
		concurrency: bar.concurrency ? Number(bar.concurrency) : undefined,
		keepGoing: bar.keepGoing,
		mode: bar.mode,
	});

	const runOne = (workspace: string, task: string, mode: 'normal' | 'force') => {
		// A single row's button runs just that workspace, which is what --filter does.
		void runner.start({
			tasks: [task],
			filter: workspace,
			sequentially: false,
			concurrency: undefined,
			keepGoing: false,
			mode,
		});
	};

	const failure = run.outcome?.status === 'failed' || run.outcome?.status === 'interrupted';

	return (
		<>
			<RunBar
				tasks={taskNames}
				state={bar}
				onChange={(next) => setBar((current) => ({ ...current, ...next }))}
				onRun={() => void runner.start(settings())}
				onStop={() => void runner.stop()}
			/>

			<div className="app-main__scroll">
				<div className="app-main__inner">
					{runner.error && (
						<div className="notice notice--bad mb-3">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="flex-grow-1 selectable">{runner.error}</div>
							<button
								type="button"
								className="btn-close btn-sm"
								aria-label="Dismiss"
								onClick={runner.dismissError}
							/>
						</div>
					)}

					{failure && run.outcome?.status === 'interrupted' && (
						<div className="notice mb-3">
							<i className="bi bi-slash-circle" aria-hidden="true" />
							<div>interrupted — running tasks were stopped</div>
						</div>
					)}

					{run.warnings.length > 0 && (
						<div className="notice mb-3">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="selectable">
								{run.warnings.map((warning, index) => (
									<div key={index}>{warning}</div>
								))}
							</div>
						</div>
					)}

					{project.workspaces.length === 0 ? (
						<div className="empty-state">
							<i className="bi bi-inboxes fs-2" aria-hidden="true" />
							<div>
								No workspaces declared. Add them to the <code>workspaces</code> array in
								lattice.json.
							</div>
						</div>
					) : (
						project.workspaces.map((workspace) => (
							<WorkspaceCard
								key={workspace.name}
								workspace={workspace}
								runInFlight={inFlight}
								onRun={runOne}
							/>
						))
					)}
				</div>
			</div>
		</>
	);
};

export default TaskListView;
