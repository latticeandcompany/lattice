import { useApp, type View } from '../context/appContext.tsx';

const DESTINATIONS: { view: View; label: string; icon: string }[] = [
	{ view: 'tasks', label: 'Tasks', icon: 'bi-play-circle' },
	{ view: 'graph', label: 'Graph', icon: 'bi-diagram-3' },
	{ view: 'toolchains', label: 'Toolchains', icon: 'bi-tools' },
	{ view: 'cache', label: 'Cache', icon: 'bi-lightning-charge' },
	{ view: 'config', label: 'Config', icon: 'bi-braces' },
];

const SidebarNav = () => {
	const { view, setView, project } = useApp();

	return (
		<nav className="rail-nav" aria-label="Views">
			{DESTINATIONS.map((destination) => (
				<button
					key={destination.view}
					type="button"
					className={`rail-link${view === destination.view ? ' active' : ''}`}
					onClick={() => setView(destination.view)}
					disabled={!project}
					aria-current={view === destination.view ? 'page' : undefined}
				>
					<i className={`bi ${destination.icon}`} aria-hidden="true" />
					<span>{destination.label}</span>
				</button>
			))}
		</nav>
	);
};

export default SidebarNav;
