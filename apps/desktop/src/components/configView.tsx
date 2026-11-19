import { useEffect, useMemo, useState, type ReactNode } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { removeAt, setValue } from '../lib/configEdit.ts';
import { isDetailed, isPathKey, overrideCommand, overrideFacets } from '../lib/overrides.ts';
import type { ConfigDiagnostic, LatticeConfig, PipelineTask, WorkspaceOverride } from '../lib/types.ts';
import FieldHint from './fieldHint.tsx';
import IconButton from './iconButton.tsx';
import LanguageMark from './languageMark.tsx';
import PageHead from './pageHead.tsx';
import Spinner from './spinner.tsx';

type Tab = 'form' | 'json';
type Section = 'workspaces' | 'engines' | 'tasks' | 'overrides' | 'project';

// The editor's source of truth is the text, never a parsed object.
//
// The Rust config types reject unknown keys, but a key this build of the app has
// never heard of is still valid input for a newer Lattice. Every control computes a
// minimal edit against the original string, so unknown keys, key order and the
// file's own formatting all survive a save.
//
// Every control is labelled in English and carries the key it writes beside it: the
// English makes the form usable without the schema, the key keeps the form from
// hiding the schema.
const ConfigView = () => {
	const { project, configText, diagnostics, setConfigText, saveConfig, busy, catalog } = useApp();
	const [tab, setTab] = useState<Tab>('form');
	const [section, setSection] = useState<Section>('workspaces');
	const [live, setLive] = useState<ConfigDiagnostic[]>([]);
	const [dirty, setDirty] = useState(false);

	const text = configText ?? '';
	const parsed = useMemo<LatticeConfig | null>(() => {
		try {
			return JSON.parse(text) as LatticeConfig;
		} catch {
			return null;
		}
	}, [text]);

	// Validation is the backend's, debounced: the same parse the CLI does, so the
	// editor never disagrees with what a run would say.
	useEffect(() => {
		if (!dirty) return;
		const timer = setTimeout(() => {
			void (async () => {
				try {
					setLive(await api.configValidate(text));
				} catch {
					// A failed validate call is not worth surfacing over the editor.
				}
			})();
		}, 300);
		return () => clearTimeout(timer);
	}, [text, dirty]);

	if (!project || configText === null) return null;

	const edit = (next: string) => {
		setConfigText(next);
		setDirty(true);
	};

	const problems = dirty ? live : diagnostics;
	const engines = Object.entries(parsed?.engines ?? {});
	const tasks = Object.entries(parsed?.tasks ?? {});
	const overrides = Object.entries(parsed?.overrides ?? {});
	const knownEngines = catalog?.engines.map((engine) => engine.name) ?? [];
	const knownDrivers = catalog?.drivers.map((driver) => driver.tool) ?? [];

	const sections: { id: Section; label: string; count?: number }[] = [
		{ id: 'workspaces', label: 'Workspaces', count: project.workspaces.length },
		{ id: 'engines', label: 'Engines', count: engines.length },
		{ id: 'tasks', label: 'Tasks', count: tasks.length },
		{ id: 'overrides', label: 'Overrides', count: overrides.length },
		{ id: 'project', label: 'Settings' },
	];

	const save = () =>
		void saveConfig(text).then((saved) => {
			// A save that failed resolves too: the error goes to the banner.
			if (!saved) return;
			setDirty(false);
			setLive([]);
		});

	return (
		<>
			<PageHead title="Config">
				<div className="picker" role="group" aria-label="Editor">
					<button type="button" className="picker__option" onClick={() => setTab('form')} aria-pressed={tab === 'form'}>
						Form
					</button>
					<button type="button" className="picker__option" onClick={() => setTab('json')} aria-pressed={tab === 'json'}>
						lattice.json
					</button>
				</div>
				<span className="small text-body-secondary">{dirty ? 'Unsaved changes' : 'Saved'}</span>
				<button
					type="button"
					className="btn btn-primary btn-sm d-inline-flex align-items-center gap-2"
					disabled={busy || !dirty || problems.length > 0}
					onClick={save}
				>
					{busy && <Spinner label="Saving" quiet />}
					Save
				</button>
			</PageHead>

			<div className="app-scroll">
				<div className="app-column">
					{problems.map((problem, index) => (
						<pre className="cli-error selectable mb-3" key={index}>
							{problem.line !== null ? `line ${problem.line}: ` : ''}
							{problem.message}
						</pre>
					))}

					{tab === 'json' ? (
						<textarea
							className="config-source selectable"
							spellCheck={false}
							value={text}
							onChange={(event) => edit(event.target.value)}
							aria-label="lattice.json source"
						/>
					) : parsed === null ? (
						<div className="empty-state">
							<h2>lattice.json is not valid JSON</h2>
							<p>The form cannot show it until it parses. Fix it in the lattice.json tab.</p>
						</div>
					) : (
						<div className="config-layout">
							<nav className="config-index" aria-label="Config sections">
								{sections.map((entry) => (
									<button
										key={entry.id}
										type="button"
										className={`config-index__link${section === entry.id ? ' active' : ''}`}
										onClick={() => setSection(entry.id)}
										aria-current={section === entry.id ? 'true' : undefined}
									>
										<span>{entry.label}</span>
										{entry.count !== undefined && <span className="config-index__count">{entry.count}</span>}
									</button>
								))}
							</nav>

							<div>
								{section === 'workspaces' && (
									<section className="config-section">
										<h2>Workspaces</h2>
										<p>
											Workspaces named by package.json, pnpm-workspace.yaml, Cargo.toml, go.work, uv or deno are found
											without being listed. List other directories here, or globs of them.
										</p>
										<Field label="Also workspaces" jsonKey="workspaces" hint="Relative to the repo root. apps/* takes every directory under apps that holds a project.">
											<StringList
												label="workspaces"
												values={parsed.workspaces ?? []}
												placeholder="services/*"
												onChange={(next) => edit(setValue(text, ['workspaces'], next.length > 0 ? next : undefined))}
											/>
										</Field>
										<Field label="Leave out" jsonKey="exclude" hint="Globs of workspace paths to drop, whoever named them.">
											<StringList
												label="exclude"
												values={parsed.exclude ?? []}
												placeholder="examples/**"
												onChange={(next) => edit(setValue(text, ['exclude'], next.length > 0 ? next : undefined))}
											/>
										</Field>

										<h3 className="mt-4">What Lattice found</h3>
										<table className="table table-sm found-table">
											<thead>
												<tr>
													<th scope="col">Workspace</th>
													<th scope="col">Path</th>
													<th scope="col">Runs with</th>
													<th scope="col">Tasks</th>
												</tr>
											</thead>
											<tbody>
												{project.workspaces.map((workspace) => (
													<tr key={workspace.name}>
														<td>
															<span className="d-inline-flex align-items-center gap-2">
																<LanguageMark
																	tool={workspace.driver?.tool ?? null}
																	language={workspace.driver?.language ?? null}
																/>
																{workspace.name}
															</span>
														</td>
														<td className="font-monospace small">{workspace.path}</td>
														<td className="small">{workspace.driver?.tool ?? 'commands in overrides'}</td>
														<td className="small text-body-secondary">{workspace.tasks.length}</td>
													</tr>
												))}
											</tbody>
										</table>
									</section>
								)}

								{section === 'engines' && (
									<section className="config-section">
										<h2>Engines</h2>
										<p>
											Tool versions every workspace runs with. A run stops before any task starts if the tool on PATH
											does not match. To have Lattice install a tool itself, give its engine an installCmd in
											lattice.json.
										</p>
										{engines.map(([name, spec]) => (
											<div className="string-list__row mb-2" key={name} style={{ maxWidth: '26rem' }}>
												<span className="entry__title align-self-center" style={{ minWidth: '6rem' }}>
													{name}
												</span>
												<input
													className="form-control form-control-sm font-monospace"
													value={typeof spec === 'string' ? spec : (spec.version ?? '')}
													placeholder="any version"
													onChange={(event) =>
														edit(
															setValue(
																text,
																typeof spec === 'string' ? ['engines', name] : ['engines', name, 'version'],
																event.target.value,
															),
														)
													}
													aria-label={`${name} version`}
												/>
												{typeof spec !== 'string' && spec.installCmd && (
													<span className="small text-body-secondary align-self-center">installs</span>
												)}
												<IconButton icon="bi-x" label={`Stop pinning ${name}`} onClick={() => edit(removeAt(text, ['engines', name]))} />
											</div>
										))}
										<AddRow
											placeholder="node"
											label="Pin a tool"
											suggestions={knownEngines.filter((name) => !parsed.engines?.[name])}
											onAdd={(name) => edit(setValue(text, ['engines', name], '>=0'))}
										/>
									</section>
								)}

								{section === 'tasks' && (
									<section className="config-section">
										<h2>Tasks</h2>
										<p>
											How a task behaves in every workspace that has it. A task does not need to be listed to run: any
											script or verb a workspace has runs by name. build waits for the builds it depends on, and dev,
											start, serve and watch keep running, without saying so here.
										</p>
										{tasks.map(([name, task]) => (
											<TaskEntry
												key={name}
												name={name}
												task={task}
												onEdit={(path, value) => edit(setValue(text, ['tasks', name, ...path], value))}
												onRemove={() => edit(removeAt(text, ['tasks', name]))}
											/>
										))}
										<AddRow
											placeholder="test"
											label="Configure a task"
											suggestions={[...new Set(project.workspaces.flatMap((ws) => ws.tasks.map((t) => t.task)))].filter(
												(name) => !parsed.tasks?.[name],
											)}
											onAdd={(name) => edit(setValue(text, ['tasks', name], {}))}
										/>
									</section>
								)}

								{section === 'overrides' && (
									<section className="config-section">
										<h2>Overrides</h2>
										<p>
											Settings for particular workspaces: a dependency between languages, the tool that runs them, or a
											command written out. A key is a workspace name, or a glob over paths when it has a / in it.
										</p>
										{overrides.map(([key, override]) => (
											<OverrideEntry
												key={key}
												id={key}
												override={override}
												drivers={knownDrivers}
												workspaces={project.workspaces.map((ws) => ws.name)}
												onEdit={(path, value) => edit(setValue(text, ['overrides', key, ...path], value))}
												onRemove={() => edit(removeAt(text, ['overrides', key]))}
											/>
										))}
										<AddRow
											placeholder="workspace name or apps/*"
											label="Add an override"
											suggestions={project.workspaces.map((ws) => ws.name).filter((name) => !parsed.overrides?.[name])}
											onAdd={(key) => edit(setValue(text, ['overrides', key], {}))}
										/>
									</section>
								)}

								{section === 'project' && (
									<section className="config-section">
										<h2>Settings</h2>
										<div className="entry__grid">
											<Field label="Lattice version" jsonKey="latticeVersion" hint="A Lattice that installed itself switches to this version. Any other install warns and runs anyway.">
												<input
													className="form-control form-control-sm"
													placeholder="any version"
													value={parsed.latticeVersion ?? ''}
													onChange={(event) => edit(setValue(text, ['latticeVersion'], event.target.value || undefined))}
												/>
											</Field>
											<Field label="Cache size limit" jsonKey="settings.maxCacheSize" hint="After a run, the least recently used entries are deleted to get back under this.">
												<input
													className="form-control form-control-sm"
													placeholder="10GB"
													value={parsed.settings?.maxCacheSize ?? ''}
													onChange={(event) =>
														edit(setValue(text, ['settings', 'maxCacheSize'], event.target.value || undefined))
													}
												/>
											</Field>
											<Field label="Cache directory" jsonKey="settings.cacheDir" hint="A path inside the repo. Empty means .lattice/cache.">
												<input
													className="form-control form-control-sm"
													placeholder=".lattice/cache"
													value={parsed.settings?.cacheDir ?? ''}
													onChange={(event) => edit(setValue(text, ['settings', 'cacheDir'], event.target.value || undefined))}
												/>
											</Field>
										</div>
										<Field label="Files every task depends on" jsonKey="globalDependencies" hint="Repo-root globs hashed into every task's cache key, such as a shared tsconfig.">
											<StringList
												label="globalDependencies"
												values={parsed.globalDependencies ?? []}
												placeholder="tsconfig.base.json"
												onChange={(next) =>
													edit(setValue(text, ['globalDependencies'], next.length > 0 ? next : undefined))
												}
											/>
										</Field>
										<Field label="Environment every task depends on" jsonKey="globalEnv" hint="Variable names whose values are hashed into every task's cache key.">
											<StringList
												label="globalEnv"
												values={parsed.globalEnv ?? []}
												placeholder="NODE_ENV"
												onChange={(next) => edit(setValue(text, ['globalEnv'], next.length > 0 ? next : undefined))}
											/>
										</Field>
									</section>
								)}
							</div>
						</div>
					)}
				</div>
			</div>
		</>
	);
};

