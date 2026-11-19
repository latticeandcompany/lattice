import { useEffect, useState } from 'react';

import WorkspaceSection from '../../../desktop/src/components/workspaceSection.tsx';
import { useRunView } from '../../../desktop/src/hooks/useRunStore.ts';
import { isFullCache, runSummary } from '../../../desktop/src/lib/format.ts';
import { RUN_MS, settleDemo, startDemo, stopDemo, workspaces } from '../lib/desktopDemo.ts';
import '../styles/desktopApp.scss';

// The run sheet in the /desktop hero, which is the app's own — WorkspaceSection,
// TaskRow, LanguageMark, OutputPane and the run store, imported from apps/desktop.
// This file adds the frame and a replay button.

const RunSummary = () => {
	const run = useRunView();
	if (!run.result) return null;
	// The same expression the app's run bar uses, which is the CLI's summary line.
	return (
		<span className="font-monospace text-body-secondary small">
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
				<span className="flex-grow-1"></span>
				<RunSummary />
				<button type="button" className="btn btn-sm border text-body-secondary" onClick={replay} disabled={running}>
					{running ? 'Running' : 'Replay'}
				</button>
			</div>

			<div className="desktop-window__body">
				{workspaces.map((workspace) => (
					// A recording has nothing to run, so rows get no Run buttons.
					<WorkspaceSection key={workspace.name} workspace={workspace} runInFlight={false} />
				))}
			</div>
		</div>
	);
};

export default DesktopWindow;
