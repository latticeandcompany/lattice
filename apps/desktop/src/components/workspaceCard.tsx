import type { WorkspaceView } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import TaskRow from './taskRow.tsx';

interface WorkspaceCardProps {
	workspace: WorkspaceView;
	onRun: (workspace: string, task: string, mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

const WorkspaceCard = ({ workspace, onRun, runInFlight }: WorkspaceCardProps) => (
	<div className="card mb-3">
		<div className="ws-card__head">
			<LanguageMark
				tool={workspace.driver?.tool ?? null}
				language={workspace.driver?.language ?? null}
			/>
			<div className="flex-grow-1 tw:min-w-0">
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
				None of the tasks in this repo have anything to run in this folder.
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

// Why this tool, and not another.
const evidenceTitle = (via: { kind: string; file?: string }) =>
	via.kind === 'declaration'
		? 'You named this tool in lattice.json'
		: `Chosen because this folder has ${via.file}`;

export default WorkspaceCard;
