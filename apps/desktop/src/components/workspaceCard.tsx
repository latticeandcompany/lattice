import { useIsDark } from '../hooks/useThemeTokens.ts';
import type { WorkspaceView } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import TaskRow from './taskRow.tsx';

interface WorkspaceCardProps {
	workspace: WorkspaceView;
	onRun: (workspace: string, task: string, mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

const WorkspaceCard = ({ workspace, onRun, runInFlight }: WorkspaceCardProps) => {
	const dark = useIsDark();

	return (
		<div className="card mb-3">
			<div className="ws-card__head">
				<LanguageMark
					tool={workspace.driver?.tool ?? null}
					language={workspace.driver?.language ?? null}
					dark={dark}
				/>
				<div className="flex-grow-1 min-w-0">
					<div className="ws-card__name">{workspace.name}</div>
					<div className="ws-card__path selectable">{workspace.path}</div>
				</div>
				<div className="d-flex align-items-center gap-1 flex-wrap justify-content-end">
					{workspace.driver && (
						<span className="chip" title={evidenceTitle(workspace.driver.via)}>
							{workspace.driver.tool}
						</span>
					)}
					{workspace.engines.map((engine) => (
						<span key={engine.name} className="chip">
							{engine.name} {engine.version ?? ''}
						</span>
					))}
				</div>
			</div>

			{workspace.tasks.length === 0 ? (
				<div className="p-3" style={{ color: 'var(--text-muted)', fontSize: '0.875rem' }}>
					No task in the pipeline resolves to a command here.
				</div>
			) : (
				workspace.tasks.map((task) => (
					<TaskRow
						key={task.task}
						workspace={workspace.name}
						task={task}
						runInFlight={runInFlight}
						onRun={(mode) => onRun(workspace.name, task.task, mode)}
					/>
				))
			)}
		</div>
	);
};

// Why this driver, in the terms the engine uses.
const evidenceTitle = (via: WorkspaceView['driver'] extends null ? never : { kind: string; file?: string }) => {
	switch (via.kind) {
		case 'declaration':
			return 'Declared in lattice.json';
		case 'nativeFile':
			return `From ${via.file}`;
		default:
			return `From ${via.file}`;
	}
};

export default WorkspaceCard;
