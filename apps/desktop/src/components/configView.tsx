import { useEffect, useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import * as api from '../lib/api.ts';
import { insertInto, removeAt, setValue } from '../lib/configEdit.ts';
import type { ConfigDiagnostic, LatticeConfig } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';
import Spinner from './spinner.tsx';

type Tab = 'form' | 'json';

// The editor's source of truth is the text, never a parsed object.
//
// The Rust config types are deny_unknown_fields, so a key this build has never heard
// of is still valid input the user is entitled to keep. Every control here computes a
// minimal edit against the original string, which is what makes an unknown key, the
// key order, and the file's own formatting all survive a save.
//
// Every control is labelled in English and carries the key it writes beside it. The
// English is what makes the form usable by someone who has not read the schema; the
// key is what stops the form from being the place the schema goes to hide.
const ConfigView = () => {
	const { project, configText, diagnostics, setConfigText, saveConfig, busy, catalog } = useApp();
	const [tab, setTab] = useState<Tab>('form');
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

	return (
		<div className="app-main__scroll">
			<div className="app-main__inner" style={{ maxWidth: '56rem' }}>
				<div className="d-flex align-items-center gap-2 mb-3">
					<div className="command-tabs" role="group" aria-label="Editor">
						<button
							type="button"
							className={`command-tab${tab === 'form' ? ' command-tab--active' : ''}`}
							onClick={() => setTab('form')}
						>
							Form
						</button>
						<button
							type="button"
							className={`command-tab${tab === 'json' ? ' command-tab--active' : ''}`}
							onClick={() => setTab('json')}
						>
							JSON
						</button>
					</div>
					<span className="command-tab__hint">
						{dirty ? 'Unsaved changes' : 'Saved'}
					</span>
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
					<div className="notice notice--bad mb-3">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div className="flex-grow-1">
							{problems.map((problem, index) => (
								<div key={index} className="selectable">
									{problem.message}
									{problem.line !== null && <span className="chip ms-2">line {problem.line}</span>}
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
								className="code-card__body selectable w-100 border-0 bg-transparent"
								style={{ minHeight: '28rem', resize: 'vertical', color: 'var(--text)' }}
								spellCheck={false}
								value={text}
								onChange={(event) => edit(event.target.value)}
								aria-label="lattice.json source"
							/>
						</div>
					</div>
				) : parsed === null ? (
					<div className="notice">
						<i className="bi bi-braces" aria-hidden="true" />
						<div>
							lattice.json is not valid JSON, so the form cannot show it. Fix it in the JSON tab.
						</div>
					</div>
				) : (
					<>
						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Project</h2>
							<p className="step__hint mb-2">Settings for the whole project, not for one workspace.</p>
							<div className="row g-3">
								<div className="col-12 col-md-6">
									<Label
										htmlFor="lattice-version"
										text="Lattice version"
										jsonKey="latticeVersion"
									/>
									<input
										id="lattice-version"
										className="form-control form-control-sm"
										placeholder="any version"
										value={parsed.latticeVersion ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['latticeVersion'], event.target.value || undefined))
										}
									/>
									<div className="form-text">
										A Lattice that installed itself switches to this version. Any other install
										warns and runs anyway.
									</div>
								</div>
								<div className="col-12 col-md-6">
									<Label
										htmlFor="cache-dir"
										text="Cache directory"
										jsonKey="settings.cacheDir"
									/>
									<input
										id="cache-dir"
										className="form-control form-control-sm"
										placeholder=".lattice/cache"
										value={parsed.settings?.cacheDir ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['settings', 'cacheDir'], event.target.value || undefined))
										}
									/>
									<div className="form-text">
										A path inside the project. Leave it empty to use .lattice/cache.
									</div>
								</div>
								<div className="col-12 col-md-6">
									<Label
										htmlFor="max-cache"
										text="Cache size limit"
										jsonKey="settings.maxCacheSize"
									/>
									<input
										id="max-cache"
										className="form-control form-control-sm"
										placeholder="10GB"
										value={parsed.settings?.maxCacheSize ?? ''}
										onChange={(event) =>
											edit(
												setValue(text, ['settings', 'maxCacheSize'], event.target.value || undefined),
											)
										}
									/>
									<div className="form-text">
										After a run, Lattice deletes the least recently used entries to get back under
										this.
									</div>
								</div>
							</div>
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Engines</h2>
							<p className="step__hint mb-2">
								Tool versions this project pins. A run fails before any task starts if the tool on
								PATH does not match. To have Lattice install the tool instead, give the engine an
								installCmd in the JSON tab.
							</p>
							{pinned.length === 0 ? (
								<p className="form-text">
									Nothing is pinned, so every task uses the tools already on the machine.
								</p>
							) : (
								pinned.map(([name, spec]) => (
									<div className="scan-row" key={name}>
										<span className="chip">{name}</span>
										<input
											className="form-control form-control-sm"
											style={{ maxWidth: '12rem' }}
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
											<span className="scan-row__meta">Not one of the engines Lattice knows by name</span>
										)}
										<button
											type="button"
											className="icon-btn ms-auto"
											title={`Stop pinning ${name}`}
											aria-label={`Stop pinning ${name}`}
											onClick={() => edit(removeAt(text, ['engines', name]))}
										>
											<i className="bi bi-x" aria-hidden="true" />
										</button>
									</div>
								))
							)}
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Workspaces</h2>
							<p className="step__hint mb-2">
								Every directory with its own manifest. Lattice runs tasks in each one listed here.
							</p>
							{(parsed.workspaces ?? []).map((workspace, index) => {
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
													className="form-control form-control-sm"
													style={{ maxWidth: '12rem' }}
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
												<button
													type="button"
													className="icon-btn"
													title={`Remove ${workspace.name}`}
													aria-label={`Remove ${workspace.name}`}
													onClick={() => edit(removeAt(text, ['workspaces', index]))}
												>
													<i className="bi bi-x" aria-hidden="true" />
												</button>
											</div>
											<div className="form-check form-switch">
												<input
													className="form-check-input"
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
												<label className="form-check-label small" htmlFor={`auto-${index}`}>
													Driver detection <span className="form-key">auto</span>
												</label>
												<div className="form-text mt-0">
													Lattice picks the tool from the files in the directory. Turn it off to
													declare scripts and engines yourself.
												</div>
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
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Tasks</h2>
							<p className="step__hint mb-2">
								One name, such as build or test, that each workspace runs its own way.
							</p>
							{Object.entries(parsed.tasks ?? {}).map(([name, task]) => (
								<div className="card mb-2" key={name}>
									<div className="card-body p-3">
										<div className="d-flex align-items-start gap-3 mb-3 flex-wrap">
											<strong className="me-auto">{name}</strong>
											<TriState
												label="Persistent"
												jsonKey="persistent"
												hint="Runs until stopped. Never cached, and nothing can depend on it."
												value={task.persistent}
												onChange={(next) =>
													edit(setValue(text, ['tasks', name, 'persistent'], next))
												}
											/>
											<TriState
												label="Cache"
												jsonKey="cache"
												hint="A cache hit restores the stored outputs instead of running the command."
												value={task.cache}
												onChange={(next) => edit(setValue(text, ['tasks', name, 'cache'], next))}
											/>
										</div>
										<StringList
											label="Dependencies"
											jsonKey="dependsOn"
											hint="Tasks that must finish first. A ^ prefix means the same task in every workspace this one depends on."
											values={task.dependsOn ?? []}
											onChange={(next) =>
												edit(
													setValue(
														text,
														['tasks', name, 'dependsOn'],
														next.length > 0 ? next : undefined,
													),
												)
											}
										/>
										<StringList
											label="Inputs"
											jsonKey="inputs"
											hint="Files the task reads. A change to any of them misses the cache. With none listed, every file in the workspace counts."
											values={task.inputs ?? []}
											onChange={(next) =>
												edit(
													setValue(
														text,
														['tasks', name, 'inputs'],
														next.length > 0 ? next : undefined,
													),
												)
											}
										/>
										<StringList
											label="Outputs"
											jsonKey="outputs"
											hint="Files a successful run saves into the cache. A cache hit restores them."
											values={task.outputs ?? []}
											onChange={(next) =>
												edit(
													setValue(
														text,
														['tasks', name, 'outputs'],
														next.length > 0 ? next : undefined,
													),
												)
											}
										/>
									</div>
								</div>
							))}
						</section>
					</>
				)}
			</div>
		</div>
	);
};

