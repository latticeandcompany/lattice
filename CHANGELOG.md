# Changelog

Nothing is released yet. Everything below is in `mega` on the way to 0.1.0.

## Copy and comment cleanup across the repo — 2026-07-28

### Voice guides
- `marketing/BRAND.md` §4 is now an enforceable contract rather than three bullets: it names the banned words, the pillar phrasings that must not ship, and the specific slop patterns that kept reappearing — bolded-lead-in bullet lists, rhythmic triads, unmeasurable comparatives, invented numbers, em-dash dramatic pauses, and aphorisms about ourselves
- The voice table previously used the banned word "polyglot" in its own recommended column, and offered invented benchmarks ("Cold builds in 4s. Cached in 90ms.") as the model answer in a section requiring measurable claims; both are corrected
- `marketing/MESSAGING.md` value props were named after the four core pillars, which is what the guidelines forbid; they are renamed for what the user gets
- The official tagline is recorded as an explicit, documented exception to the performance-adjective and pillar rules, so a future pass does not "correct" it
- `.claude/agents/lattice-docs.md` prescribed `### Added/Changed/Fixed` with bold lead-in entries, which was generating the changelog style it produced; it now carries the current format

### Copy
- The tagline is one string in 12 places, including the `crates.io` package description in `crates/lattice/Cargo.toml`. Two competing variants were in use
- "polyglot" is gone from all prose; the `examples/polyglot/` path is unchanged
- Every `https://lattice.build` URL is replaced. The domain is not registered, so the documented install command and all five docs links were dead
- Installation now documents the from-source path that works. The advertised `curl | sh` one-liner had no installer behind it at any URL, and there is no release to fetch a binary from
- GitHub URLs are normalized to the `latticeandcompany` org; the repo previously used two owners across 12 links

### Crates
- 67 dash-padded banner comments removed across 8 files, including the 22 `// ---- x tests ----` markers inside modules already named `mod tests`
- Every `(PRD §…)`, `(BRAND.md §…)`, and `(decision #…)` citation is gone. The PRD was deleted in `120ad78`, so all 19 were dangling references to a document that does not exist
- Redundant doc comments that restated the item's name are deleted, along with comments narrating past fixes or congratulating the code
- All-caps prose emphasis, rhythmic triads, and pillar bragging removed from module headers and doc comments
- User-facing error messages follow one convention: lowercase start, no trailing period
- Four stress-test assertions and four unit-test assertions updated to the new message casing

### Docs
- The orphaned 228-line `docs/toolchains.md` is folded into the published `apps/web/src/content/docs/toolchains.md`, which was a 14-line stub pointing back at the repo file, and the root duplicate is deleted
- `docs/` no longer exists; the published docs are the single source

## Public repository documentation — 2026-07-28

### Repo
- `.github/README.md` leads with the lockup and the tagline, the install one-liner, and a quick start whose terminal output is captured from a real run of `examples/polyglot` — a cold run, then the same run entirely from cache
- `CONTRIBUTING.md` covers the crate layout, the `feature/* → mega` flow, the build and test commands, dogfooding through `scripts/dev-link.sh`, and the Rust standards: clippy clean at `-D warnings`, no `unwrap`/`panic!` on user-reachable paths, errors propagate
- The testing policy requires tests with every change, the stress test updated and exiting `0`, and cache work proved in both directions
- The AI-assistance policy accepts agent-written contributions on the condition that they are disclosed and that the human author owns every line
- `CODE_OF_CONDUCT.md` sets out the enforcement ladder, the prohibited-conduct list, and appeals routed to the maintainer
- `SECURITY.md` routes reports through private GitHub advisories and states the threat model: executing declared commands, running detected tools, and provisioning a declared `installCmd` are intended behavior, while cache poisoning, extraction path traversal, escapes from `./.lattice/`, distribution integrity failures, and leaked secrets are in scope

### Issue and pull request templates
- Four YAML forms in `.github/ISSUE_TEMPLATE/`; blank issues are off and questions route to Discussions
- Bug reports collect `lattice version --json`, the platform, the relevant `lattice.json`, the exact command, and `-l` output
- Feature requests ask whether the thing is already possible by declaring the commands yourself, and gate on not requiring Lattice to identify a tool it has no evidence for
- The toolchain form asks for what a new driver actually needs: unambiguous detection evidence, the literal invoke form, real `versionCmd` output, the pinning file, and a non-interactive install command
- A docs form covers wrong, missing, or stale pages
- The pull request template requires tests, a changelog entry, a stress-test update, docs when behavior changes, and an AI-assistance disclosure

