import { useApp } from '../context/appContext.tsx';
import { useFullPower } from '../hooks/useFullPower.ts';
import { useReconnect } from '../hooks/useReconnect.ts';
import CacheView from './cacheView.tsx';
import ConfigView from './configView.tsx';
import GraphView from './graphView.tsx';
import MigrateView from './migrateView.tsx';
import ProjectSwitcher from './projectSwitcher.tsx';
import Rosette from './rosette.tsx';
import SetupWizard from './setupWizard.tsx';
import SidebarNav from './sidebarNav.tsx';
import Spinner from './spinner.tsx';
import TasksView from './tasksView.tsx';
import ThemeControl from './themeControl.tsx';
import ToolchainsView from './toolchainsView.tsx';
import UnreadableConfig from './unreadableConfig.tsx';

// Screens holding results for one project are keyed by its root, so switching
// projects starts them over instead of showing the last project's results.
const Screen = () => {
	const { project, view, needsMigration, configText, pendingRoot } = useApp();

	if (needsMigration) return <MigrateView key={project?.root ?? pendingRoot ?? ''} />;
	if (!project) return configText !== null ? <UnreadableConfig /> : <SetupWizard />;
	switch (view) {
		case 'graph':
			return <GraphView />;
		case 'toolchains':
			return <ToolchainsView key={project.root} />;
		case 'cache':
			return <CacheView key={project.root} />;
		case 'config':
			return <ConfigView />;
		case 'setup':
			return <SetupWizard />;
		default:
			return <TasksView />;
	}
};

const AppShell = () => {
	const { project, info, error, dismissError, reload, busy } = useApp();
	useReconnect(project);
	useFullPower();

	return (
		<div className="app-shell">
			<aside className="app-rail" data-tauri-drag-region>
				<div className="rail-head">
					<Rosette size="1.5rem" />
					<span className="rail-wordmark">
						lattice <span className="wordmark__product">desktop</span>
					</span>
					{busy && <Spinner label="Working…" quiet className="ms-auto" />}
				</div>

				<ProjectSwitcher />
				<SidebarNav />

				<div className="rail-foot">
					{project ? (
						<button
							type="button"
							className="icon-btn"
							onClick={() => void reload()}
							disabled={busy}
							title="Read lattice.json and the workspaces again"
							aria-label="Reload the project from disk"
						>
							<i className="bi bi-arrow-repeat" aria-hidden="true" />
						</button>
					) : (
						<span />
					)}
					<span className="datum">{info?.latticeVersion ?? ''}</span>
					<ThemeControl />
				</div>
			</aside>

			<main className="app-main">
				{error && (
					<div className="app-banner">
						<div className="alert alert-danger alert-dismissible d-flex align-items-start gap-2" role="alert">
							<i className="bi bi-exclamation-triangle" aria-hidden="true" />
							<div className="flex-grow-1 selectable" style={{ whiteSpace: 'pre-wrap' }}>
								{error}
							</div>
							<button type="button" className="btn-close" aria-label="Dismiss" onClick={dismissError} />
						</div>
					</div>
				)}
				<Screen />
			</main>
		</div>
	);
};

export default AppShell;
