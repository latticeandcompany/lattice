// A line diff, for showing what `lattice migrate` would change.
//
// Longest common subsequence over lines. A 1.x config is a few hundred lines at the
// outside, so the quadratic table costs nothing worth a smarter algorithm.

export type DiffKind = 'same' | 'removed' | 'added';

export interface DiffLine {
	kind: DiffKind;
	text: string;
}

const lines = (text: string): string[] => {
	const split = text.replace(/\r\n/g, '\n').split('\n');
	if (split[split.length - 1] === '') split.pop();
	return split;
};

export const lineDiff = (before: string, after: string): DiffLine[] => {
	const a = lines(before);
	const b = lines(after);
	const table: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
	for (let i = a.length - 1; i >= 0; i -= 1) {
		for (let j = b.length - 1; j >= 0; j -= 1) {
			table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
		}
	}

	const out: DiffLine[] = [];
	let i = 0;
	let j = 0;
	while (i < a.length && j < b.length) {
		if (a[i] === b[j]) {
			out.push({ kind: 'same', text: a[i] });
			i += 1;
			j += 1;
		} else if (table[i + 1][j] >= table[i][j + 1]) {
			out.push({ kind: 'removed', text: a[i] });
			i += 1;
		} else {
			out.push({ kind: 'added', text: b[j] });
			j += 1;
		}
	}
	for (; i < a.length; i += 1) out.push({ kind: 'removed', text: a[i] });
	for (; j < b.length; j += 1) out.push({ kind: 'added', text: b[j] });
	return out;
};

/** How many lines each side has, for a one-line summary above the diff. */
export const lineCounts = (before: string, after: string): { before: number; after: number } => ({
	before: lines(before).length,
	after: lines(after).length,
});
