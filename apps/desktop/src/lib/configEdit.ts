// Editing lattice.json without rewriting it.
//
// The editor's source of truth is the file's text, never a parsed object, and that is
// the whole design. The Rust config types are `deny_unknown_fields`, so a key this
// build of the app has never heard of is still valid input the user is entitled to
// keep. A parse-mutate-stringify cycle would silently delete it, reorder the rest,
// and reformat the file.
//
// A minimal edit cannot: bytes outside the span being changed are never rewritten, so
// unknown keys, key order, comments, indentation, and line endings all survive by
// construction rather than by care.

import { applyEdits, findNodeAtLocation, modify, parseTree, type JSONPath } from 'jsonc-parser';

export type { JSONPath };

/**
 * Whichever line ending the file already uses.
 *
 * This repo checks lattice.json out CRLF, but any repo may be LF, and rewriting every
 * line on every save would turn a one-field change into a whole-file diff.
 */
export const detectEol = (text: string): '\r\n' | '\n' => (text.includes('\r\n') ? '\r\n' : '\n');

export interface Indent {
	tabSize: number;
	insertSpaces: boolean;
}

/** Whatever the file indents with, so an inserted key matches its neighbours. */
export const detectIndent = (text: string): Indent => {
	const line = text.split(/\r?\n/).find((candidate) => /^[\t ]+\S/.test(candidate));
	if (!line) return { tabSize: 2, insertSpaces: true };
	if (line.startsWith('\t')) return { tabSize: 1, insertSpaces: false };
	return { tabSize: line.length - line.trimStart().length, insertSpaces: true };
};

const options = (text: string) => ({
	formattingOptions: { ...detectIndent(text), eol: detectEol(text) },
});

/** Set the value at `path`. `undefined` removes the member. */
export const setValue = (text: string, path: JSONPath, value: unknown): string =>
	applyEdits(text, modify(text, path, value, options(text)));

/** Append to the array at `path`. */
export const insertInto = (text: string, path: JSONPath, value: unknown): string =>
	applyEdits(text, modify(text, [...path, -1], value, { ...options(text), isArrayInsertion: true }));

/** Remove the member or array element at `path`. */
export const removeAt = (text: string, path: JSONPath): string => setValue(text, path, undefined);

/** The byte offset of the value at `path`, for scrolling a raw view to a field. */
export const offsetOf = (text: string, path: JSONPath): number | null => {
	const root = parseTree(text);
	if (!root) return null;
	const node = findNodeAtLocation(root, path);
	return node ? node.offset : null;
};

/** 1-based line and column for an offset, to point at a diagnostic. */
export const positionAt = (text: string, offset: number): { line: number; column: number } => {
	const before = text.slice(0, offset);
	const lines = before.split(/\r?\n/);
	return { line: lines.length, column: (lines[lines.length - 1]?.length ?? 0) + 1 };
};

/** Parse for reading. Returns null when the text is not valid JSON yet. */
export const readConfig = <T>(text: string): T | null => {
	try {
		return JSON.parse(text) as T;
	} catch {
		return null;
	}
};
