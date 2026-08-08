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
						{dirty ? 'Not saved yet' : 'Everything is saved'}
					</span>
					<button
						type="button"
						className="btn btn-primary btn-sm ms-auto px-3 py-2 d-inline-flex align-items-center gap-2"
						disabled={busy || !dirty || problems.length > 0}
						onClick={() =>
							void saveConfig(text).then(() => {
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
							The file is not valid JSON right now, so the form cannot show it. Fix it in the JSON
							tab.
						</div>
					</div>
				) : (
					<>
						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">This repo</h2>
							<div className="row g-3">
								<div className="col-12 col-md-6">
									<Label
										htmlFor="lattice-version"
										text="Version of Lattice to expect"
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
										Anyone on an older Lattice is told to upgrade, instead of hitting a confusing
										error later on.
									</div>
								</div>
								<div className="col-12 col-md-6">
									<Label
										htmlFor="cache-dir"
										text="Where results are kept"
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
										A folder in this repo. Leave it empty to use the default.
									</div>
								</div>
								<div className="col-12 col-md-6">
									<Label
										htmlFor="max-cache"
										text="How much of the disk it may use"
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
										Past this, the results nobody has needed in the longest go first.
									</div>
								</div>
							</div>
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Tool versions</h2>
							<p className="step__hint mb-2">
								Pin a version here and everyone working in this repo — and CI — gets that one,
								rather than whatever happens to be on the machine. Lattice can fetch the tools it
								already knows; anything else has to be told how to report its own version, which
								the JSON tab can do.
							</p>
							{pinned.length === 0 ? (
								<p className="form-text">
									Nothing is pinned, so every tool comes from the machine running it.
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
											aria-label={`Which version of ${name} to use`}
										/>
										{!engineNames.includes(name) && typeof spec === 'string' && (
											<span className="scan-row__meta">Lattice does not know this tool yet</span>
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
								A folder Lattice can run something in — usually one with a manifest of its own, like
								a package.json, a Cargo.toml, or a go.mod.
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
													placeholder="What to call it"
													value={workspace.name}
													onChange={(event) =>
														edit(setValue(text, ['workspaces', index, 'name'], event.target.value))
													}
													aria-label={`What to call workspace ${index + 1}`}
												/>
												<input
													className="form-control form-control-sm"
													placeholder="Folder, from the top of the repo"
													value={workspace.path}
													onChange={(event) =>
														edit(setValue(text, ['workspaces', index, 'path'], event.target.value))
													}
													aria-label={`Folder for workspace ${index + 1}`}
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
													Work out how to run things here from what is in the folder{' '}
													<span className="form-key">auto</span>
												</label>
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
								One name — build, test, lint — that every workspace runs its own way.
							</p>
							{Object.entries(parsed.tasks ?? {}).map(([name, task]) => (
								<div className="card mb-2" key={name}>
									<div className="card-body p-3">
										<div className="d-flex align-items-start gap-3 mb-3 flex-wrap">
											<strong className="me-auto">{name}</strong>
											<TriState
												label="Keeps running until stopped"
												jsonKey="persistent"
												value={task.persistent}
												onChange={(next) =>
													edit(setValue(text, ['tasks', name, 'persistent'], next))
												}
											/>
											<TriState
												label="Reuse the result when nothing changed"
												jsonKey="cache"
												value={task.cache}
												onChange={(next) => edit(setValue(text, ['tasks', name, 'cache'], next))}
											/>
										</div>
										<StringList
											label="Waits for"
											jsonKey="dependsOn"
											hint="Tasks that have to finish first. A ^ in front means the same task, in everything this workspace depends on."
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
											label="Files it reads"
											jsonKey="inputs"
											hint="Change one of these and the task runs again instead of coming back from the cache. Leave it empty and every tracked file in the workspace counts."
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
											label="Files it produces"
											jsonKey="outputs"
											hint="What gets saved, and what gets put back in place when the result is reused."
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
	value,
	onChange,
}: {
	label: string;
	jsonKey: string;
	value: boolean | undefined;
	onChange: (next: boolean | undefined) => void;
}) => (
	<label className="small">
		<span className="d-block" style={{ color: 'var(--text-subtle)' }}>
			{label} <span className="form-key">{jsonKey}</span>
		</span>
		<select
			className="form-select form-select-sm mt-1"
			style={{ width: '13rem' }}
			value={value === undefined ? 'unset' : String(value)}
			onChange={(event) =>
				onChange(event.target.value === 'unset' ? undefined : event.target.value === 'true')
			}
			aria-label={label}
		>
			<option value="unset">Leave it to Lattice</option>
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
