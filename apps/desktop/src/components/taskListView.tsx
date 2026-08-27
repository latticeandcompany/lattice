import { useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useRunView } from '../hooks/useRunStore.ts';
import { useRunner } from '../hooks/useRunner.ts';
import { defaultSelection, effectiveSelection } from '../lib/runOptions.ts';
import RunBar, { type RunBarState } from './runBar.tsx';
import WorkspaceCard from './workspaceCard.tsx';

// One card per workspace, in declaration order, with one row per task that actually
// resolves to a command there. Not a workspaces-by-tasks matrix: with six tasks and
// forty workspaces that is unreadable, and the run bar already scopes the view.
const TaskListView = () => {
	const { project } = useApp();
	const run = useRunView();
	const runner = useRunner();

	const taskNames = useMemo(() => project?.tasks.map((task) => task.name) ?? [], [project]);
	const [bar, setBar] = useState<RunBarState>(() => ({
		selected: defaultSelection(taskNames),
		mode: 'normal',
		filter: '',
		concurrency: '',
		keepGoing: false,
		sequentially: false,
	}));
	// This view is not remounted when the project changes, so what was picked in the
	// last one is filtered against what this one actually defines.
	const selected = useMemo(() => effectiveSelection(bar.selected, taskNames), [bar, taskNames]);

	if (!project) return null;

	const inFlight = run.phase === 'running' || run.phase === 'stopping';

	const settings = () => ({
		tasks: selected,
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
				state={{ ...bar, selected }}
				onChange={(next) => setBar((current) => ({ ...current, ...next }))}
				onRun={() => void runner.start(settings())}
				onStop={() => void runner.stop()}
			/>

			<div className="app-main__scroll">
				<div className="app-main__inner">
					{runner.error && (
						<div
							className="alert alert-danger alert-dismissible d-flex align-items-start gap-2 mb-3"
							role="alert"
						>
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="flex-grow-1 selectable">{runner.error}</div>
							<button
								type="button"
								className="btn-close"
								aria-label="Dismiss"
								onClick={runner.dismissError}
							/>
						</div>
					)}

					{failure && run.outcome?.status === 'interrupted' && (
						<div className="alert alert-secondary d-flex align-items-start gap-2 mb-3" role="alert">
							<i className="bi bi-slash-circle" aria-hidden="true" />
							<div>Interrupted. Every running task was stopped.</div>
						</div>
					)}

					{run.warnings.length > 0 && (
						<div className="alert alert-secondary d-flex align-items-start gap-2 mb-3" role="alert">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="selectable">
								{run.warnings.map((warning, index) => (
									<div key={index}>{warning}</div>
								))}
							</div>
						</div>
					)}

					{project.workspaces.length === 0 ? (
						<div className="d-flex flex-column align-items-center justify-content-center gap-3 text-center text-body-secondary py-5">
							<i className="bi bi-inboxes fs-2" aria-hidden="true" />
							<div>No workspaces declared. Add one in Config.</div>
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
