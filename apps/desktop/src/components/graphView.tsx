import { useEffect, useMemo, useState } from 'react';

import { useApp } from '../context/appContext.tsx';
import { useEcharts } from '../hooks/useEcharts.ts';
import { useReducedMotion } from '../hooks/useReducedMotion.ts';
import { useTaskViewsRev } from '../hooks/useRunStore.ts';
import { useThemeTokens } from '../hooks/useThemeTokens.ts';
import * as api from '../lib/api.ts';
import { closure, filterDump, layerCount } from '../lib/graphLayout.ts';
import { buildGraphOption, declaredEdges, type NodeState } from '../lib/graphOption.ts';
import { defaultSelection, effectiveSelection, taskNamesOf } from '../lib/runOptions.ts';
import { runStore } from '../lib/runStore.ts';
import type { GraphDump } from '../lib/types.ts';
import GraphLegend from './graphLegend.tsx';
import PageHead from './pageHead.tsx';

type Mode = 'graph' | 'list';

const GraphView = () => {
	const { project } = useApp();
	const tokens = useThemeTokens();
	const reducedMotion = useReducedMotion();
	const viewsRev = useTaskViewsRev();

	const taskNames = useMemo(() => taskNamesOf(project?.workspaces ?? []), [project]);
	const [picked, setPicked] = useState<string[]>(() => defaultSelection(taskNames));
	// Not the raw pick: the project may have been switched under it.
	const selected = useMemo(() => effectiveSelection(picked, taskNames), [picked, taskNames]);
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

	// Live state, so the picture fills in as the build happens. One subscription to
	// "some task changed" rather than one per node, and output lines do not move it.
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
	}, [filtered, viewsRev]);

	const focus = useMemo(
		() => (focused && dump ? closure(dump, focused) : undefined),
		[focused, dump],
	);

	const declared = useMemo(
		() => declaredEdges(filtered, project?.workspaces ?? []),
		[filtered, project],
	);

	const option = useMemo(
		() =>
			filtered.nodes.length === 0
				? null
				: buildGraphOption({ dump: filtered, states, tokens, focus, reducedMotion, declared }),
		[filtered, states, tokens, focus, reducedMotion, declared],
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
		// The chart's own click never fires for empty canvas, so clearing the focus has
		// to come off zrender — and has to be taken back off it, since the instance
		// outlives this effect.
		const onBackground = (event: { target?: unknown }) => {
			if (!event.target) setFocused(null);
		};
		const zr = instance.getZr();
		instance.on('click', onClick as never);
		zr.on('click', onBackground as never);
		return () => {
			instance.off('click', onClick as never);
			zr.off('click', onBackground as never);
		};
	}, [chart, ready, mode]);

	if (!project) return null;

	const toggle = (task: string, additive: boolean) =>
		setPicked(
			additive
				? selected.includes(task)
					? selected.filter((candidate) => candidate !== task)
					: [...selected, task]
				: [task],
		);

	return (
		<>
			<PageHead title="Graph">
				<span className="datum">
					{filtered.nodes.length} tasks, {layerCount(filtered)} deep
				</span>
			</PageHead>

			<div className="app-scroll">
				<div className="app-column d-flex flex-column gap-3">
					<div className="d-flex align-items-center flex-wrap gap-2">
						<div className="picker" role="group" aria-label="Tasks to show">
							{taskNames.slice(0, 8).map((task) => (
								<button
									key={task}
									type="button"
									className="picker__option"
									onClick={(event) => toggle(task, event.metaKey || event.ctrlKey)}
									aria-pressed={selected.includes(task)}
								>
									{task}
								</button>
							))}
						</div>

						<div className="picker" role="group" aria-label="Display">
							{(['graph', 'list'] as const).map((value) => (
								<button
									key={value}
									type="button"
									className="picker__option"
									onClick={() => setMode(value)}
									aria-pressed={mode === value}
								>
									{value === 'graph' ? 'Graph' : 'List'}
								</button>
							))}
						</div>

						<input
							type="search"
							className="form-control form-control-sm w-auto"
							placeholder="Find a task"
							aria-label="Find a task in the graph"
							value={search}
							onChange={(event) => setSearch(event.target.value)}
						/>

						{focused && (
							<button type="button" className="btn btn-outline-secondary btn-sm" onClick={() => setFocused(null)}>
								Show every task
							</button>
						)}
					</div>

					{error && <pre className="cli-error selectable">{error}</pre>}

					{filtered.nodes.length === 0 ? (
						<div className="empty-state">
							<h2>No task matches</h2>
							<p>Pick another task, or clear the search.</p>
						</div>
					) : mode === 'graph' ? (
						<>
							<div className="graph-frame">
								<div ref={hostRef} className="graph-host" />
							</div>
							<GraphLegend declared={declared.size > 0} />
							<p className="small text-body-secondary mb-0">
								Click a task to see only what it depends on and what depends on it. Drag to pan, scroll to
								zoom.
							</p>
						</>
					) : (
						// A canvas cannot be read by a screen reader or walked by a keyboard, so
						// the same data is here as a table.
						<table className="table table-sm align-middle">
							<caption className="visually-hidden">Tasks in dependency order, with what each one runs</caption>
							<thead>
								<tr>
									<th scope="col">Task</th>
									<th scope="col">Command</th>
									<th scope="col">Notes</th>
								</tr>
							</thead>
							<tbody>
								{filtered.nodes.map((node) => (
									<tr key={node.id}>
										<td className="font-monospace small">{node.id}</td>
										<td className="font-monospace small selectable">{node.command}</td>
										<td className="small">
											{[
												node.persistent ? 'keeps running' : '',
												node.pulledIn ? 'a dependency of what you picked' : '',
												states.get(node.id)?.state ?? '',
											]
												.filter(Boolean)
												.join(', ')}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					)}
				</div>
			</div>
		</>
	);
};

export default GraphView;
