// The ECharts option for a task graph.
//
// One pure function from (graph, task states, theme, focus) to an option, rebuilt
// whole every time and applied with notMerge. That is deliberate on two counts: a
// merged option would leave removed nodes and edges behind, and it keeps us clear of
// echarts#21200, where setTheme discards options a prior setOption applied. We never
// call setTheme — re-theming is just another rebuild.
//
// On encoding: the graph spends no hue it does not have to. A terminal label has only
// colour left to distinguish it, which is why the CLI has a palette; a node has
// position, size, symbol, fill, border weight, border style, and opacity. Those carry
// the meaning here, and the only accents are the focus ring and failure.

import { layout } from './graphLayout.ts';
import type { TaskState } from './taskStatus.ts';
import type { Tokens } from './themeTokens.ts';
import type { GraphDump } from './types.ts';

export interface NodeState {
	state: TaskState;
	durationMs?: number;
	cacheKey?: string;
}

export interface GraphOptionInput {
	dump: GraphDump;
	/** Task id to its live state, when a run is in flight. */
	states: ReadonlyMap<string, NodeState>;
	tokens: Tokens;
	/** The focused node's dependency closure, dimming everything else. */
	focus?: ReadonlySet<string>;
	reducedMotion: boolean;
}

const escapeHtml = (value: string): string =>
	value
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');

/** A node's fill, border and opacity for its state. */
export const nodeStyle = (
	state: TaskState | undefined,
	pulledIn: boolean,
	tokens: Tokens,
): { color: string; borderColor: string; borderWidth: number; borderType: 'solid' | 'dashed'; opacity: number } => {
	const base = {
		color: tokens.surface2,
		borderColor: tokens.border,
		borderWidth: 1,
		borderType: pulledIn ? ('dashed' as const) : ('solid' as const),
		opacity: 1,
	};

	switch (state) {
		case 'done':
			// Inverted: the most contrast goes to the thing that did the work.
			return { ...base, color: tokens.text, borderColor: tokens.text };
		case 'cached':
			// Recessive on purpose: this one did not do any work.
			return { ...base, opacity: 0.5 };
		case 'running':
			return { ...base, borderColor: tokens.text, borderWidth: 2 };
		case 'failed':
		case 'exited':
			return { ...base, borderColor: tokens.fail, borderWidth: 2 };
		case 'skipped':
			return { ...base, opacity: 0.4 };
		default:
			// `pulled_in` keeps the surface fill so it reads as "not what you asked
			// for", with the dashed border saying why.
			return pulledIn ? { ...base, color: tokens.surface } : base;
	}
};

export const buildGraphOption = (input: GraphOptionInput): Record<string, unknown> => {
	const { dump, states, tokens, focus, reducedMotion } = input;
	const points = layout(dump);

	const dim = (id: string) => (focus && !focus.has(id) ? 0.15 : 1);

	const nodes = dump.nodes.map((node) => {
		const point = points.get(node.id) ?? { x: 0, y: 0 };
		const live = states.get(node.id);
		const style = nodeStyle(live?.state, node.pulledIn, tokens);
		const focused = focus?.has(node.id) && focus.size > 0;

		return {
			id: node.id,
			name: node.id,
			x: point.x,
			y: point.y,
			// A dev server is a different kind of thing, so it is a different shape.
			symbol: node.persistent ? 'roundRect' : 'circle',
			symbolSize: node.persistent ? [18, 14] : 14,
			itemStyle: {
				color: style.color,
				borderColor: focused ? tokens.focus : style.borderColor,
				borderWidth: focused ? 2 : style.borderWidth,
				borderType: style.borderType,
				opacity: style.opacity * dim(node.id),
			},
			label: {
				show: true,
				position: 'right' as const,
				formatter: labelFor(node.id, live),
				color: tokens.textSubtle,
				fontFamily: 'DM Mono, ui-monospace, monospace',
				fontSize: 11,
				opacity: dim(node.id),
			},
		};
	});

	const edges = dump.edges.map((edge) => ({
		source: edge.from,
		target: edge.to,
		lineStyle: {
			color: tokens.border,
			width: 1,
			curveness: 0.12,
			// A cached prerequisite's edge recedes with it.
			opacity:
				Math.min(dim(edge.from), dim(edge.to)) *
				(states.get(edge.from)?.state === 'cached' ? 0.5 : 1),
			type: edge.crossWorkspace ? ('solid' as const) : ('dashed' as const),
		},
	}));

	return {
		backgroundColor: 'transparent',
		// The global default infers dark mode from backgroundColor, which a
		// transparent one makes impossible, so it has to be stated.
		darkMode: tokens.dark,
		animation: !reducedMotion,
		tooltip: {
			trigger: 'item',
			// The panel is drawn by the formatter's own markup, not by this container.
			// ECharts positions and shows the container whether or not the formatter
			// returned anything, so any surface put here appears as an empty rectangle
			// over whatever is under the pointer the moment you hover an edge or pan
			// the canvas. Styling the returned markup instead means an empty tooltip
			// is an empty box, which is nothing at all.
			className: 'graph-tooltip',
			backgroundColor: 'transparent',
			borderWidth: 0,
			padding: 0,
			extraCssText: 'box-shadow:none;',
			formatter: (params: { dataType?: string; data?: unknown }) =>
				params.dataType === 'node' ? tooltipHtml(params.data, dump, states) : '',
		},
		series: [
			{
				type: 'graph',
				layout: 'none',
				roam: true,
				draggable: false,
				// The default rescales labels on hover, which re-lays out the whole
				// label set every time the pointer moves.
				emphasis: { scale: false, focus: 'none' },
				edgeSymbol: ['none', 'arrow'],
				edgeSymbolSize: 7,
				labelLayout: { hideOverlap: true },
				data: nodes,
				links: edges,
			},
		],
	};
};

