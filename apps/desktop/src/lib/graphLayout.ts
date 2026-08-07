// Where the nodes go.
//
// Layered, not force-directed, and the reason is not aesthetic: the only question a
// build graph is asked is what runs before what, and the backend already computed the
// answer — it hands nodes back in topological order. Longest-path depth over that is
// exact in one pass.
//
// A force layout would throw that away and approximate it from spring forces, settle
// somewhere different on every mount, and animate continuously, which cannot honour
// prefers-reduced-motion.

import type { GraphDump, GraphEdge, GraphNode } from './types.ts';

export interface Point {
	x: number;
	y: number;
}

export interface Gap {
	x: number;
	y: number;
}

export const DEFAULT_GAP: Gap = { x: 210, y: 58 };

/**
 * Longest-path depth per node.
 *
 * One forward pass is exact because `nodes` is already a topological order: every
 * prerequisite of a node has been visited before the node itself.
 */
export const depths = (dump: GraphDump): Map<string, number> => {
	const outgoing = new Map<string, string[]>();
	for (const edge of dump.edges) {
		const list = outgoing.get(edge.from);
		if (list) list.push(edge.to);
		else outgoing.set(edge.from, [edge.to]);
	}

	const depth = new Map<string, number>();
	for (const node of dump.nodes) depth.set(node.id, 0);

	for (const node of dump.nodes) {
		const here = depth.get(node.id) ?? 0;
		for (const next of outgoing.get(node.id) ?? []) {
			if ((depth.get(next) ?? 0) < here + 1) depth.set(next, here + 1);
		}
	}
	return depth;
};

/**
 * A position per node: depth on x, position within the layer on y.
 *
 * Rows within a layer follow the order the nodes arrived in, which is the config's
 * declaration order, so redrawing the same graph never reshuffles it.
 */
export const layout = (dump: GraphDump, gap: Gap = DEFAULT_GAP): Map<string, Point> => {
	const depth = depths(dump);
	const filled = new Map<number, number>();
	const points = new Map<string, Point>();

	for (const node of dump.nodes) {
		const column = depth.get(node.id) ?? 0;
		const row = filled.get(column) ?? 0;
		filled.set(column, row + 1);
		points.set(node.id, { x: column * gap.x, y: row * gap.y });
	}

	// Centre each column against the tallest, so the picture reads as a graph rather
	// than as text flushed to a corner.
	const tallest = Math.max(1, ...filled.values());
	for (const node of dump.nodes) {
		const column = depth.get(node.id) ?? 0;
		const height = filled.get(column) ?? 1;
		const point = points.get(node.id);
		if (point) point.y += ((tallest - height) * gap.y) / 2;
	}

	return points;
};

/** How many layers deep the graph is. */
export const layerCount = (dump: GraphDump): number => {
	if (dump.nodes.length === 0) return 0;
	return Math.max(...depths(dump).values()) + 1;
};

/**
 * Everything the given node depends on and everything that depends on it, plus the
 * node. This is what "focus" means for a build graph: the slice a change to this task
 * can affect, in both directions.
 */
export const closure = (dump: GraphDump, id: string): Set<string> => {
	const forward = new Map<string, string[]>();
	const backward = new Map<string, string[]>();
	for (const edge of dump.edges) {
		push(forward, edge.from, edge.to);
		push(backward, edge.to, edge.from);
	}

	const found = new Set<string>([id]);
	walk(forward, id, found);
	walk(backward, id, found);
	return found;
};

const push = (map: Map<string, string[]>, from: string, to: string) => {
	const list = map.get(from);
	if (list) list.push(to);
	else map.set(from, [to]);
};

const walk = (edges: Map<string, string[]>, start: string, found: Set<string>) => {
	const stack = [start];
	while (stack.length > 0) {
		const current = stack.pop() as string;
		for (const next of edges.get(current) ?? []) {
			if (!found.has(next)) {
				found.add(next);
				stack.push(next);
			}
		}
	}
};

export interface GraphFilter {
	workspaces?: readonly string[];
	tasks?: readonly string[];
	search?: string;
}

/**
 * Narrow a dump, then keep only the edges whose ends both survived.
 *
 * Re-layering afterwards is what makes a filtered graph tighten up rather than leave
 * the gaps where the removed nodes were.
 */
export const filterDump = (dump: GraphDump, filter: GraphFilter): GraphDump => {
	const search = filter.search?.trim().toLowerCase();
	const keep = (node: GraphNode) => {
		if (filter.workspaces?.length && !filter.workspaces.includes(node.workspace)) return false;
		if (filter.tasks?.length && !filter.tasks.includes(node.task)) return false;
		if (search && !node.id.toLowerCase().includes(search)) return false;
		return true;
	};

	const nodes = dump.nodes.filter(keep);
	const ids = new Set(nodes.map((node) => node.id));
	const edges = dump.edges.filter((edge: GraphEdge) => ids.has(edge.from) && ids.has(edge.to));
	return { nodes, edges };
};
