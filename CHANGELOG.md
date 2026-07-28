# Changelog

## Unreleased

### Added

- **Marketing + docs site** (`apps/web`) — a single Astro site combining the landing
  page and documentation, built primarily in React with Bootstrap, styled to the
  monochrome brand system.
  - Weaving hero: the rosette mark draws itself on load with dashed threads running
    in from each supported language (Go, Node.js, Python, Ruby, .NET, JVM).
  - Value props, an inspectable `lattice.json` + terminal sample, and an install CTA.
  - Post-footer pattern band: the mark half cut off at the left edge with the
    woven-arc pattern tiling to the right.
  - Custom Vercel-style docs shell: sidebar + content + on-this-page TOC, driven by a
    Markdown/MDX content collection. Content left minimal for a follow-up pass.
  - Self-hosted DM Sans, DM Mono, and Bootstrap Icons (loaded through SASS); no CDN.
  - Light, dark, and system themes, keyboard focus, and reduced-motion support.
  - Wired into the Lattice dogfood: `apps/web` is a workspace in `lattice.json`, so
    `lattice run build --filter web` builds the site and caches the result.

### Changed

- Site copy pass: corrected the `lattice.json` examples on the landing page and in
  Getting started to the real schema (a `workspaces` array with `name`/`path`, plural
  `engines` version constraints, and `tasks` in place of the outdated `pipeline` key),
  and loosened stiff, contraction-free phrasing across the landing and docs copy so it
  reads in the brand's plain, confident voice.
- Dropped the "nothing to set up / no config to write" landing claims, which
  overstated the tool: a `lattice.json` is required (`lattice init` scaffolds it) and
  workspaces are declared explicitly. Reframed to the accurate benefit — Lattice infers
  each project's build from its native manifest, so there are no per-language build
  scripts to write.
- Landing copy speaks only to the end-user benefit; product tagline standardized on
  "A fast, local toolchain for managing monorepos."
- Hero language logos use real brand artwork through Astro's `<Image>` component.
- Post-footer band reworked into the mark plus a run of pattern rectangles.
- UI built from Bootstrap components throughout (navbar, cards, dropdown, input-group,
  nav); a restrained teal accent added per the brand.
- Footer now carries the Lattice & Company parent lockup (rosette + DM Serif Text
  wordmark, self-hosted). Copy no longer promises offline-only, leaving room for
  Lattice Cloud.
