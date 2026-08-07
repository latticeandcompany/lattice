import { useEffect, useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useIsDark } from '../hooks/useThemeTokens.ts';
import * as api from '../lib/api.ts';
import { insertInto, removeAt, setValue } from '../lib/configEdit.ts';
import type { ConfigDiagnostic, LatticeConfig } from '../lib/types.ts';
import LanguageMark from './languageMark.tsx';

type Tab = 'form' | 'json';

// The editor's source of truth is the text, never a parsed object.
//
// The Rust config types are deny_unknown_fields, so a key this build has never heard
// of is still valid input the user is entitled to keep. Every control here computes a
// minimal edit against the original string, which is what makes an unknown key, the
// key order, and the file's own formatting all survive a save.
const ConfigView = () => {
	const { project, configText, diagnostics, setConfigText, saveConfig, busy, catalog } = useApp();
	const dark = useIsDark();
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
						className="btn btn-contrast btn-sm ms-auto px-3 py-2"
						disabled={busy || !dirty || problems.length > 0}
						onClick={() =>
							void saveConfig(text).then(() => {
								setDirty(false);
								setLive([]);
							})
						}
					>
						Save
					</button>
				</div>

				{problems.length > 0 && (
					<div className="notice notice--bad mb-3">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div className="flex-grow-1">
							{problems.map((problem, index) => (
								<div key={index} className="selectable">
									{problem.message}
									{problem.line !== null && (
										<span className="chip ms-2">line {problem.line}</span>
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
							The file is not valid JSON right now, so the form cannot show it. Fix it in the
							JSON tab.
						</div>
					</div>
				) : (
					<>
						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Repo</h2>
							<div className="row g-3">
								<div className="col-12 col-md-6">
									<label className="form-label small" htmlFor="lattice-version">
										latticeVersion
									</label>
									<input
										id="lattice-version"
										className="form-control form-control-sm"
										value={parsed.latticeVersion ?? ''}
										onChange={(event) =>
											edit(setValue(text, ['latticeVersion'], event.target.value || undefined))
										}
									/>
								</div>
								<div className="col-12 col-md-6">
									<label className="form-label small" htmlFor="cache-dir">
										settings.cacheDir
									</label>
									<input
										id="cache-dir"
										className="form-control form-control-sm"
										placeholder=".lattice/cache"
										value={parsed.settings?.cacheDir ?? ''}
										onChange={(event) =>
											edit(
												setValue(
													text,
													['settings', 'cacheDir'],
													event.target.value || undefined,
												),
											)
										}
									/>
								</div>
								<div className="col-12 col-md-6">
									<label className="form-label small" htmlFor="max-cache">
										settings.maxCacheSize
									</label>
									<input
										id="max-cache"
										className="form-control form-control-sm"
										placeholder="10GB"
										value={parsed.settings?.maxCacheSize ?? ''}
										onChange={(event) =>
											edit(
												setValue(
													text,
													['settings', 'maxCacheSize'],
													event.target.value || undefined,
												),
											)
										}
									/>
								</div>
							</div>
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Engines</h2>
							<p className="command-tab__hint text-start mb-2">
								A name outside the known set has to give its own versionCmd, which the JSON
								tab can express.
							</p>
							{Object.entries(parsed.engines ?? {}).map(([name, spec]) => (
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
										aria-label={`Version constraint for ${name}`}
									/>
									{!engineNames.includes(name) && typeof spec === 'string' && (
										<span className="scan-row__meta">not a known engine name</span>
									)}
									<button
										type="button"
										className="icon-btn ms-auto"
										title={`Remove ${name}`}
										aria-label={`Remove ${name}`}
										onClick={() => edit(removeAt(text, ['engines', name]))}
									>
										<i className="bi bi-x" aria-hidden="true" />
									</button>
								</div>
							))}
						</section>

						<section className="mb-4">
							<h2 className="h5 fw-bold mb-3">Workspaces</h2>
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
													dark={dark}
												/>
												<input
													className="form-control form-control-sm"
													style={{ maxWidth: '12rem' }}
													value={workspace.name}
													onChange={(event) =>
														edit(
															setValue(
																text,
																['workspaces', index, 'name'],
																event.target.value,
															),
														)
													}
													aria-label={`Name of workspace ${index + 1}`}
												/>
												<input
													className="form-control form-control-sm"
													value={workspace.path}
													onChange={(event) =>
														edit(
															setValue(
																text,
																['workspaces', index, 'path'],
																event.target.value,
															),
														)
													}
													aria-label={`Path of workspace ${index + 1}`}
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
													Detect the driver from what is in the directory
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
							{Object.entries(parsed.tasks ?? {}).map(([name, task]) => (
								<div className="card mb-2" key={name}>
									<div className="card-body p-3">
										<div className="d-flex align-items-center gap-2 mb-2">
											<strong className="me-auto">{name}</strong>
											<TriState
												label="persistent"
												value={task.persistent}
												onChange={(next) =>
													edit(setValue(text, ['tasks', name, 'persistent'], next))
												}
											/>
											<TriState
												label="cache"
												value={task.cache}
												onChange={(next) => edit(setValue(text, ['tasks', name, 'cache'], next))}
											/>
										</div>
										<StringList
											label="dependsOn"
											hint="A ^ prefix means the same task in each dependency."
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
											label="inputs"
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
											label="outputs"
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

// `Option<bool>` must not collapse to a switch: writing `false` where the file said
// nothing is a different config, and the engine's defaults are not always false.
const TriState = ({
	label,
	value,
	onChange,
}: {
	label: string;
	value: boolean | undefined;
	onChange: (next: boolean | undefined) => void;
}) => (
	<label className="d-inline-flex align-items-center gap-1 small">
		<span style={{ color: 'var(--text-muted)' }}>{label}</span>
		<select
			className="form-select form-select-sm"
			style={{ width: '6.5rem' }}
			value={value === undefined ? 'unset' : String(value)}
			onChange={(event) =>
				onChange(event.target.value === 'unset' ? undefined : event.target.value === 'true')
			}
			aria-label={label}
		>
			<option value="unset">unset</option>
			<option value="true">true</option>
			<option value="false">false</option>
		</select>
	</label>
);

const StringList = ({
	label,
	hint,
	values,
	onChange,
}: {
	label: string;
	hint?: string;
	values: string[];
	onChange: (next: string[]) => void;
}) => (
	<div className="mb-2">
		<label className="form-label small mb-1">{label}</label>
		{hint && <div className="scan-row__meta mb-1">{hint}</div>}
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
						aria-label={`${label} entry ${index + 1}`}
					/>
					<button
						type="button"
						className="btn btn-outline-secondary"
						aria-label={`Remove ${label} entry ${index + 1}`}
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