const labelFor = (id: string, live: NodeState | undefined): string => {
	if (!live) return id;
	switch (live.state) {
		case 'cached':
			return `${id} ·cache hit`;
		case 'failed':
			return `${id} ·failed`;
		case 'skipped':
			return `${id} ·skipped`;
		default:
			return id;
	}
};

export const tooltipHtml = (
	data: unknown,
	dump: GraphDump,
	states: ReadonlyMap<string, NodeState>,
): string => {
	const id = (data as { id?: string } | null)?.id;
	if (!id) return '';
	const node = dump.nodes.find((candidate) => candidate.id === id);
	if (!node) return '';
	const live = states.get(id);

	const rows: [string, string][] = [];
	if (live) rows.push(['status', live.state]);
	if (live?.cacheKey) rows.push(['key', live.cacheKey.slice(0, 8)]);
	if (node.persistent) rows.push(['persistent', 'yes']);
	if (node.pulledIn) rows.push(['pulled in', 'a dependency of a task you selected']);

	const body = rows
		.map(
			([label, value]) =>
				`<div class="d-flex justify-content-between gap-4 py-1 border-top"><span class="text-body-secondary">${escapeHtml(label)}</span><span>${escapeHtml(value)}</span></div>`,
		)
		.join('');

	// The command is a shell string from the user's own config, and this is HTML
	// handed to a renderer, so it is escaped rather than trusted.
	//
	// The classes are Bootstrap's own: a floating panel is a card with a shadow, and
	// writing it that way means the tooltip re-themes with everything else instead of
	// carrying a private copy of the surface colours.
	return [
		'<div class="card shadow p-3 small tw:max-w-[26rem]">',
		`<div class="font-monospace fw-medium">${escapeHtml(node.id)}</div>`,
		`<div class="font-monospace text-body-secondary mt-1 mb-2 tw:text-[0.75rem] tw:whitespace-pre-wrap tw:break-all">${escapeHtml(node.command)}</div>`,
		body,
		'</div>',
	].join('');
};

export interface LegendEntry {
	icon: string;
	label: string;
}

/**
 * The four states worth naming, keyed by the same glyphs the task list uses.
 *
 * The encodings left out are the ones the picture already explains: the arrows say
 * dependency order, "not run" is what a node that is not any of these looks like, and
 * a dashed border is spelled out in the node's own tooltip. A key long enough to need
 * reading is one nobody reads.
 */
export const LEGEND: LegendEntry[] = [
	{ icon: 'bi-check-lg', label: 'Ran' },
	{ icon: 'bi-lightning-charge', label: 'Cache hit' },
	{ icon: 'bi-x-lg', label: 'Failed' },
	{ icon: 'bi-square', label: 'Persistent' },
];
