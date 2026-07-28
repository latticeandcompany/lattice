# @lattice/web

The Lattice marketing site and documentation, built as one Astro site. The landing
page and the docs share a single brand system, layout language, and component set.

## Stack

- **Astro** — static-first, one site for landing + docs.
- **React** — interactive islands (nav, hero animation, docs sidebar/TOC, copy button).
- **Bootstrap** — components and structure, customized entirely through variable
  overrides (`src/styles/bootstrap.scss`). Bootstrap Icons load as a font through SASS.
- **Tailwind** — available for utility gaps; Bootstrap stays primary.
- **SCSS** — tokens, `@font-face`, keyframes, and prose styling.

All typefaces (DM Sans, DM Mono) and icon fonts are self-hosted in `public/fonts` —
no CDN, no external font service.

## Develop

```sh
npm install
npm run dev      # http://localhost:4321
npm run build    # static output in dist/
npm run preview
```

## Where things live

| Path | What |
| --- | --- |
| `src/pages/index.astro` | Landing page composition |
| `src/pages/docs/[...slug].astro` | Docs route (renders the content collection) |
| `src/content/docs/*.md` | Docs content — add a file with `title`/`group`/`order` and it appears in the sidebar |
| `src/components/` | Components (React `.tsx`, Astro `.astro`) |
| `src/styles/` | Bootstrap overrides, tokens, fonts, docs prose |
| `src/lib/` | Data: nav, languages, docs nav builder |
| `public/brand/` | Logo, favicon, and the woven-arc pattern |
| `public/fonts/` | Self-hosted DM Sans, DM Mono, Bootstrap Icons |

## Brand

Monochrome-first: ink `#020D0C` / paper `#FBF8FF` and a derived gray ramp, with a
restrained teal accent (per `marketing/BRAND.md`) on a few icons, the active nav
item, and focus rings. Copy speaks only to the end-user benefit; the product is
described as "a fast, local toolchain for managing monorepos." See
`marketing/BRAND.md` and `marketing/MESSAGING.md` for the full system.

## Design signatures

- **Weaving hero.** The rosette mark draws itself stroke by stroke on load, with
  threads running in from each supported language logo.
- **Post-footer pattern band.** The left half of the mark on black, then a run of
  pattern rectangles beside it, so the logo reads as the first tile in the series.

## Dogfood

The site is a workspace in the repo's `lattice.json`. Build it through Lattice with:

```sh
lattice run build --filter web
```

Docs content is intentionally minimal; the shell is ready for another agent to fill in.
