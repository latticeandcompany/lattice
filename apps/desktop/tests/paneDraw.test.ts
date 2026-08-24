import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LineBuffer } from '../src/lib/lineBuffer.ts';
import { nothingToDraw, paneDraw } from '../src/lib/paneDraw.ts';

const line = (n: number) => ({ stderr: false, line: `line ${n}` });

// A pane, as far as this can be tested without a webview: a list of span texts and a
// mark. Every assertion below is about those two staying honest.
const pane = () => {
	const spans: string[] = [];
	let mark = 0;
	return {
		spans,
		get mark() {
			return mark;
		},
		draw(buffer: LineBuffer) {
			const plan = paneDraw(spans.length, buffer.since(mark));
			if (nothingToDraw(plan)) return;
			for (const appended of plan.append) spans.push(appended.line);
			spans.splice(0, plan.trim);
			mark = plan.mark;
		},
	};
};

test('a pane opened late marks what was produced, not what it drew', () => {
	// 2,500 lines into a 2,000-line buffer: 500 are already gone, and a mark of 2,000
	// would ask for 500 lines it has just drawn all over again.
	const buffer = new LineBuffer(2000);
	for (let i = 0; i < 2500; i += 1) buffer.push(line(i));

	const late = pane();
	late.draw(buffer);
	assert.equal(late.mark, 2500);
	assert.equal(late.spans.length, 2000);

	const drawn = late.spans.length;
	late.draw(buffer);
	assert.equal(late.spans.length, drawn, 'nothing is appended twice');
});

test('a failure landing on an already open pane does not duplicate a block', () => {
	const buffer = new LineBuffer(2000);
	for (let i = 0; i < 5000; i += 1) buffer.push(line(i));

	const open = pane();
	open.draw(buffer);
	// `failureOutput` re-sends the captured tail, which the buffer appends.
	for (let i = 5000; i < 5010; i += 1) buffer.push(line(i));
	open.draw(buffer);

	assert.deepEqual(open.spans, [...buffer.all()].map((held) => held.line));
	assert.equal(new Set(open.spans).size, open.spans.length, 'every span is distinct');
});

test('a pane keeps appending when exactly as many lines fell off as arrived', () => {
	// mark 3, dropped 3, held 3: the arithmetic that read six produced lines as nine
	// left this pane silently stuck, three lines short, forever.
	const buffer = new LineBuffer(3);
	for (let i = 0; i < 3; i += 1) buffer.push(line(i));
	const watching = pane();
	watching.draw(buffer);
	assert.equal(watching.mark, 3);

	for (let i = 3; i < 6; i += 1) buffer.push(line(i));
	watching.draw(buffer);
	assert.equal(watching.mark, 6);
	assert.deepEqual(watching.spans, ['line 3', 'line 4', 'line 5']);

	buffer.push(line(6));
	watching.draw(buffer);
	assert.deepEqual(watching.spans, ['line 4', 'line 5', 'line 6'], 'still following');
});

test('a pane open for a whole build stays as long as the buffer, not as long as the build', () => {
	const buffer = new LineBuffer(2000);
	const watching = pane();
	for (let i = 0; i < 50_000; i += 1) {
		buffer.push(line(i));
		// A frame's worth at a time, as the store's batching delivers them.
		if (i % 137 === 0) {
			watching.draw(buffer);
			assert.ok(watching.spans.length <= 2000, `${watching.spans.length} spans at line ${i}`);
		}
	}
	watching.draw(buffer);
	assert.equal(watching.spans.length, 2000);
	assert.equal(watching.spans[1999], 'line 49999');
});

test('a pane that outlives its run redraws from the new run first line', () => {
	const first = new LineBuffer(2000);
	for (let i = 0; i < 40; i += 1) first.push(line(i));
	const watching = pane();
	watching.draw(first);
	assert.equal(watching.spans.length, 40);

	// A new run gives the task a fresh buffer, so its counts start over.
	const second = new LineBuffer(2000);
	watching.draw(second);
	assert.deepEqual(watching.spans, [], 'the last run is cleared rather than left behind');

	second.push(line(0));
	watching.draw(second);
	assert.deepEqual(watching.spans, ['line 0']);
});

test('nothing to draw is nothing to do', () => {
	const buffer = new LineBuffer(10);
	const watching = pane();
	watching.draw(buffer);
	assert.ok(nothingToDraw(paneDraw(0, buffer.since(0))));

	buffer.push(line(0));
	assert.ok(!nothingToDraw(paneDraw(0, buffer.since(0))));
});