### Brand assets
- `.github/assets/` carries lockups whose wordmark is converted from `<text>` to outline paths, because an SVG loaded as an image cannot use a page's `@font-face` and every renderer without DM Sans installed was silently substituting a fallback face
- Glyphs are shaped with HarfBuzz against a `wght=500` instance of the DM Sans the site ships, then tracked (−0.056em) so the wordmark ink lands on the original canvas edge
- PNG copies sit alongside for renderers that reject SVG, and the Lattice & Company logos are keyed to transparency so they read on either GitHub theme
- The site ships the same outlined files at `public/brand/lockup-*.svg`, on the original viewBox, so every consumer of them is unaffected
- The composed lockups in the navbar and footer — an animated mark beside live `lattice` text — track to `--wordmark-tracking`, the same −0.056em the outlines were built with, so the wordmark reads identically whether it is set as text or drawn from the asset. The mark's draw-in animation is unchanged
- The Inkscape sources under `marketing/` keep live `<text>`, since they are the editable originals

## Nested repos: docs, worked example, and tests — 2026-07-28

### Docs
- A subtree that already has its own task runner is declared as a manual workspace whose scripts shell out to that runner; this needed no feature work, since ordering, `dependsOn`, caching as one opaque unit, and validation all fall out of the existing mechanism
- The nested-repos page covers the config, what each tool owns, and the `ignore` set that broad `inputs` require: dependency trees, the inner runner's own cache, and output directories
- Two limitations are documented — a manual workspace must declare any task invoked directly, and a downstream workspace must not copy an upstream artifact at build time, because a cache key covers only the inputs its own workspace declares

### Example & tests
- `examples/nested-repo` is a runnable repo with a real JS monorepo (npm workspaces, two packages, an inner dependency edge) as one Lattice node, plus a downstream service
- `e2e_passthrough.rs` and a new stress-test section prove the nested repo runs in graph order, provisions no toolchains, caches and restores as one unit, re-runs on an inner source edit, and never reports a hit when the inner runner's cache directory is left unignored

## Docs site search — 2026-07-28

### Site
- Full-text search over the documentation, built on Pagefind; the index is generated from the built HTML as the last step of `npm run build` and ships as static files
- `data-pagefind-body` on the docs article scopes the index to documentation prose, keeping the landing page and 404 out
- Page title and section come from the collection frontmatter rather than being scraped out of headings
- The palette opens with `⌘K`/`Ctrl-K` or `/`, walks results with the arrow keys, and lists heading-level matches beneath each page so a long page points at the section that matched
- With no index present the palette shows an explanatory notice; this is the case under `astro dev`, since search needs `npm run build && npm run preview`

## Docs site deploys from CI — 2026-07-28

### Site
- `.github/workflows/docs.yml` builds the site on every pull request touching `apps/web/**` and deploys to GitHub Pages on push to `mega`
- The build asserts the search index exists, so a missing index fails CI instead of shipping a search box that returns nothing
- Pages deploys are serialized and never cancelled mid-flight

## Docs navigation skeleton — 2026-07-28

### Site
- The sidebar now covers the planned documentation set under Overview, Guides, Concepts, and Reference
- Templates, Task graph, Persistent tasks, and CLI reference join as placeholders; Configuration, Caching, and Toolchains move to the groups they belong in
- The placeholder pages are empty on purpose and their content is tracked separately

## lattice-docs agent — 2026-07-28

### Repo
- A project subagent in `.claude/agents/` that carries the architecture, CLI surface, config schema, and brand voice as standing context, so documentation work starts from the real model instead of rediscovering it
- It maps the crate graph and the five mental models — engine gradient, evidence ladder, DAG and schedule, cache identity, output modes — and points at where each audience's docs live
- Every behavioral claim it writes has to trace to a file before it ships

## Site build output is no longer committed — 2026-07-28

### Repo
- `apps/web/dist/` and `apps/web/.astro/` were tracked, so every local build dirtied the tree and the repo carried a stale copy of the site
- Both are now ignored and untracked; CI builds the site from source

## Tailwind utilities are prefixed to stop Bootstrap collisions — 2026-07-27

### Site
- Tailwind utilities are namespaced with a `tw:` prefix, so the v4 on-demand scanner cannot regenerate bare classes that collide with Bootstrap (`collapse`, `container`, `col-*`, …)
- An unprefixed setup emitted `.collapse{visibility:collapse}` plus a Tailwind `.container` and grid, which hid the navbar actions and the docs sidebar and skewed the layout
- Write Tailwind utilities as `tw:flex`, `tw:text-teal-500`

## Dependencies upgraded across the workspace and the site — 2026-07-27

