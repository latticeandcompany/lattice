import type { Evidence, WorkspaceView } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import TaskRow from './taskRow.tsx';

interface WorkspaceSectionProps {
	workspace: WorkspaceView;
	/** The tasks to show, in order. Every task the workspace has when absent. */
	tasks?: readonly string[];
	/** Absent where there is nothing to run, as on the website's recording. */
	onRun?: (workspace: string, task: string, mode: 'normal' | 'force') => void;
	runInFlight: boolean;
}

// Why this tool, and not another: the file that decided it.
const evidence = (via: Evidence): string => {
	switch (via.kind) {
		case 'declaration':
			return 'set by driver in lattice.json';
		case 'lockfile':
			return `found ${via.file}`;
		case 'manifest':
			return `only ${via.file} reads like this`;
		default:
			return `named in ${via.file}`;
	}
};

const WorkspaceSection = ({ workspace, tasks, onRun, runInFlight }: WorkspaceSectionProps) => {
	const rows = tasks
		? tasks.flatMap((name) => workspace.tasks.filter((task) => task.task === name))
		: workspace.tasks;
	const declared = workspace.dependsOn.filter((dep) => !workspace.inferredDeps.includes(dep));

	return (
		<section className="ws" aria-label={workspace.name}>
			<div className="ws__head">
				<LanguageMark tool={workspace.driver?.tool ?? null} language={workspace.driver?.language ?? null} />
				<span className="ws__name">{workspace.name}</span>
				<span className="ws__path selectable" title={workspace.path}>
					{workspace.path}
				</span>
				{workspace.driver && (
					<span className="ws__driver" title={evidence(workspace.driver.via)}>
						{workspace.driver.tool}
					</span>
				)}
			</div>

			{workspace.dependsOn.length > 0 && (
				<p className="ws__after">
					after{' '}
					{workspace.dependsOn.map((dep, index) => (
						<span key={dep}>
							{index > 0 && ', '}
							{declared.includes(dep) ? (
								<span className="ws__declared" title="Written in lattice.json">
									{dep}
								</span>
							) : (
								dep
							)}
						</span>
					))}
				</p>
			)}

			{workspace.problems.map((problem) => (
				<p className="ws__problem selectable" key={problem}>
					{problem}
				</p>
			))}

			{rows.map((task) => (
				<TaskRow
					key={task.task}
					workspace={workspace.name}
					task={task}
					runInFlight={runInFlight}
					onRun={onRun ? (mode) => onRun(workspace.name, task.task, mode) : undefined}
				/>
			))}
		</section>
	);
};

export default WorkspaceSection;
