// The site deploys to a GitHub Pages project subpath, so every root-relative link
// and public asset needs the `base` prefix. Vite inlines import.meta.env.BASE_URL at
// build time, so this works in .astro templates and client-side React alike.
const base = import.meta.env.BASE_URL.replace(/\/$/, '');

/** Prefix a root-relative path (leading slash) with the configured site base. */
export const withBase = (path: string) => `${base}${path}`;