const Field = ({ label, jsonKey, hint, children }: { label: string; jsonKey: string; hint: string; children: ReactNode }) => (
	<div className="field">
		<span className="field__label">
			{label} <span className="field__key">{jsonKey}</span>
			<FieldHint label={label}>{hint}</FieldHint>
		</span>
		{children}
	</div>
);

const StringList = ({
	label,
	values,
	placeholder,
	onChange,
}: {
	label: string;
	values: string[];
	placeholder: string;
	onChange: (next: string[]) => void;
}) => (
	<div className="string-list">
		{values.map((value, index) => (
			<div className="string-list__row" key={index}>
				<input
					className="form-control form-control-sm"
					value={value}
					onChange={(event) => {
						const next = [...values];
						next[index] = event.target.value;
						onChange(next);
					}}
					aria-label={`${label}, entry ${index + 1}`}
				/>
				<IconButton icon="bi-x" label={`Remove ${value || 'this entry'}`} onClick={() => onChange(values.filter((_, at) => at !== index))} />
			</div>
		))}
		<AddRow placeholder={placeholder} label={`Add to ${label}`} onAdd={(value) => onChange([...values, value])} />
	</div>
);

const AddRow = ({
	placeholder,
	label,
	suggestions = [],
	onAdd,
}: {
	placeholder: string;
	label: string;
	suggestions?: string[];
	onAdd: (value: string) => void;
}) => {
	const [value, setValue] = useState('');
	const listId = `add-${label.replace(/\W+/g, '-')}`;
	const add = () => {
		const trimmed = value.trim();
		if (!trimmed) return;
		onAdd(trimmed);
		setValue('');
	};
	return (
		<div className="add-row">
			<input
				className="form-control form-control-sm"
				placeholder={placeholder}
				value={value}
				list={suggestions.length > 0 ? listId : undefined}
				onChange={(event) => setValue(event.target.value)}
				onKeyDown={(event) => {
					if (event.key === 'Enter') add();
				}}
				aria-label={label}
			/>
			{suggestions.length > 0 && (
				<datalist id={listId}>
					{suggestions.map((suggestion) => (
						<option key={suggestion} value={suggestion} />
					))}
				</datalist>
			)}
			<button type="button" className="btn btn-outline-secondary btn-sm text-nowrap" onClick={add} disabled={!value.trim()}>
				{label}
			</button>
		</div>
	);
};

