import { useEffect, useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useEcharts } from '../hooks/useEcharts.ts';
import { useReducedMotion } from '../hooks/useReducedMotion.ts';
import { useRunView } from '../hooks/useRunStore.ts';
import { useThemeTokens } from '../hooks/useThemeTokens.ts';
import * as api from '../lib/api.ts';
import { closure, filterDump, layerCount } from '../lib/graphLayout.ts';
import { buildGraphOption, type NodeState } from '../lib/graphOption.ts';
import { runStore } from '../lib/runStore.ts';
import type { GraphDump } from '../lib/types.ts';
import GraphLegend from './graphLegend.tsx';

type Mode = 'graph' | 'list';

const GraphView = () => {
	const { project } = useApp();
	const tokens = useThemeTokens();
	const reducedMotion = useReducedMotion();
	const run = useRunView();

	const taskNames = project?.tasks.map((task) => task.name) ?? [];
	const [selected, setSelected] = useState<string[]>(
		taskNames.includes('build') ? ['build'] : taskNames.slice(0, 1),
	);
	const [search, setSearch] = useState('');
	const [mode, setMode] = useState<Mode>('graph');
	const [focused, setFocused] = useState<string | null>(null);
	const [dump, setDump] = useState<GraphDump | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		if (selected.length === 0) {
			setDump({ nodes: [], edges: [] });
			return;
		}
		let cancelled = false;
		void (async () => {
			try {
				const next = await api.graphDump({ tasks: selected, sequentially: false });
				if (!cancelled) {
					setDump(next);
					setError(null);
				}
			} catch (caught) {
				if (!cancelled) {
					setError(caught instanceof Error ? caught.message : String(caught));
					setDump({ nodes: [], edges: [] });
				}
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [selected, project?.root]);

	const filtered = useMemo(
		() => (dump ? filterDump(dump, { search }) : { nodes: [], edges: [] }),
		[dump, search],
	);

	// Live state, so the picture animates the build as it happens. Read from the store
	// keyed off the run's revision rather than subscribed per node.
	const states = useMemo(() => {
		const map = new Map<string, NodeState>();
		for (const node of filtered.nodes) {
			const view = runStore.taskView(node.id);
			if (view.state !== 'idle') {
				map.set(node.id, {
					state: view.state,
					durationMs: view.durationMs,
					cacheKey: view.cacheKey,
				});
			}
		}
		return map;
	}, [filtered, run.phase, run.result]);

	const focus = useMemo(
		() => (focused && dump ? closure(dump, focused) : undefined),
		[focused, dump],
	);

	const option = useMemo(
		() =>
			filtered.nodes.length === 0
				? null
				: buildGraphOption({ dump: filtered, states, tokens, focus, reducedMotion }),
		[filtered, states, tokens, focus, reducedMotion],
	);

	const { hostRef, chart, ready } = useEcharts(mode === 'graph' ? option : null);

	// Click focuses a node's closure; the background clears it.
	useEffect(() => {
		const instance = chart.current;
		if (!instance || !ready) return;
		// ECharts types the payload as its own union, so the id is narrowed here rather
		// than asserted at the boundary.
		const onClick = (params: unknown) => {
			const event = params as { dataType?: string; data?: { id?: unknown } | null };
			const id = typeof event.data?.id === 'string' ? event.data.id : null;
			if (event.dataType === 'node' && id) {
				setFocused((current) => (current === id ? null : id));
			}
		};
		const onBackground = () => setFocused(null);
		instance.on('click', onClick as never);
		instance.getZr().on('click', ((event: { target?: unknown }) => {
			if (!event.target) onBackground();
		}) as never);
		return () => {
			instance.off('click', onClick as never);
		};
	}, [chart, ready, mode]);

	if (!project) return null;

	return (
		<div className="app-main__scroll">
			<div className="app-main__inner graph-shell">
				<div className="run-bar__row">
					<div className="command-tabs" role="group" aria-label="Tasks to show">
						{taskNames.map((task) => (
							<button
								key={task}
								type="button"
								className={`command-tab${selected.includes(task) ? ' command-tab--active' : ''}`}
								onClick={(event) =>
									setSelected((current) =>
										event.metaKey || event.ctrlKey
											? current.includes(task)
												? current.filter((candidate) => candidate !== task)
												: [...current, task]
											: [task],
									)
								}
								aria-pressed={selected.includes(task)}
							>
								{task}
							</button>
						))}
					</div>

					<div className="command-tabs" role="group" aria-label="Display">
						<button
							type="button"
							className={`command-tab${mode === 'graph' ? ' command-tab--active' : ''}`}
							onClick={() => setMode('graph')}
						>
							Graph
						</button>
						<button
							type="button"
							className={`command-tab${mode === 'list' ? ' command-tab--active' : ''}`}
							onClick={() => setMode('list')}
						>
							List
						</button>
					</div>

					<div className="input-group input-group-sm" style={{ maxWidth: '15rem' }}>
						<span className="input-group-text">
							<i className="bi bi-search" aria-hidden="true" />
						</span>
						<input
							type="text"
							className="form-control"
							placeholder="Find a task"
							aria-label="Find a task in the graph"
							value={search}
							onChange={(event) => setSearch(event.target.value)}
						/>
					</div>

					{focused && (
						<button
							type="button"
							className="btn btn-outline-secondary btn-sm"
							onClick={() => setFocused(null)}
						>
							Clear focus
						</button>
					)}

					<span className="ms-auto run-bar__summary">
						{filtered.nodes.length} tasks · {layerCount(filtered)} layers deep
					</span>
				</div>

				{error && (
					<div className="notice notice--bad">
						<i className="bi bi-exclamation-triangle" aria-hidden="true" />
						<div className="selectable">{error}</div>
					</div>
				)}

				{filtered.nodes.length === 0 ? (
					<div className="empty-state">
						<i className="bi bi-diagram-3 fs-2" aria-hidden="true" />
						<div>No task matches what you picked.</div>
					</div>
				) : mode === 'graph' ? (
					<>
						<div className="graph-canvas">
							<div ref={hostRef} className="graph-canvas__host" />
						</div>
						<GraphLegend />
						<p className="command-tab__hint" style={{ textAlign: 'left' }}>
							Click a task to focus what it depends on and what depends on it. Drag to pan,
							scroll to zoom.
						</p>
					</>
				) : (
					// A canvas cannot be read by a screen reader or walked by a keyboard, so
					// the same data is here as a table rather than as aria on a <canvas>.
					<table className="graph-table">
						<caption className="visually-hidden">
							Tasks in dependency order, with what each one runs
						</caption>
						<thead>
							<tr>
								<th scope="col">#</th>
								<th scope="col">Task</th>
								<th scope="col">Command</th>
								<th scope="col">Notes</th>
							</tr>
						</thead>
						<tbody>
							{filtered.nodes.map((node, index) => (
								<tr key={node.id}>
									<td className="mono">{index + 1}</td>
									<td className="mono">{node.id}</td>
									<td className="mono selectable">{node.command}</td>
									<td>
										{[
											node.persistent ? 'persistent' : '',
											node.pulledIn ? 'pulled in as a dependency' : '',
											states.get(node.id)?.state ?? '',
										]
											.filter(Boolean)
											.join(' · ')}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				)}
			</div>
		</div>
	);
};

export default GraphView;
