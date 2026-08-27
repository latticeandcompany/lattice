import { useEffect, useState } from 'react';

import WorkspaceCard from '../../../desktop/src/components/workspaceCard.tsx';
import { useRunView } from '../../../desktop/src/hooks/useRunStore.ts';
import { isFullCache, runSummary } from '../../../desktop/src/lib/format.ts';
import { RUN_MS, settleDemo, startDemo, stopDemo, workspaces } from '../lib/desktopDemo.ts';
import '../styles/desktopApp.scss';

// The task list in the /desktop hero, which is the app's task list — WorkspaceCard,
// TaskRow, LanguageMark, OutputPane and the run store, imported from apps/desktop. The
// only thing this file adds is a window frame and a replay button.

const RunSummary = () => {
	const run = useRunView();
	if (!run.result) return null;
	// The same expression the app's run bar uses, which is the CLI's summary line.
	return (
		<span className="font-monospace text-body-secondary tw:text-[0.72rem]">
			{runSummary(run.result)}
			{isFullCache(run.result) ? ' · full power, nothing to run' : ''}
		</span>
	);
};

const DesktopWindow = () => {
	const [running, setRunning] = useState(false);

	useEffect(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			settleDemo();
			return;
		}
		setRunning(true);
		startDemo();
		return stopDemo;
	}, []);

	useEffect(() => {
		if (!running) return;
		const done = setTimeout(() => setRunning(false), RUN_MS + 400);
		return () => clearTimeout(done);
	}, [running]);

	const replay = () => {
		setRunning(true);
		startDemo();
	};

	return (
		<div className="desktop-window">
			<div className="desktop-window__bar">
				<span className="fw-medium">acme-monorepo</span>
				<span className="badge border bg-body-tertiary text-body-secondary fw-normal font-monospace">
					mega
				</span>
				<span className="flex-grow-1"></span>
				<RunSummary />
				<button
					type="button"
					className="btn btn-sm border text-body-secondary font-monospace tw:text-[0.72rem]"
					onClick={replay}
					disabled={running}
				>
					{running ? 'Running' : 'Replay'}
				</button>
			</div>

			<div className="desktop-window__body">
				{workspaces.map((workspace) => (
					<WorkspaceCard
						key={workspace.name}
						workspace={workspace}
						// A recording has nothing to run, so the row buttons stay inert rather
						// than pretending to start something.
						onRun={() => {}}
						runInFlight={false}
					/>
				))}
			</div>

			<div className="desktop-window__fade" aria-hidden="true"></div>
		</div>
	);
};

export default DesktopWindow;
