import assert from 'node:assert/strict';
import { test } from 'node:test';
import { maxSections, toHits, type PagefindData } from '../src/lib/search.ts';

const page = (overrides: Partial<PagefindData> = {}): PagefindData => ({
	url: '/lattice/docs/caching/',
	excerpt: 'Task results are keyed by a <mark>hash</mark>.',
	meta: { title: 'Caching' },
	sub_results: [],
	...overrides,
});

// The site is served from a subpath and Pagefind already applies it, so the one thing
// this must not do is add the base a second time.
test('passes result URLs through untouched', () => {
	const [hit] = toHits([page()]);
	assert.equal(hit.href, '/lattice/docs/caching/');
});

test('keeps the base on section links too', () => {
	const hits = toHits([
		page({
			sub_results: [
				{ title: 'Caching', url: '/lattice/docs/caching/', excerpt: 'x' },
				{ title: 'What goes into a hash', url: '/lattice/docs/caching/#hash', excerpt: 'y' },
			],
		}),
	]);
	assert.deepEqual(
		hits.map((hit) => hit.href),
		['/lattice/docs/caching/', '/lattice/docs/caching/#hash'],
	);
});

test('drops the sub-result that just restates the page', () => {
	const hits = toHits([
		page({
			sub_results: [{ title: 'Caching', url: '/lattice/docs/caching/', excerpt: 'x' }],
		}),
	]);
	assert.equal(hits.length, 1);
	assert.equal(hits[0].section, undefined);
});

test('caps the sections shown per page', () => {
	const sub_results = Array.from({ length: maxSections + 3 }, (_, i) => ({
		title: `Section ${i}`,
		url: `/lattice/docs/caching/#s${i}`,
		excerpt: 'x',
	}));
	assert.equal(toHits([page({ sub_results })]).length, maxSections + 1);
});

test('falls back to the URL when a page has no title', () => {
	const [hit] = toHits([page({ meta: {} })]);
	assert.equal(hit.title, '/lattice/docs/caching/');
});
