import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LEGEND, buildGraphOption, nodeStyle, tooltipHtml } from '../src/lib/graphOption.ts';
import { FALLBACK } from '../src/lib/themeTokens.ts';
import type { GraphDump } from '../src/lib/types.ts';

const dump: GraphDump = {
	nodes: [
		{ id: 'api:build', workspace: 'api', task: 'build', command: 'make api', persistent: false, pulledIn: false },
		{ id: 'web:build', workspace: 'web', task: 'build', command: 'npm run build', persistent: false, pulledIn: true },
		{ id: 'web:dev', workspace: 'web', task: 'dev', command: 'vite', persistent: true, pulledIn: false },
	],
	edges: [{ from: 'api:build', to: 'web:build', crossWorkspace: true }],
};

const option = (over: Partial<Parameters<typeof buildGraphOption>[0]> = {}) =>
	buildGraphOption({
		dump,
		states: new Map(),
		tokens: FALLBACK,
		reducedMotion: false,
		...over,
	});

const series = (opt: Record<string, unknown>) =>
	(opt.series as { data: any[]; links: any[] }[])[0];

test('the option carries every node and edge', () => {
	const s = series(option());
	assert.equal(s.data.length, 3);
	assert.equal(s.links.length, 1);
});

test('dark mode is stated rather than inferred', () => {
	// The default infers it from backgroundColor, which transparency makes impossible.
	const light = option();
	assert.equal(light.backgroundColor, 'transparent');
	assert.equal(light.darkMode, false);

	const dark = option({ tokens: { ...FALLBACK, dark: true } });
	assert.equal(dark.darkMode, true);
});

test('light and dark produce different literals from the same graph', () => {
	const light = series(option()).data[0].itemStyle.color;
	const dark = series(option({ tokens: { ...FALLBACK, dark: true, surface2: '#141f1e' } })).data[0]
		.itemStyle.color;
	assert.notEqual(light, dark);
});

test('a persistent task is a different shape, not a different colour', () => {
	const s = series(option());
	const dev = s.data.find((n) => n.id === 'web:dev');
	const build = s.data.find((n) => n.id === 'api:build');
	assert.equal(dev.symbol, 'roundRect');
	assert.equal(build.symbol, 'circle');
});

test('a pulled-in node is dashed', () => {
	const s = series(option());
	assert.equal(s.data.find((n) => n.id === 'web:build').itemStyle.borderType, 'dashed');
	assert.equal(s.data.find((n) => n.id === 'api:build').itemStyle.borderType, 'solid');
});

test('a cache-hit node recedes and says so in its label', () => {
	const s = series(
		option({ states: new Map([['api:build', { state: 'cached', cacheKey: 'abcdef12' }]]) }),
	);
	const node = s.data.find((n) => n.id === 'api:build');
	assert.ok(node.itemStyle.opacity < 1);
	assert.ok(node.label.formatter.includes('cache hit'));
});

test('failure is the only place amber appears', () => {
	const s = series(option({ states: new Map([['api:build', { state: 'failed' }]]) }));
	const failed = s.data.find((n) => n.id === 'api:build');
	assert.equal(failed.itemStyle.borderColor, FALLBACK.fail);

	const others = s.data.filter((n) => n.id !== 'api:build');
	for (const node of others) {
		assert.notEqual(node.itemStyle.borderColor, FALLBACK.fail);
	}
});

test('the accent appears only on the focused closure', () => {
	// The brand budgets the accent at about 5% of a view, and a focus ring is exactly
	// what it is licensed for.
	const plain = series(option());
	for (const node of plain.data) {
		assert.notEqual(node.itemStyle.borderColor, FALLBACK.focus);
	}

	const focused = series(option({ focus: new Set(['api:build', 'web:build']) }));
	const inFocus = focused.data.filter((n) => n.itemStyle.borderColor === FALLBACK.focus);
	assert.equal(inFocus.length, 2);
	// Everything outside is dimmed rather than recoloured.
	assert.ok(focused.data.find((n) => n.id === 'web:dev').itemStyle.opacity < 0.2);
});

test('reduced motion turns the animation off', () => {
	assert.equal(option({ reducedMotion: true }).animation, false);
	assert.equal(option({ reducedMotion: false }).animation, true);
});

test('hover does not rescale, which would re-lay-out every label', () => {
	const s = series(option()) as unknown as { emphasis: { scale: boolean } };
	assert.equal(s.emphasis.scale, false);
});

test('a command is escaped before it reaches the tooltip', () => {
	const hostile: GraphDump = {
		nodes: [
			{
				id: 'x:build',
				workspace: 'x',
				task: 'build',
				command: 'echo "<img src=x onerror=alert(1)>"',
				persistent: false,
				pulledIn: false,
			},
		],
		edges: [],
	};
	const html = tooltipHtml({ id: 'x:build' }, hostile, new Map());
	assert.ok(!html.includes('<img'), 'a config value must not become markup');
	assert.ok(html.includes('&lt;img'));
});

test('a tooltip for an unknown node is empty rather than broken', () => {
	assert.equal(tooltipHtml({ id: 'nope' }, dump, new Map()), '');
	assert.equal(tooltipHtml(null, dump, new Map()), '');
});

test('the legend names the states and stays short enough to read', () => {
	// A key long enough to need reading is one nobody reads, so it covers the states
	// a node can be in and leaves the rest to the arrows and the tooltip.
	assert.ok(LEGEND.length <= 4);
	const text = LEGEND.map((entry) => entry.label).join(' ').toLowerCase();
	for (const word of ['ran', 'cache hit', 'failed', 'persistent']) {
		assert.ok(text.includes(word), `the legend never mentions ${word}`);
	}
	for (const entry of LEGEND) {
		assert.ok(entry.icon.startsWith('bi-'), `${entry.label} has no glyph`);
	}
});

test('an empty tooltip carries no surface with it', () => {
	// ECharts shows its tooltip container whether or not the formatter returned
	// anything, so a panel drawn by the container appears as a bare rectangle over
	// the canvas the moment the pointer is on an edge rather than a node.
	assert.equal(tooltipHtml({ id: 'nope' }, dump, new Map()), '');
	const real = tooltipHtml({ id: 'api:build' }, dump, new Map());
	assert.ok(real.startsWith('<div class="card'), 'the panel is the formatter\'s own markup');
});

test('a done node takes the highest contrast fill', () => {
	const style = nodeStyle('done', false, FALLBACK);
	assert.equal(style.color, FALLBACK.text);
});