// `Option<bool>` must not collapse to a switch: writing `false` where the file said
// nothing is a different config, and the defaults are not always false.
const TriState = ({
	label,
	value,
	onChange,
}: {
	label: string;
	value: boolean | undefined;
	onChange: (next: boolean | undefined) => void;
}) => (
	<select
		className="form-select form-select-sm"
		value={value === undefined ? 'unset' : String(value)}
		onChange={(event) => onChange(event.target.value === 'unset' ? undefined : event.target.value === 'true')}
		aria-label={label}
	>
		<option value="unset">Lattice decides</option>
		<option value="true">Yes</option>
		<option value="false">No</option>
	</select>
);

type Edit = (path: (string | number)[], value: unknown) => void;

const listEdit = (onEdit: Edit, key: string) => (next: string[]) => onEdit([key], next.length > 0 ? next : undefined);

const TaskEntry = ({ name, task, onEdit, onRemove }: { name: string; task: PipelineTask; onEdit: Edit; onRemove: () => void }) => (
	<div className="entry">
		<div className="entry__head">
			<span className="entry__title">{name}</span>
			<IconButton className="ms-auto" icon="bi-x" label={`Stop configuring ${name}`} onClick={onRemove} />
		</div>
		<div className="entry__grid">
			<Field label="Keeps running" jsonKey="persistent" hint="A dev server or watcher. Never cached, and nothing can depend on it.">
				<TriState label={`${name} keeps running`} value={task.persistent} onChange={(next) => onEdit(['persistent'], next)} />
			</Field>
			<Field label="Cached" jsonKey="cache" hint="A cache hit restores the stored outputs instead of running the command.">
				<TriState label={`${name} is cached`} value={task.cache} onChange={(next) => onEdit(['cache'], next)} />
			</Field>
		</div>
		<Field label="Runs after" jsonKey="dependsOn" hint="Tasks that finish first. ^build means the build of every workspace this one depends on.">
			<StringList label="dependsOn" values={task.dependsOn ?? []} placeholder="^build" onChange={listEdit(onEdit, 'dependsOn')} />
		</Field>
		<Field label="Inputs" jsonKey="inputs" hint="Files the task reads, relative to each workspace. With none, every file git tracks there counts.">
			<StringList label="inputs" values={task.inputs ?? []} placeholder="src/**" onChange={listEdit(onEdit, 'inputs')} />
		</Field>
		<Field label="Outputs" jsonKey="outputs" hint="Files stored in the cache and restored on a hit. Replaces the default for each workspace's tool.">
			<StringList label="outputs" values={task.outputs ?? []} placeholder="dist/**" onChange={listEdit(onEdit, 'outputs')} />
		</Field>
	</div>
);

