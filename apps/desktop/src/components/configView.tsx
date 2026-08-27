import { useEffect, useMemo, useState, type ReactNode } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { insertInto, removeAt, setValue } from '../lib/configEdit.ts';
import type { ConfigDiagnostic, LatticeConfig } from '../lib/types.ts';
import FieldHint from './fieldHint.tsx';
import IconButton from './iconButton.tsx';
import LanguageMark from './languageMark.tsx';
import Spinner from './spinner.tsx';

type Tab = 'form' | 'json';
type Section = 'project' | 'engines' | 'workspaces' | 'tasks';

// The editor's source of truth is the text, never a parsed object.
//
// The Rust config types are deny_unknown_fields, so a key this build has never heard
// of is still valid input the user is entitled to keep. Every control here computes a
// minimal edit against the original string, which is what makes an unknown key, the
// key order, and the file's own formatting all survive a save.
//
// Every control is labelled in English and carries the key it writes beside it. The
// English is what makes the form usable by someone who has not read the schema; the
// key is what stops the form from being the place the schema goes to hide. The
// sentences explaining each one are a click away rather than on the page: four
// sections of labelled inputs is a form, and the same four with a paragraph under
// every input is a document.
const ConfigView = () => {
	const { project, configText, diagnostics, setConfigText, saveConfig, busy, catalog } = useApp();
	const [tab, setTab] = useState<Tab>('form');
	const [open, setOpen] = useState<Section>('project');
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

	// Validation is the backend's, debounced: it is the same parse the CLI does, so
	// the editor never disagrees with what a run would say.
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

	const edit = (next: string) => {
		setConfigText(next);
		setDirty(true);
	};

	const problems = dirty ? live : diagnostics;

	if (!project || configText === null) return null;

	const engineNames = catalog?.engines.map((engine) => engine.name) ?? [];
	const pinned = Object.entries(parsed?.engines ?? {});
	const workspaces = parsed?.workspaces ?? [];
	const tasks = Object.entries(parsed?.tasks ?? {});

	return (
		<div className="app-main__scroll">
			<div className="app-main__inner tw:max-w-[56rem]">
				<div className="d-flex align-items-center gap-2 mb-3">
					<div className="btn-group btn-group-sm" role="group" aria-label="Editor">
						<button
							type="button"
							className={`btn btn-outline-secondary${tab === 'form' ? ' active' : ''}`}
							onClick={() => setTab('form')}
							aria-pressed={tab === 'form'}
						>
							Form
						</button>
						<button
							type="button"
							className={`btn btn-outline-secondary${tab === 'json' ? ' active' : ''}`}
							onClick={() => setTab('json')}
							aria-pressed={tab === 'json'}
						>
							JSON
						</button>
					</div>
					<span className="small text-body-secondary">{dirty ? 'Unsaved changes' : 'Saved'}</span>
					<button
						type="button"
						className="btn btn-primary btn-sm ms-auto px-3 py-2 d-inline-flex align-items-center gap-2"
						disabled={busy || !dirty || problems.length > 0}
						onClick={() =>
							void saveConfig(text).then((saved) => {
								// A save that failed resolves too: the error goes to the
								// banner rather than to this promise.
								if (!saved) return;
								setDirty(false);
								setLive([]);
							})
						}
					>
						{busy && <Spinner label="Saving" quiet />}
						{busy ? 'Saving…' : 'Save'}
					</button>
				</div>

				{problems.length > 0 && (
					<div className="alert alert-danger d-flex align-items-start gap-2 mb-3" role="alert">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div className="flex-grow-1">
							{problems.map((problem, index) => (
								<div key={index} className="selectable">
									{problem.message}
									{problem.line !== null && (
										<span className="badge border bg-body-tertiary text-body-secondary fw-normal ms-2">
											line {problem.line}
										</span>
									)}
								</div>
							))}
						</div>
					</div>
				)}

				{tab === 'json' ? (
					<div className="card code-card">
						<div className="card-header code-card__bar">
							<span className="code-card__dot" />
							<span className="code-card__dot" />
							<span className="code-card__dot" />
							<span className="code-card__name">lattice.json</span>
						</div>
						<div className="card-body p-0">
							<textarea
								className="code-card__body selectable w-100 border-0 bg-transparent text-body tw:min-h-[28rem] tw:resize-y"
								spellCheck={false}
								value={text}
								onChange={(event) => edit(event.target.value)}
								aria-label="lattice.json source"
							/>
						</div>
					</div>
				) : parsed === null ? (
					<div className="alert alert-secondary d-flex align-items-start gap-2" role="alert">
						<i className="bi bi-braces" aria-hidden="true" />
						<div>
							lattice.json is not valid JSON, so the form cannot show it. Fix it in the JSON tab.
						</div>
					</div>
				) : (
					<div className="accordion">
						<Panel
							id="project"
							title="Project"
							open={open}
							onOpen={setOpen}
							summary="Settings for the whole project"
						>
							<div className="row g-3">
								<div className="col-12 col-md-6">
									<Label htmlFor="lattice-version" text="Lattice version" jsonKey="latticeVersion">
										A Lattice that installed itself switches to this version. Any other install
										warns and runs anyway.
									</Label>
									<input
										id="lattice-version"
										className="form-control form-control-sm"
										placeholder="any version"
										value={parsed.latticeVersion ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['latticeVersion'], event.target.value || undefined))
										}
									/>
								</div>
								<div className="col-12 col-md-6">
									<Label htmlFor="cache-dir" text="Cache directory" jsonKey="settings.cacheDir">
										A path inside the project. Leave it empty to use .lattice/cache.
									</Label>
									<input
										id="cache-dir"
										className="form-control form-control-sm"
										placeholder=".lattice/cache"
										value={parsed.settings?.cacheDir ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['settings', 'cacheDir'], event.target.value || undefined))
										}
									/>
								</div>
								<div className="col-12 col-md-6">
									<Label htmlFor="max-cache" text="Cache size limit" jsonKey="settings.maxCacheSize">
										After a run, Lattice deletes the least recently used entries to get back under
										this.
									</Label>
									<input
										id="max-cache"
										className="form-control form-control-sm"
										placeholder="10GB"
										value={parsed.settings?.maxCacheSize ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['settings', 'maxCacheSize'], event.target.value || undefined))
										}
									/>
								</div>
							</div>
						</Panel>

						<Panel
							id="engines"
							title="Engines"
							count={pinned.length}
							open={open}
							onOpen={setOpen}
							summary="Tool versions this project pins"
						>
							<p className="small text-body-secondary">
								A run fails before any task starts if the tool on PATH does not match. To have
								Lattice install the tool instead, give the engine an installCmd in the JSON tab.
							</p>
							{pinned.length === 0 ? (
								<p className="small text-body-secondary mb-0">
									Nothing is pinned, so every task uses the tools already on the machine.
								</p>
							) : (
								<ul className="list-group list-group-flush">
									{pinned.map(([name, spec]) => (
										<li className="list-group-item d-flex align-items-center gap-2 px-0" key={name}>
											<span className="badge border bg-body-tertiary text-body-secondary fw-normal font-monospace">
												{name}
											</span>
											<input
												className="form-control form-control-sm tw:max-w-[12rem]"
												value={typeof spec === 'string' ? spec : (spec.version ?? '')}
												onChange={(event) =>
													edit(
														setValue(
															text,
															typeof spec === 'string'
																? ['engines', name]
																: ['engines', name, 'version'],
															event.target.value,
														),
													)
												}
												aria-label={`${name} version`}
											/>
											{!engineNames.includes(name) && typeof spec === 'string' && (
												<span className="small text-body-secondary">
													Not one of the engines Lattice knows by name
												</span>
											)}
											<IconButton
												className="ms-auto"
												icon="bi-x"
												label={`Stop pinning ${name}`}
												onClick={() => edit(removeAt(text, ['engines', name]))}
											/>
										</li>
									))}
								</ul>
							)}
						</Panel>

						<Panel
							id="workspaces"
							title="Workspaces"
							count={workspaces.length}
							open={open}
							onOpen={setOpen}
							summary="Every directory with its own manifest"
						>
							{workspaces.map((workspace, index) => {
								const resolved = project.workspaces.find(
									(candidate) => candidate.name === workspace.name,
								);
								return (
									<div className="card mb-2" key={`${workspace.name}-${index}`}>
										<div className="card-body p-3">
											<div className="d-flex align-items-center gap-2 mb-2">
												<LanguageMark
													tool={resolved?.driver?.tool ?? null}
													language={resolved?.driver?.language ?? null}
												/>
												<input
													className="form-control form-control-sm tw:max-w-[12rem]"
													placeholder="Workspace name"
													value={workspace.name}
													onChange={(event) =>
														edit(setValue(text, ['workspaces', index, 'name'], event.target.value))
													}
													aria-label={`Workspace ${index + 1} name`}
												/>
												<input
													className="form-control form-control-sm"
													placeholder="Path from the project root"
													value={workspace.path}
													onChange={(event) =>
														edit(setValue(text, ['workspaces', index, 'path'], event.target.value))
													}
													aria-label={`Workspace ${index + 1} path`}
												/>
												<IconButton
													icon="bi-x"
													label={`Remove ${workspace.name}`}
													onClick={() => edit(removeAt(text, ['workspaces', index]))}
												/>
											</div>
											<div className="form-check form-switch d-flex align-items-center gap-2 mb-0">
												<input
													className="form-check-input mt-0"
													type="checkbox"
													id={`auto-${index}`}
													checked={workspace.auto !== false}
													onChange={(event) =>
														edit(
															setValue(
																text,
																['workspaces', index, 'auto'],
																event.target.checked ? undefined : false,
															),
														)
													}
												/>
												<label className="form-check-label small mb-0" htmlFor={`auto-${index}`}>
													Driver detection <JsonKey>auto</JsonKey>
												</label>
												<FieldHint label="driver detection">
													Lattice picks the tool from the files in the directory. Turn it off to
													declare scripts and engines yourself.
												</FieldHint>
											</div>
										</div>
									</div>
								);
							})}
							<button
								type="button"
								className="btn btn-outline-secondary btn-sm"
								onClick={() =>
									edit(insertInto(text, ['workspaces'], { name: 'new', path: 'path/to/dir' }))
								}
							>
								<i className="bi bi-plus-lg me-1" aria-hidden="true" />
								Add a workspace
							</button>
						</Panel>

						<Panel
							id="tasks"
							title="Tasks"
							count={tasks.length}
							open={open}
							onOpen={setOpen}
							summary="One name each workspace runs its own way"
						>
							{tasks.map(([name, task]) => (
								<div className="card mb-2" key={name}>
									<div className="card-body p-3">
										<div className="row g-3 align-items-start mb-2">
											<div className="col-12">
												<strong>{name}</strong>
											</div>
											<div className="col-6 col-md-4">
												<TriState
													label="Persistent"
													jsonKey="persistent"
													hint="Runs until stopped. Never cached, and nothing can depend on it."
													value={task.persistent}
													onChange={(next) => edit(setValue(text, ['tasks', name, 'persistent'], next))}
												/>
											</div>
											<div className="col-6 col-md-4">
												<TriState
													label="Cache"
													jsonKey="cache"
													hint="A cache hit restores the stored outputs instead of running the command."
													value={task.cache}
													onChange={(next) => edit(setValue(text, ['tasks', name, 'cache'], next))}
												/>
											</div>
										</div>
										<StringList
											label="Dependencies"
											jsonKey="dependsOn"
											hint="Tasks that must finish first. A ^ prefix means the same task in every workspace this one depends on."
											values={task.dependsOn ?? []}
											onChange={(next) =>
												edit(
													setValue(text, ['tasks', name, 'dependsOn'], next.length > 0 ? next : undefined),
												)
											}
										/>
										<StringList
											label="Inputs"
											jsonKey="inputs"
											hint="Files the task reads. A change to any of them misses the cache. With none listed, every file in the workspace counts."
											values={task.inputs ?? []}
											onChange={(next) =>
												edit(setValue(text, ['tasks', name, 'inputs'], next.length > 0 ? next : undefined))
											}
										/>
										<StringList
											label="Outputs"
											jsonKey="outputs"
											hint="Files a successful run saves into the cache. A cache hit restores them."
											values={task.outputs ?? []}
											onChange={(next) =>
												edit(setValue(text, ['tasks', name, 'outputs'], next.length > 0 ? next : undefined))
											}
										/>
									</div>
								</div>
							))}
						</Panel>
					</div>
				)}
			</div>
		</div>
	);
};

