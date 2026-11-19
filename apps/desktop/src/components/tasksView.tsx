import { useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useRunView } from '../hooks/useRunStore.ts';
import { useRunner } from '../hooks/useRunner.ts';
import { effectiveSelection, parseArgs, taskNamesOf } from '../lib/runOptions.ts';
import { nameList } from '../lib/toolchains.ts';
import RunBar, { type RunBarState } from './runBar.tsx';
import WorkspaceSection from './workspaceSection.tsx';

// What a run of the picked tasks will do, workspace by workspace. A workspace that
// has none of them is not a section; it is named once at the end, the way the CLI
// says who sat a run out.
const TasksView = () => {
	const { project, setView } = useApp();
	const run = useRunView();
	const runner = useRunner();

	const taskNames = useMemo(() => taskNamesOf(project?.workspaces ?? []), [project]);
	const [bar, setBar] = useState<RunBarState>(() => ({
		selected: [],
		mode: 'normal',
		filter: '',
		concurrency: '',
		keepGoing: false,
		sequentially: false,
		args: '',
	}));
	// Not remounted when the project changes, so what was picked in the last one is
	// filtered against what this one has.
	const selected = useMemo(() => effectiveSelection(bar.selected, taskNames), [bar, taskNames]);

	if (!project) return null;

	const inFlight = run.phase === 'running' || run.phase === 'stopping';
	const args = parseArgs(bar.args);

	const start = () =>
		void runner.start({
			tasks: selected,
			filter: bar.filter.trim() || undefined,
			args: args.length > 0 ? args : undefined,
			sequentially: bar.sequentially,
			concurrency: bar.concurrency ? Number(bar.concurrency) : undefined,
			keepGoing: bar.keepGoing,
			mode: bar.mode,
		});

	const runOne = (workspace: string, task: string, mode: 'normal' | 'force') =>
		void runner.start({
			tasks: [task],
			workspaces: [workspace],
			sequentially: false,
			keepGoing: false,
			mode,
		});

	const involved = project.workspaces.filter((ws) => ws.tasks.some((task) => selected.includes(task.task)));
	const satOut = project.workspaces.filter((ws) => !involved.includes(ws)).map((ws) => ws.name);

	return (
		<>
			<RunBar
				tasks={taskNames}
				state={{ ...bar, selected }}
				onChange={(next) => setBar((current) => ({ ...current, ...next }))}
				onRun={start}
				onStop={() => void runner.stop()}
			/>

			<div className="app-scroll">
				<div className="app-column">
					{runner.error && (
						<div className="alert alert-danger alert-dismissible d-flex align-items-start gap-2 mb-3" role="alert">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="flex-grow-1 selectable" style={{ whiteSpace: 'pre-wrap' }}>
								{runner.error}
							</div>
							<button type="button" className="btn-close" aria-label="Dismiss" onClick={runner.dismissError} />
						</div>
					)}

					{run.outcome?.status === 'interrupted' && (
						<div className="alert alert-secondary mb-3" role="status">
							Stopped. Every task that was running was ended.
						</div>
					)}

					{project.workspaces.length === 0 ? (
						<div className="empty-state">
							<h2>Lattice found no workspaces here</h2>
							<p>
								Nothing at the root names workspaces, and nothing here is a project Lattice recognizes.
								List their directories under workspaces in the config.
							</p>
							<button type="button" className="btn btn-outline-secondary btn-sm" onClick={() => setView('config')}>
								Open the config
							</button>
						</div>
					) : (
						<>
							{involved.map((workspace) => (
								<WorkspaceSection
									key={workspace.name}
									workspace={workspace}
									tasks={selected}
									runInFlight={inFlight}
									onRun={runOne}
								/>
							))}
							{satOut.length > 0 && (
								<p className="sat-out">
									No {selected.join(' or ')} task in {nameList(satOut, 6)}.
								</p>
							)}
						</>
					)}
				</div>
			</div>
		</>
	);
};

export default TasksView;