const OverrideEntry = ({
	id,
	override,
	drivers,
	workspaces,
	onEdit,
	onRemove,
}: {
	id: string;
	override: WorkspaceOverride;
	drivers: string[];
	workspaces: string[];
	onEdit: Edit;
	onRemove: () => void;
}) => {
	const tasks = Object.entries(override.tasks ?? {});
	const facets = overrideFacets(override);
	return (
		<div className="entry">
			<div className="entry__head">
				<span className="entry__title">{id}</span>
				<span className="entry__facets">
					{isPathKey(id) ? 'matches paths' : 'a workspace'}
					{facets.length > 0 ? `, ${facets.join(', ')}` : ''}
				</span>
				<IconButton className="ms-auto" icon="bi-x" label={`Remove the override for ${id}`} onClick={onRemove} />
			</div>
			<div className="entry__grid">
				<Field label="Name" jsonKey="name" hint="The name Lattice uses for the workspace, in place of the one its manifest gives it.">
					<input
						className="form-control form-control-sm"
						value={override.name ?? ''}
						placeholder="from its manifest"
						onChange={(event) => onEdit(['name'], event.target.value || undefined)}
					/>
				</Field>
				<Field label="Runs with" jsonKey="driver" hint="The tool that runs its tasks, when Lattice cannot tell from the files. None means every task is written out below.">
					<select
						className="form-select form-select-sm"
						value={override.driver ?? ''}
						onChange={(event) => onEdit(['driver'], event.target.value || undefined)}
					>
						<option value="">Lattice decides</option>
						<option value="none">None, commands below</option>
						{drivers.map((driver) => (
							<option key={driver} value={driver}>
								{driver}
							</option>
						))}
					</select>
				</Field>
			</div>
			<Field label="Also runs after" jsonKey="dependsOn" hint="Workspaces this one depends on that its manifest does not say, such as an edge between languages.">
				<StringList
					label={`${id} dependsOn`}
					values={override.dependsOn ?? []}
					placeholder={workspaces[0] ?? 'workspace'}
					onChange={listEdit(onEdit, 'dependsOn')}
				/>
			</Field>
			<Field label="Commands" jsonKey="tasks" hint="A task's command for these workspaces. A task written as an object is edited in lattice.json.">
				<div className="string-list">
					{tasks.map(([task, value]) => (
						<div className="string-list__row" key={task}>
							<span className="entry__title align-self-center" style={{ minWidth: '5rem' }}>
								{task}
							</span>
							{isDetailed(value) ? (
								<span className="small text-body-secondary align-self-center">
									{overrideCommand(value) ?? 'settings only'}, edited in lattice.json
								</span>
							) : (
								<input
									className="form-control form-control-sm"
									value={value}
									onChange={(event) => onEdit(['tasks', task], event.target.value)}
									aria-label={`${task} command`}
								/>
							)}
							<IconButton icon="bi-x" label={`Remove ${task}`} onClick={() => onEdit(['tasks', task], undefined)} />
						</div>
					))}
					<AddRow placeholder="build" label="Write a command" onAdd={(task) => onEdit(['tasks', task], '')} />
				</div>
			</Field>
		</div>
	);
};

export default ConfigView;
