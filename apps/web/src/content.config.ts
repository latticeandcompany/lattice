import { defineCollection } from 'astro:content';
import { z as zod } from 'astro/zod';
import { glob } from 'astro/loaders';

// Docs content lives as Markdown/MDX in src/content/docs. Another agent writes the
// pages; the shell derives the sidebar from this frontmatter, so adding a file with
// a group + order is all it takes to slot a page into the nav.
const docs = defineCollection({
	loader: glob({ pattern: '**/*.{md,mdx}', base: './src/content/docs' }),
	schema: zod.object({
		title: zod.string(),
		description: zod.string().optional(),
		group: zod.string().default('Overview'),
		order: zod.number().default(100),
	}),
});

export const collections = { docs };
