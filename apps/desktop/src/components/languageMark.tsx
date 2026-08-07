import { artFor } from '../lib/languageArt.ts';
import { languageMark } from '../lib/languages.ts';

interface LanguageMarkProps {
	tool: string | null | undefined;
	language: string | null | undefined;
	dark: boolean;
}

// A driver's ecosystem, as artwork where we have it and a monogram where we do not.
// The monogram sits on the same square as a wizard step number, so a workspace with
// no logo still reads as deliberate rather than as a missing image.
const LanguageMark = ({ tool, language, dark }: LanguageMarkProps) => {
	const mark = languageMark(tool, language);

	if (mark.kind === 'art' && mark.slug) {
		const source = artFor(mark.slug, dark);
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
