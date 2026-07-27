# Lattice Product Messaging

One canonical source for how we describe the product. The README, website, and
CLI copy all derive from this. For visual identity — color, type, logo — see
`BRAND.md`; for register (how sentences should sound) see `BRAND.md` §4. This
doc covers only *what we say*: naming, tagline, pitch, value props, terminology.

---

## Naming

The product is **Lattice** on its own. Use it standalone in the README, the CLI,
docs, and running prose.

It becomes **Lattice Build** *only* when it appears alongside other Lattice & Co.
products (e.g. Lattice Cloud) and needs disambiguating. On its own it is never
"Lattice Build" — just "Lattice."

Per `BRAND.md` §1, the wordmark `lattice` is never colored. In the disambiguating
lockup the product word **Build** takes teal; standalone, the lockup is
monochrome. Never color the word `lattice`, and never introduce the teal "Build"
outside a multi-product context.

In prose the product is capitalized ("Lattice runs tasks in parallel"); the
rendered wordmark stays lowercase (see `BRAND.md` §6).

---

## Tagline

**A local-first build system for polyglot monorepos.**

---

## Elevator pitch

Lattice runs tasks across a polyglot monorepo in dependency order and in
parallel, and caches every result by content so unchanged work is skipped. It
never guesses: it reads the tools each workspace already uses — a lockfile, a
wrapper script, a native declaration — and runs those, stepping aside when you'd
rather declare the commands yourself. Everything runs locally, and the commands
stay visible in one file, so there is nothing to lock into.

---

## Core value props

**Local-first.**
Task running and caching happen on your machine. The cache is an on-disk,
content-addressed store; no account, network, or remote service is required to
build.

**Polyglot.**
One tool spans the languages in your repo — JavaScript/TypeScript, Rust, Python,
Go, Ruby, the JVM (Gradle and Maven), .NET, and more. Lattice identifies each
workspace's toolchain from tool-unique evidence in the directory and orders tasks
across all of them at once. (See `docs/toolchains.md` for the full built-in
driver set.)

**Inspectable, never-prescribe.**
Lattice runs *your* real tools and never invents a command. It acts only on
evidence — a lockfile, a native declaration file, or an explicit `engines`/
`scripts` block — following a fixed ladder (your declaration wins, then a native
file, then a lockfile). `auto` workspaces run the detected tool; set
`auto: false` and you declare the exact commands yourself. Either way the
commands live in plain `lattice.json` — readable, portable, and yours to run by
hand without Lattice.

**Complements Turborepo.**
The pipeline model is deliberately Turbo-shaped, so a JS monorepo already driven
by Turborepo can be declared as one workspace whose task simply calls
`turbo run …`. Lattice orchestrates *around* it across the other languages in the
repo — it wraps a Turborepo, it doesn't replace it.

---

## Terminology

Canonical one-line definitions. Reuse these verbatim; keep them faithful to the
code and to `docs/toolchains.md`.

| Term | Definition |
| --- | --- |
| **`lattice.json`** | The single config file at the repo root that declares workspaces, engines, and the pipeline. |
| **workspace** | A single project directory — the unit of task running and caching. Declared explicitly by path; no globs. |
| **pipeline** | The map of task definitions in `lattice.json`: each task's `dependsOn`, `inputs`, `ignore`, `outputs`, and cache behavior. |
| **task** | A named unit of work run in a workspace (e.g. `build`, `test`, `lint`), resolved to a concrete shell command. |
| **driver** | The tool that runs a workspace's tasks (npm, cargo, go, uv, gradle, …), selected by never-prescribe detection. Each driver has a **role** — runtime, build tool, package manager, or task runner — and different roles compose into one stack while two tools competing for the same role conflict until you disambiguate. |
| **engine** | A language runtime or tool with a version constraint (`node`, `rust`, `python`, `go`, …), inferred from the workspace or declared in `lattice.json`. |
| **toolchain** | The engines Lattice resolves for a build. Along the *engine gradient* it either uses what's on your `PATH`, validates the version, or provisions the tool into `./.lattice/toolchains/` and pins it in `pins.json`. |
| **content-addressed cache** | Task results keyed by a SHA-256 hash over the task, its command, inputs, environment, and lockfiles. A matching hash is a cache hit; the stored result is reused instead of re-run. |

---

## Voice for messaging

Register is defined once in `BRAND.md` §4 — precise, confident, understated,
Vercel/Linear, never hype. Do not restate it; follow it. What follows is
messaging-specific: how to write feature and benefit copy in that register.

- **Name the mechanism, not the adjective.** Describe *how* it's fast, not
  *that* it's fast: "parallel, dependency-ordered execution with a
  content-addressed cache," not "blazingly fast."
- **Feature → benefit in one breath.** "Caches by content, so unchanged work is
  skipped." Lead with what it does; let the payoff follow.
- **Only claims the code backs.** Every feature sentence should trace to real
  behavior. If it doesn't ship, it doesn't go in copy.
- **No fabricated numbers.** We have no verified benchmarks. Never write specific
  figures ("builds in 4s," "cached in 90ms" — the illustrative numbers in
  `BRAND.md` §4 are placeholders, not measurements). When a line wants a metric,
  leave `<!-- TODO: measured benchmark -->` until we have one.

| Instead of | Write |
| --- | --- |
| "Blazingly fast builds." | "Runs tasks in parallel and caches results by content." |
| "The only build tool you'll ever need." | "One build system across every language in the repo." |
| "Zero-config magic." | "Detects your tools from real evidence; override any command." |
| "Rip out Turborepo." | "Wrap your Turborepo as a workspace; Lattice runs around it." |
| "Locks your builds down." | "Commands stay in plain `lattice.json` — run them by hand anytime." |
