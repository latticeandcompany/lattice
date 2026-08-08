import { artFor } from '../lib/languageArt.ts';
import { languageMark } from '../lib/languages.ts';

interface LanguageMarkProps {
	tool: string | null | undefined;
	language: string | null | undefined;
}

// A driver's ecosystem, as its own logo. A driver with no ecosystem — one of the
// agnostic task runners — gets a monogram on the same square, which reads as a
// deliberate mark rather than a missing image.
const LanguageMark = ({ tool, language }: LanguageMarkProps) => {
	const mark = languageMark(tool, language);

	if (mark.kind === 'art' && mark.slug) {
		const source = artFor(mark.slug);
		if (source) {
			return (
				<span className="lang-mark lang-mark--art" title={mark.title}>
					<img src={source} alt="" />
				</span>
			);
		}
	}

	if (mark.kind === 'monogram') {
		return (
			<span className="lang-mark" title={mark.title}>
				{mark.monogram}
			</span>
		);
	}

	return (
		<span className="lang-mark lang-mark--none" title={mark.title} aria-label={mark.title}>
			<i className="bi bi-question" aria-hidden="true" />
		</span>
	);
};

export default LanguageMark;