// Bootstrap's accordion markup, opened by React rather than by Bootstrap's collapse
// plugin — the same trade the theme and project menus already make.
const Panel = ({
	id,
	title,
	count,
	summary,
	open,
	onOpen,
	children,
}: {
	id: Section;
	title: string;
	count?: number;
	summary: string;
	open: Section;
	onOpen: (next: Section) => void;
	children: ReactNode;
}) => {
	const expanded = open === id;
	return (
		<div className="accordion-item">
			<h2 className="accordion-header">
				<button
					type="button"
					className={`accordion-button${expanded ? '' : ' collapsed'}`}
					onClick={() => onOpen(id)}
					aria-expanded={expanded}
					aria-controls={`panel-${id}`}
				>
					<span className="fw-bold me-2">{title}</span>
					{count !== undefined && (
						<span className="badge border bg-body-tertiary text-body-secondary fw-normal me-2">
							{count}
						</span>
					)}
					<span className="small text-body-secondary">{summary}</span>
				</button>
			</h2>
			<div id={`panel-${id}`} className={`accordion-collapse collapse${expanded ? ' show' : ''}`}>
				<div className="accordion-body">{children}</div>
			</div>
		</div>
	);
};

/** The `lattice.json` key a control writes, shown beside its plain-English label. */
const JsonKey = ({ children }: { children: string }) => (
	<span className="font-monospace text-body-secondary tw:text-[0.7rem]">{children}</span>
);