### Rust
- Notable majors: petgraph 0.6→0.8, sha2 0.10→0.11, console 0.15→0.16, indicatif 0.17→0.18, dialoguer 0.11→0.12, jsonschema 0.48→0.49, plus caret-compatible bumps via `cargo update` (anyhow, clap, tokio, serde, chrono, libc, indexmap, tempfile, …)
- sha2 0.11 dropped the `io::Write` impl on hashers, so the cache file digest now reads bytes in an explicit loop
- dialoguer 0.12 takes `Select::items` by value
- Build, tests, clippy, and the stress test all pass on the new versions

### Site
- `apps/web` moved to Astro 7 (from 5), Tailwind CSS v4 (from v3), and the latest `@astrojs/*`, React, Sass, and `astro-seo` releases
- Tailwind now runs through the CSS-first `@tailwindcss/vite` plugin; the deprecated `@astrojs/tailwind` integration was removed
- The brand theme lives in `src/styles/tailwind.css`, and Tailwind is imported without preflight so Bootstrap keeps owning the reset

## Persistent tasks stream their output by default — 2026-07-27

### Runner
- A `lattice run` that pulls in a persistent task — a dev server, a watcher, or anything in its dependency closure — now defaults to raw line-by-line output instead of the live TUI, so the process's streaming output stays visible; this previously required `-l` (`--loquacious`)
- Non-persistent runs on a terminal still get the interactive TUI
- A persistent task's output always streams live even in raw mode, while other per-task output stays collapsed and is surfaced on failure
- Auto-detection no longer fabricates a command for a `persistent` task: a direct-invoke driver (cargo, go, …) used to invent a command for any task name, so `lattice run dev` picked up every Rust and Go workspace as `cargo dev` or `go dev` even though no such task exists
- A persistent task now runs only where the workspace declares it, through an explicit `scripts` entry or a manifest script for the JS and Deno drivers; non-persistent tasks (`build`, `test`, …) still infer as before

## Stacked commands and a self-healing editor schema — 2026-07-27

### CLI
- `lattice run` accepts multiple tasks in one invocation (`lattice run lint test build`); the roots merge into a single dependency graph, so a dependency shared by several roots runs once and independent roots parallelize where the graph allows
- All existing flags (`--filter`, `--concurrency`, `--continue`, `--dry-run`, `--no-cache`) apply to the combined run, and an unknown task in the list fails fast and names the offender
- `--sequentially` / `-s` runs each task's graph to completion in the order given before starting the next; fail-fast stops at the first failed phase, and `--continue` runs the remaining phases and still exits non-zero
- `run`, `setup`, and `prune` write `.lattice/schema.json` when it is missing, as happens with a cleared cache directory or a clone where it was never committed, so an editor's JSON language server can resolve the config's `$schema`
- An existing copy is left untouched to avoid churn, and the schema is committed to this repo so validation works before the first run

## Marketing and docs site — 2026-07-27

### Site
- A single Astro site combining the landing page and the documentation, built primarily in React with Bootstrap and styled to the monochrome brand system
- The hero draws the rosette mark on load, with dashed threads running in from each supported language (Go, Node.js, Python, Ruby, .NET, JVM)
- The landing page carries a readable `lattice.json` and terminal sample, and an install action
- A post-footer band places the mark half cut off at the left edge with the woven-arc pattern tiling to the right
- The docs shell is sidebar, content, and an on-this-page table of contents, driven by a Markdown/MDX content collection
- DM Sans, DM Mono, and Bootstrap Icons are self-hosted and loaded through SASS
- Light, dark, and system themes, keyboard focus, and reduced-motion support
- `apps/web` is a workspace in `lattice.json`, so `lattice run build --filter web` builds the site and caches the result

### Copy
- Corrected the `lattice.json` examples on the landing page and in Getting started to the real schema: a `workspaces` array with `name` and `path`, plural `engines` version constraints, and `tasks` in place of the outdated `pipeline` key
- Dropped the "nothing to set up / no config to write" claims, which overstated the tool — a `lattice.json` is required and `lattice init` scaffolds it, and workspaces are declared explicitly
- Reframed that claim to what is true: Lattice infers each project's build from its native manifest, so there are no per-language build scripts to write
- Hero language logos use real brand artwork through Astro's `<Image>` component
- The footer carries the Lattice & Company parent lockup, a rosette with a self-hosted DM Serif Text wordmark
- Copy no longer promises offline-only operation, leaving room for Lattice Cloud

## The documented install command installed the wrong software — 2026-07-27

### Docs
- The docs site told readers to run `cargo install lattice`, but `lattice` on crates.io is an unrelated markdown linter, so anyone following the getting-started page or the landing-page copy button got someone else's tool
- Both now show the repo-local bootstrap one-liner, with `cargo install --git … lattice` documented as the from-source path

## License of record was inconsistent — 2026-07-27

### Repo
- `LICENSE` is ISC while the workspace manifest declared `license = "MIT"`; the manifest now says `ISC`
