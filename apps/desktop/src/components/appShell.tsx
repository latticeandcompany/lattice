import { useApp } from '../context/appContext.tsx';
import { shortenPath } from '../lib/format.ts';
import ConfigView from './configView.tsx';
import GraphView from './graphView.tsx';
import ProjectSwitcher from './projectSwitcher.tsx';
import Rosette from './rosette.tsx';
import SetupWizard from './setupWizard.tsx';
import SidebarNav from './sidebarNav.tsx';
import TaskListView from './taskListView.tsx';
import ThemeControl from './themeControl.tsx';

const AppShell = () => {
	const { project, view, info, error, dismissError, reload, busy } = useApp();

	return (
		<div className="app-shell">
			<aside className="app-rail" data-tauri-drag-region>
				<div className="app-rail__head">
					<Rosette size="1.6rem" />
					<span className="app-rail__wordmark">lattice</span>
				</div>

				{project && (
					<div className="app-rail__project">
						<div className="app-rail__project-name" title={project.name}>
							{project.name}
						</div>
						<div className="app-rail__project-path" title={project.root}>
							{shortenPath(project.root, 2)}
						</div>
					</div>
				)}

				<SidebarNav />

				{project && (
					<div>
						<div className="app-rail__group">Repo</div>
						<button
							type="button"
							className="rail-link nav-link nav-link--quiet"
							onClick={() => void reload()}
							disabled={busy}
						>
							<i className="bi bi-arrow-repeat" aria-hidden="true" />
							<span>Reload from disk</span>
						</button>
					</div>
				)}

				<div className="app-rail__foot">
					<span
						style={{
							fontFamily: 'DM Mono, ui-monospace, monospace',
							fontSize: '0.68rem',
							color: 'var(--text-muted)',
						}}
					>
						{info?.latticeVersion ?? ''}
					</span>
					<div className="d-flex align-items-center">
						<ProjectSwitcher />
						<ThemeControl />
					</div>
				</div>
			</aside>

			<main className="app-main">
				{error && (
					<div className="run-bar">
						<div className="notice notice--bad">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="flex-grow-1 selectable">{error}</div>
							<button
								type="button"
								className="btn-close btn-sm"
								aria-label="Dismiss"
								onClick={dismissError}
							/>
						</div>
					</div>
				)}

				{!project || view === 'setup' ? (
					<SetupWizard />
				) : view === 'tasks' ? (
					<TaskListView />
				) : view === 'graph' ? (
					<GraphView />
				) : (
					<ConfigView />
				)}
			</main>
		</div>
	);
};

export default AppShell;
