import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LineBuffer } from '../src/lib/lineBuffer.ts';

const line = (n: number) => ({ stderr: false, line: `line ${n}` });

test('a buffer under capacity holds everything', () => {
	const buffer = new LineBuffer(10);
	for (let i = 0; i < 5; i += 1) buffer.push(line(i));
	assert.equal(buffer.length, 5);
	assert.equal(buffer.dropped, 0);
	assert.equal(buffer.produced, 5);
});

test('past capacity the oldest go and are counted', () => {
	const buffer = new LineBuffer(3);
	for (let i = 0; i < 10; i += 1) buffer.push(line(i));
	assert.equal(buffer.length, 3);
	assert.equal(buffer.dropped, 7);
	assert.equal(buffer.produced, 10);
	// The tail is kept, because that is where a failure is.
	assert.deepEqual(
		buffer.all().map((l) => l.line),
		['line 7', 'line 8', 'line 9'],
	);
});

test('since() hands back exactly what has not been drawn yet', () => {
	const buffer = new LineBuffer(10);
	for (let i = 0; i < 4; i += 1) buffer.push(line(i));

	const first = buffer.since(0);
	assert.equal(first.lines.length, 4);

	buffer.push(line(4));
	const next = buffer.since(4);
	assert.equal(next.lines.length, 1);
	assert.equal(next.lines[0].line, 'line 4');
});

test('since() stays contiguous across a wrap', () => {
	const buffer = new LineBuffer(3);
	for (let i = 0; i < 3; i += 1) buffer.push(line(i));
	// A reader has drawn all three.
	assert.equal(buffer.since(3).lines.length, 0);

	for (let i = 3; i < 6; i += 1) buffer.push(line(i));
	// Three arrived and three fell off, so the reader is told to drop what it has.
	const after = buffer.since(3);
	assert.equal(after.dropped, 3);
	assert.deepEqual(
		after.lines.map((l) => l.line),
		['line 3', 'line 4', 'line 5'],
	);
});

test('clearing resets the drop count too', () => {
	const buffer = new LineBuffer(2);
	for (let i = 0; i < 5; i += 1) buffer.push(line(i));
	buffer.clear();
	assert.equal(buffer.length, 0);
	assert.equal(buffer.dropped, 0);
	assert.equal(buffer.produced, 0);
});

// A reader has to end up holding what the buffer holds, or its own copy grows without
// bound while the buffer stays capped.
test('since() says how many lines are still held', () => {
	const buffer = new LineBuffer(3);
	for (let i = 0; i < 10; i += 1) buffer.push(line(i));
	assert.equal(buffer.since(0).held, 3);
	assert.equal(buffer.since(10).held, 3);
	assert.equal(new LineBuffer(3).since(0).held, 0);
});
