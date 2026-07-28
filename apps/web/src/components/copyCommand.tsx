import { useState } from 'react';

interface CopyCommandProps {
	command: string;
	/** 'sm' is the compact hero line; 'md' is the one in the install card. */
	size?: 'sm' | 'md';
}

// The install command is ~70 monospace characters, so both sizes pin a width and
// a matching font size rather than inheriting Bootstrap's — at the default input
// sizing the tail of the URL falls out of the field.
const SIZES = {
	sm: { group: 'input-group-sm', maxWidth: '35rem', fontSize: 'clamp(0.55rem, 1.7vw, 0.7rem)' },
	md: { group: '', maxWidth: '42rem', fontSize: 'clamp(0.6rem, 2vw, 0.82rem)' },
} as const;

// Bootstrap input-group: the command sits in a readonly field with a copy button
// appended. Keeps it a real form component rather than a bespoke box.
const CopyCommand = ({ command, size = 'md' }: CopyCommandProps) => {
	const [copied, setCopied] = useState(false);
	const { group, maxWidth, fontSize } = SIZES[size];

	const copy = async () => {
		try {
			await navigator.clipboard.writeText(command);
			setCopied(true);
			setTimeout(() => setCopied(false), 1600);
		} catch {
			// Clipboard may be blocked; the command is still visible to copy manually.
		}
	};

	const mono = { fontFamily: "'DM Mono', monospace", fontSize };

	return (
		<div className={`input-group ${group}`.trim()} style={{ maxWidth }}>
			<span className="input-group-text" style={mono} aria-hidden="true">
				$
			</span>
			<input type="text" className="form-control" style={mono} value={command} readOnly aria-label="Install command" />
			<button type="button" className="btn btn-outline-secondary" onClick={copy} aria-label={copied ? 'Copied' : 'Copy command'}>
				<i className={`bi ${copied ? 'bi-check-lg lattice-accent' : 'bi-clipboard'}`} aria-hidden="true" />
			</button>
		</div>
	);
};

export default CopyCommand;
