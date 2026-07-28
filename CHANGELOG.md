# Changelog

## Unreleased

### Changed

- **Rust workspace dependencies upgraded to latest** — notable majors: petgraph
  0.6→0.8, sha2 0.10→0.11, console 0.15→0.16, indicatif 0.17→0.18, dialoguer
  0.11→0.12, and jsonschema 0.48→0.49, plus caret-compatible bumps via `cargo
  update` (anyhow, clap, tokio, serde, chrono, libc, indexmap, tempfile, …). Two
  source fixes were required: sha2 0.11 dropped the `io::Write` impl on hashers,
  so the cache file digest now reads bytes in an explicit loop, and dialoguer
  0.12 takes `Select::items` by value. Build, tests, clippy, and the stress test
  all pass on the new versions.

- **Marketing site dependencies upgraded** — `apps/web` moved to Astro 7
  (from 5), Tailwind CSS v4 (from v3), and the latest `@astrojs/*`, React,
  Sass, and `astro-seo` releases. Tailwind now runs through the CSS-first
  `@tailwindcss/vite` plugin (the deprecated `@astrojs/tailwind` integration was
  removed); the brand theme lives in `src/styles/tailwind.css` and Tailwind is
  imported without preflight so Bootstrap keeps owning the reset. Tailwind
  utilities are namespaced with a `tw:` prefix so its v4 on-demand scanner can't
  regenerate bare classes that collide with Bootstrap (`collapse`, `container`,
  `col-*`, …) — an unprefixed setup emitted `.collapse{visibility:collapse}` and
  a Tailwind `.container`/grid, which hid the navbar actions and docs sidebar and
  skewed layout. Write Tailwind utilities as `tw:flex`, `tw:text-teal-500`.

- **Persistent tasks stream by default** — a `lattice run` that pulls in a
  persistent task (a dev server, watcher, or anything in its dependency closure)
  now defaults to raw, line-by-line output instead of the live TUI, so the
  process's streaming output stays visible. Previously this required `-l`
  (`--loquacious`). Non-persistent runs on a terminal still get the interactive
  TUI. A persistent task's output always streams live even in raw mode (other
  per-task output stays collapsed and is surfaced on failure).

### Fixed

- **Persistent tasks are no longer fabricated for auto workspaces** — a
  direct-invoke driver (cargo, go, …) used to invent a command for *any* task
  name, so `lattice run dev` picked up every Rust/Go workspace as `cargo dev` /
  `go dev` even though no such task exists. Auto-detection now never fabricates a
  command for a `persistent` task; it runs only where the workspace actually
  declares it (an explicit `scripts` entry, or a manifest script for JS/deno
  drivers). Non-persistent tasks (`build`, `test`, …) still infer as before.

### Added

- **Self-healing editor schema** — `run`, `setup`, and `prune` now write
  `.lattice/schema.json` when it's missing (a cleared cache dir, or a clone where
  it was never committed), so an editor's JSON language server can always resolve
  the config's `$schema`. An existing copy is left untouched to avoid churn. The
  schema is also committed to this repo so validation works before the first run.

- **Stacked commands** — `lattice run` now accepts multiple tasks in one
  invocation (e.g. `lattice run lint test build`). The roots are merged into a
  single dependency graph, so a dependency shared by several roots runs once and
  independent roots parallelize where the graph allows. All existing flags
  (`--filter`, `--concurrency`, `--continue`, `--dry-run`, `--no-cache`) apply to
  the combined run; an unknown task in the list fails fast and names the offender.
  - `--sequentially` / `-s` runs each task's graph to completion, in the order
    given, before starting the next — the strict-phases alternative to the merged
    default. Fail-fast stops at the first failed phase; `--continue` runs the
    remaining phases and still exits non-zero.

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
