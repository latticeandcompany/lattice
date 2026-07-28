import { useState } from 'react';

interface CopyCommandProps {
	command: string;
}

// Bootstrap input-group: the command sits in a readonly field with a copy button
// appended. Keeps it a real form component rather than a bespoke box.
const CopyCommand = ({ command }: CopyCommandProps) => {
	const [copied, setCopied] = useState(false);

	const copy = async () => {
		try {
			await navigator.clipboard.writeText(command);
			setCopied(true);
			setTimeout(() => setCopied(false), 1600);
		} catch {
			// Clipboard may be blocked; the command is still visible to copy manually.
		}
	};

	return (
		<div className="input-group input-group-lg" style={{ maxWidth: '26rem' }}>
			<span className="input-group-text" style={{ fontFamily: "'DM Mono', monospace" }} aria-hidden="true">
				$
			</span>
			<input
				type="text"
				className="form-control"
				style={{ fontFamily: "'DM Mono', monospace" }}
				value={command}
				readOnly
				aria-label="Install command"
			/>
			<button type="button" className="btn btn-outline-secondary" onClick={copy} aria-label={copied ? 'Copied' : 'Copy command'}>
				<i className={`bi ${copied ? 'bi-check-lg lattice-accent' : 'bi-clipboard'}`} aria-hidden="true" />
			</button>
		</div>
	);
};

export default CopyCommand;
