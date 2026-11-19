import type { ReactNode } from 'react';

interface PageHeadProps {
	title: string;
	/** What the screen does, on the right of its heading. */
	children?: ReactNode;
}

const PageHead = ({ title, children }: PageHeadProps) => (
	<header className="page-head">
		<h1>{title}</h1>
		{children && <div className="page-head__side">{children}</div>}
	</header>
);

export default PageHead;
