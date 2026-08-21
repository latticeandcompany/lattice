# @lattice/web

The Lattice marketing site and documentation, built as one Astro site. The landing
page and the docs share the same components and styles.

## Stack

- Astro: static-first, one site for landing + docs.
- React: the interactive islands (nav, hero animation, docs sidebar/TOC, copy button).
- Bootstrap: components and structure, customized through variable overrides in
  `src/styles/bootstrap.scss`. Bootstrap Icons load as a font through SASS.
- Tailwind: utility gaps only, with every utility prefixed `tw:` so it cannot collide
  with a Bootstrap class. Bootstrap stays primary.
- SCSS: tokens, `@font-face`, keyframes, and prose styling.
- Pagefind: docs search, indexed from the built HTML and served as static files.

Typefaces (DM Sans, DM Mono) and icon fonts are self-hosted in `public/fonts`, with
no CDN.

## Develop

```sh
npm install
npm run dev      # http://localhost:4321
npm run build    # static output in dist/, then the Pagefind index
npm run preview
```

Search is indexed from built HTML, so it only works against a build: run
`npm run build && npm run preview`. Under `npm run dev` the palette opens and says
the index is missing.

## Where things live

| Path | What |
| --- | --- |
| `src/pages/index.astro` | Landing page composition |
| `src/pages/docs/[...slug].astro` | Docs route (renders the content collection) |
| `src/content/docs/*.md` | Docs content — add a file with `title`/`group`/`order` and it appears in the sidebar |
| `src/components/docsSearch.tsx` | Search palette (⌘K), talking to the Pagefind index |
| `src/components/` | Components (React `.tsx`, Astro `.astro`) |
| `src/styles/` | Bootstrap overrides, tokens, fonts, docs prose |
| `src/lib/` | Data: nav, languages, docs nav builder |
| `public/brand/` | Logo, favicon, and the woven-arc pattern |
| `public/fonts/` | Self-hosted DM Sans, DM Mono, Bootstrap Icons |

## Brand

Monochrome-first: ink `#020D0C` / paper `#FBF8FF` and a derived gray ramp, with a
restrained teal accent on a few icons, the active nav
item, and focus rings. Copy speaks only to the end-user benefit; the product is
described as "A high-performance, local toolchain for managing monorepos." The
colors themselves live in `src/styles/_tokens.scss`.

## Dogfood

The site is a workspace in the repo's `lattice.json`. Build it through Lattice with:

```sh
lattice run build --filter web
```

## Deploy

`.github/workflows/docs.yml` builds on every pull request touching `apps/web/**`
and deploys to GitHub Pages on push to `mega`. The build fails if the Pagefind
index is missing.

The workflow runs plain `npm` rather than `lattice run build`, so a docs deploy
does not depend on compiling the CLI first.
