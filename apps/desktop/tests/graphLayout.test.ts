import assert from 'node:assert/strict';
import { test } from 'node:test';
import { closure, depths, filterDump, layerCount, layout } from '../src/lib/graphLayout.ts';
import type { GraphDump } from '../src/lib/types.ts';

const node = (id: string, extra: Partial<GraphDump['nodes'][number]> = {}) => {
	const [workspace, task] = id.split(':');
	return {
		id,
		workspace,
		task,
		command: `run ${task}`,
		persistent: false,
		pulledIn: false,
		...extra,
	};
};

const edge = (from: string, to: string, crossWorkspace = true) => ({ from, to, crossWorkspace });

test('a chain gets one layer per step', () => {
	const dump: GraphDump = {
		nodes: [node('a:build'), node('b:build'), node('c:build')],
		edges: [edge('a:build', 'b:build'), edge('b:build', 'c:build')],
	};
	const depth = depths(dump);
	assert.equal(depth.get('a:build'), 0);
	assert.equal(depth.get('b:build'), 1);
	assert.equal(depth.get('c:build'), 2);
	assert.equal(layerCount(dump), 3);
});

test('a diamond puts the join past its longest path, not its shortest', () => {
	// base -> left -> top and base -> right -> top, plus base -> top directly. `top`
	// belongs at depth 2: it cannot start until the longer path finishes.
	const dump: GraphDump = {
		nodes: [node('base:build'), node('left:build'), node('right:build'), node('top:build')],
		edges: [
			edge('base:build', 'left:build'),
			edge('base:build', 'right:build'),
			edge('left:build', 'top:build'),
			edge('right:build', 'top:build'),
			edge('base:build', 'top:build'),
		],
	};
	const depth = depths(dump);
	assert.equal(depth.get('base:build'), 0);
	assert.equal(depth.get('top:build'), 2);
});

test('a fan of independent workspaces all sits in one layer', () => {
	const dump: GraphDump = {
		nodes: [node('a:build'), node('b:build'), node('c:build')],
		edges: [],
	};
	assert.equal(layerCount(dump), 1);
	const points = layout(dump);
	const xs = new Set([...points.values()].map((p) => p.x));
	assert.equal(xs.size, 1, 'nothing depends on anything, so nothing moves right');
	const ys = new Set([...points.values()].map((p) => p.y));
	assert.equal(ys.size, 3, 'and they do not stack on top of each other');
});

test('the same graph lays out the same way twice', () => {
	const dump: GraphDump = {
		nodes: [node('a:build'), node('b:build')],
		edges: [edge('a:build', 'b:build')],
	};
	assert.deepEqual([...layout(dump).entries()], [...layout(dump).entries()]);
});

test('a closure reaches both ways from the node', () => {
	const dump: GraphDump = {
		nodes: [node('base:build'), node('mid:build'), node('top:build'), node('other:build')],
		edges: [edge('base:build', 'mid:build'), edge('mid:build', 'top:build')],
	};
	const found = closure(dump, 'mid:build');
	assert.deepEqual([...found].sort(), ['base:build', 'mid:build', 'top:build']);
	assert.ok(!found.has('other:build'));
});

test('an unconnected node is its own closure', () => {
	const dump: GraphDump = { nodes: [node('a:build')], edges: [] };
	assert.deepEqual([...closure(dump, 'a:build')], ['a:build']);
});

test('filtering drops the edges whose ends went with it', () => {
	const dump: GraphDump = {
		nodes: [node('api:build'), node('web:build')],
		edges: [edge('api:build', 'web:build')],
	};
	const filtered = filterDump(dump, { workspaces: ['web'] });
	assert.equal(filtered.nodes.length, 1);
	assert.equal(filtered.edges.length, 0, 'a dangling edge would draw to nothing');
});

test('a filtered graph re-layers rather than leaving a hole', () => {
	const dump: GraphDump = {
		nodes: [node('a:build'), node('b:build'), node('c:build')],
		edges: [edge('a:build', 'b:build'), edge('b:build', 'c:build')],
	};
	const filtered = filterDump(dump, { search: 'c:' });
	assert.equal(layerCount(filtered), 1);
	assert.equal(layout(filtered).get('c:build')?.x, 0);
});

test('search matches the label a run reports', () => {
	const dump: GraphDump = {
		nodes: [node('api:build'), node('api:test'), node('web:build')],
		edges: [],
	};
	assert.equal(filterDump(dump, { search: 'api' }).nodes.length, 2);
	assert.equal(filterDump(dump, { search: 'BUILD' }).nodes.length, 2);
	assert.equal(filterDump(dump, { tasks: ['test'] }).nodes.length, 1);
});