const Label = ({
	htmlFor,
	text,
	jsonKey,
	children,
}: {
	htmlFor: string;
	text: string;
	jsonKey: string;
	children: string;
}) => (
	<label className="form-label small mb-1 d-flex align-items-center gap-2" htmlFor={htmlFor}>
		<span>
			{text} <JsonKey>{jsonKey}</JsonKey>
		</span>
		<FieldHint label={text}>{children}</FieldHint>
	</label>
);

// `Option<bool>` must not collapse to a switch: writing `false` where the file said
// nothing is a different config, and the engine's defaults are not always false. The
// third choice is therefore offered by name rather than by leaving a switch sitting in
// a position that looks like "no".
const TriState = ({
	label,
	jsonKey,
	hint,
	value,
	onChange,
}: {
	label: string;
	jsonKey: string;
	hint: string;
	value: boolean | undefined;
	onChange: (next: boolean | undefined) => void;
}) => (
	<>
		<span className="form-label small mb-1 d-flex align-items-center gap-2">
			<span>
				{label} <JsonKey>{jsonKey}</JsonKey>
			</span>
			<FieldHint label={label}>{hint}</FieldHint>
		</span>
		<select
			className="form-select form-select-sm"
			value={value === undefined ? 'unset' : String(value)}
			onChange={(event) =>
				onChange(event.target.value === 'unset' ? undefined : event.target.value === 'true')
			}
			aria-label={label}
		>
			<option value="unset">Not set</option>
			<option value="true">Yes</option>
			<option value="false">No</option>
		</select>
	</>
);