const Label = ({ htmlFor, text, jsonKey }: { htmlFor: string; text: string; jsonKey: string }) => (
	<label className="form-label small mb-1 d-block" htmlFor={htmlFor}>
		{text} <span className="form-key">{jsonKey}</span>
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
	hint?: string;
	value: boolean | undefined;
	onChange: (next: boolean | undefined) => void;
}) => (
	<label className="small" style={{ maxWidth: '13rem' }}>
		<span className="d-block" style={{ color: 'var(--text-subtle)' }}>
			{label} <span className="form-key">{jsonKey}</span>
		</span>
		{hint && <span className="form-text d-block mt-0">{hint}</span>}
		<select
			className="form-select form-select-sm mt-1"
			style={{ width: '13rem' }}
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
	</label>
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
	hint?: string;
	values: string[];
	onChange: (next: string[]) => void;
}) => (
	<div className="mb-3">
		<div className="form-label small mb-1">
			{label} <span className="form-key">{jsonKey}</span>
		</div>
		{hint && <div className="form-text mt-0 mb-1">{hint}</div>}
		<div className="d-flex flex-wrap gap-1 align-items-center">
			{values.map((value, index) => (
				<span className="input-group input-group-sm" style={{ width: 'auto' }} key={index}>
					<input
						className="form-control form-control-sm"
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
			<button
				type="button"
				className="icon-btn"
				title={`Add to ${label}`}
				aria-label={`Add to ${label}`}
				onClick={() => onChange([...values, ''])}
			>
				<i className="bi bi-plus-lg" aria-hidden="true" />
			</button>
		</div>
	</div>
);

export default ConfigView;
