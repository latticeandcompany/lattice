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
		<div className="card-header bg-transparent d-flex align-items-center gap-2">
			<LanguageMark
				tool={workspace.driver?.tool ?? null}
				language={workspace.driver?.language ?? null}
			/>
			<div className="flex-grow-1 tw:min-w-0">
				<div className="fw-bold tw:text-[1.05rem] tw:tracking-[-0.01em]">{workspace.name}</div>
				<div className="font-monospace text-body-secondary selectable tw:text-[0.75rem]">
					{workspace.path}
				</div>
			</div>
			<div className="d-flex align-items-center gap-1 flex-wrap justify-content-end">
				{workspace.driver && (
					<span className="badge border bg-body-tertiary text-body-secondary fw-normal font-monospace" title={evidenceTitle(workspace.driver.via)}>
						{workspace.driver.tool}
					</span>
				)}
				{workspace.engines.map((engine) => (
					<span key={engine.name} className="badge border bg-body-tertiary text-body-secondary fw-normal font-monospace">
						{engine.name} {engine.version ?? ''}
					</span>
				))}
			</div>
		</div>

		{workspace.tasks.length === 0 ? (
			<div className="p-3 small text-body-secondary">
				No task in this project resolves to a command in this workspace.
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

// Why this driver, and not another.
const evidenceTitle = (via: { kind: string; file?: string }) =>
	via.kind === 'declaration'
		? 'Driver declared in lattice.json'
		: `Driver detected from ${via.file}`;

export default WorkspaceCard;
