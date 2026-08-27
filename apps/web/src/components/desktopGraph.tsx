import { useEffect, useMemo, useState } from 'react';

import GraphLegend from '../../../desktop/src/components/graphLegend.tsx';
import { useEcharts } from '../../../desktop/src/hooks/useEcharts.ts';
import { useReducedMotion } from '../../../desktop/src/hooks/useReducedMotion.ts';
import { useTaskViewsRev } from '../../../desktop/src/hooks/useRunStore.ts';
import { useThemeTokens } from '../../../desktop/src/hooks/useThemeTokens.ts';
import { closure } from '../../../desktop/src/lib/graphLayout.ts';
import { buildGraphOption, type NodeState } from '../../../desktop/src/lib/graphOption.ts';
import { runStore } from '../../../desktop/src/lib/runStore.ts';
import { graph } from '../lib/desktopDemo.ts';

// The app's GraphView itself cannot be imported: it fetches its dump over Tauri and
// reads the open project from context. Everything below that fetch can be, and is —
// the layered layout, the ECharts option, every node encoding, and the legend. This
// file supplies the dump, which here is the same fixture the hero's run plays.

// The series fits node bounds to the box and a label is not part of a node's bounds,
// so at any width the deepest layer's label hangs off the right edge. Reserving pixels
// is the right unit: a label does not scale with the zoom.
const LABEL_ROOM = 210;

const DesktopGraph = () => {
	const tokens = useThemeTokens();
	const reducedMotion = useReducedMotion();
	const viewsRev = useTaskViewsRev();
	const [focused, setFocused] = useState<string | null>(null);
	const [box, setBox] = useState<HTMLDivElement | null>(null);
	const [boxWidth, setBoxWidth] = useState(0);

	useEffect(() => {
		if (!box || typeof ResizeObserver !== 'function') return;
		const observer = new ResizeObserver(([entry]) => setBoxWidth(entry.contentRect.width));
		observer.observe(box);
		return () => observer.disconnect();
	}, [box]);

	// One subscription to "some task changed" rather than one per node, as the app does
	// it, so the picture fills in as the run in the hero above moves through it.
	const states = useMemo(() => {
		const map = new Map<string, NodeState>();
		for (const node of graph.nodes) {
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
	}, [viewsRev]);

	const focus = useMemo(() => (focused ? closure(graph, focused) : undefined), [focused]);

	const option = useMemo(() => {
		const built = buildGraphOption({ dump: graph, states, tokens, focus, reducedMotion });
		if (boxWidth === 0) return built;

		const [series] = built.series as Record<string, unknown>[];
		return { ...built, series: [{ ...series, zoom: Math.max(0.5, (boxWidth - LABEL_ROOM) / boxWidth) }] };
	}, [states, tokens, focus, reducedMotion, boxWidth]);

	const { hostRef, chart, ready } = useEcharts(option);

	// The copy beside this says clicking a task isolates it, so it has to. Same closure
	// the app focuses with.
	useEffect(() => {
		const instance = chart.current;
		if (!instance || !ready) return;
		const onClick = (params: unknown) => {
			const event = params as { dataType?: string; data?: { id?: unknown } | null };
			const id = typeof event.data?.id === 'string' ? event.data.id : null;
			if (event.dataType === 'node' && id) {
				setFocused((current) => (current === id ? null : id));
			}
		};
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
	}, [chart, ready]);

	return (
		<div className="d-flex flex-column gap-3">
			{/* The app's frame is a window's worth of height; in a page column that
			    leaves a four-layer graph stranded in empty space. */}
			<div className="border rounded-3 bg-body tw:h-[clamp(19rem,42vh,28rem)]" ref={setBox}>
				<div ref={hostRef} className="w-100 h-100" />
			</div>
			<GraphLegend />
		</div>
	);
};

export default DesktopGraph;