const StringList = ({
	label,
	jsonKey,
	hint,
	values,
	onChange,
}: {
	label: string;
	jsonKey: string;
	hint: string;
	values: string[];
	onChange: (next: string[]) => void;
}) => (
	<div className="mb-3">
		<span className="form-label small mb-1 d-flex align-items-center gap-2">
			<span>
				{label} <JsonKey>{jsonKey}</JsonKey>
			</span>
			<FieldHint label={label}>{hint}</FieldHint>
		</span>
		<div className="d-flex flex-wrap gap-1 align-items-center">
			{values.map((value, index) => (
				<span className="input-group input-group-sm w-auto" key={index}>
					<input
						className="form-control form-control-sm font-monospace"
						style={{ width: `${Math.max(6, value.length + 2)}ch` }}
						value={value}
						onChange={(event) => {
							const next = [...values];
							next[index] = event.target.value;
							onChange(next);
						}}
						aria-label={`${label}, entry ${index + 1}`}
					/>
					<button
						type="button"
						className="btn btn-outline-secondary"
						aria-label={`Remove entry ${index + 1} from ${label}`}
						onClick={() => onChange(values.filter((_, at) => at !== index))}
					>
						<i className="bi bi-x" aria-hidden="true" />
					</button>
				</span>
			))}
			<IconButton icon="bi-plus-lg" label={`Add to ${label}`} onClick={() => onChange([...values, ''])} />
		</div>
	</div>
);

export default ConfigView;
