import type { CollectionEntry } from 'astro:content';
import { withBase } from './base.ts';

export interface DocLink {
	title: string;
	href: string;
	order: number;
}

export interface DocGroup {
	title: string;
	links: DocLink[];
}

// Groups render in this order; anything else is appended alphabetically after.
const GROUP_ORDER = ['Overview', 'Guides', 'Concepts', 'Reference'];

export const docHref = (id: string) => withBase(id === 'index' ? '/docs' : `/docs/${id}`);

export const buildDocsNav = (entries: CollectionEntry<'docs'>[]): DocGroup[] => {
	const byGroup = new Map<string, DocLink[]>();

	for (const entry of entries) {
		const link: DocLink = { title: entry.data.title, href: docHref(entry.id), order: entry.data.order };
		const list = byGroup.get(entry.data.group) ?? [];
		list.push(link);
		byGroup.set(entry.data.group, list);
	}

	const rank = (name: string) => {
		const i = GROUP_ORDER.indexOf(name);
		return i === -1 ? GROUP_ORDER.length : i;
	};

	return [...byGroup.entries()]
		.sort(([a], [b]) => rank(a) - rank(b) || a.localeCompare(b))
		.map(([title, links]) => ({
			title,
			links: links.sort((a, b) => a.order - b.order || a.title.localeCompare(b.title)),
		}));
};
