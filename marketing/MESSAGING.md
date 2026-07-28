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

**A high-performance, local toolchain for managing monorepos.**

This is the official tagline. Use it verbatim in the README, the site, page
titles, and the CLI `about`. Do not write variants, and do not "correct" it.

It is a deliberate exception to two rules that hold everywhere else in our copy:
it uses a performance adjective, and it names local execution. Those rules still
apply to every other string — see `BRAND.md` §4. The tagline is the one place we
make the claim directly.

---

## Elevator pitch

Lattice runs the tasks in your monorepo in dependency order and in parallel, then
caches every result by content so unchanged work is skipped. It reads the tools
each project already declares — a lockfile, a native declaration file — and runs
those, or you declare the commands yourself. It also pins the versions of those
tools, so everyone builds against the same ones.

---

## Core value props

Named for what the user gets, not for the principle behind it. A value prop that
needs the words *local-first*, *never-prescribe*, or *zero-config* to land has
not found its benefit yet.

**One build across every language in the repo.**
JavaScript and TypeScript, Rust, Python, Go, Ruby, the JVM (Gradle and Maven),
.NET, and more. Lattice identifies each project's toolchain from tool-unique
evidence in the directory and orders tasks across all of them in one graph. (See
the [toolchains page](https://latticeandcompany.github.io/lattice/docs/toolchains)
for the built-in driver set.)

**Unchanged work is skipped.**
Every task result is keyed by a hash over its command, inputs, environment, and
lockfiles. Change one file and only what depends on it runs again.

**Your real commands, in one file you can read.**
Lattice runs the tools already in each directory and never invents a command. It
acts on evidence, following a fixed ladder: your declaration wins, then a native
file, then a lockfile. `auto` projects run the detected tool; set `auto: false`
and you declare the exact commands. Either way they live in plain `lattice.json`
and you can run any of them by hand.

**Everyone builds against the same tool versions.**
Declare `node`, `rust`, `go` or anything else with a version constraint. Lattice
validates it against the host tool, or provisions it into
`./.lattice/toolchains/` and pins the result, so a fresh clone and CI resolve the
same versions.

**An existing task runner becomes one project.**
A subtree that already has its own runner is declared as a single workspace whose
task calls that runner. Lattice orders and caches it as one unit and schedules the
rest of the repo around it.

---

## Terminology

Canonical one-line definitions. Reuse these verbatim; keep them faithful to the
code and to the [toolchains page](https://latticeandcompany.github.io/lattice/docs/toolchains).

| Term | Definition |
| --- | --- |
| **`lattice.json`** | The single config file at the repo root that declares workspaces, engines, and the pipeline. |
| **workspace** | A single project directory — the unit of task running and caching. Declared explicitly by path; no globs. |
| **pipeline** | The map of task definitions in `lattice.json`: each task's `dependsOn`, `inputs`, `ignore`, `outputs`, and cache behavior. |
| **task** | A named unit of work run in a workspace (e.g. `build`, `test`, `lint`), resolved to a concrete shell command. |
| **driver** | The tool that runs a workspace's tasks (npm, cargo, go, uv, gradle, …), identified from evidence in the directory. Each driver has a **role**: runtime, build tool, package manager, or task runner. Different roles compose into one stack. Two tools competing for the same role conflict until you disambiguate. |
| **engine** | A language runtime or tool with a version constraint (`node`, `rust`, `python`, `go`, …), inferred from the workspace or declared in `lattice.json`. |
| **toolchain** | The engines Lattice resolves for a build. Along the *engine gradient* it either uses what's on your `PATH`, validates the version, or provisions the tool into `./.lattice/toolchains/` and pins it in `pins.json`. |
| **content-addressed cache** | Task results keyed by a SHA-256 hash over the task, its command, inputs, environment, and lockfiles. A matching hash is a cache hit; the stored result is reused instead of re-run. |

---

## Voice for messaging

Register is defined once in `BRAND.md` §4: precise, confident, understated, never
hype. That section also lists the banned words and the specific slop patterns we
keep repeating. Do not restate it; follow it. What follows is messaging-specific,
about how to write feature and benefit copy in that register.

Name the mechanism, not the adjective. Describe *how* it's fast, not *that* it's
fast: "runs tasks in parallel and caches results by content," not "blazingly
fast." The official tagline is the single exception; everywhere else, earn the
claim by naming the mechanism.

Lead with what it does and let the payoff follow, in one breath: "caches by
content, so unchanged work is skipped."

Write only claims the code backs. Every feature sentence should trace to real
behavior. If it doesn't ship, it doesn't go in copy.

Use no fabricated numbers. We have no published benchmarks. Never write a
specific figure we haven't measured, in copy or as an illustration. When a line
wants a metric, leave `<!-- TODO: measured benchmark -->` until we have one.

| Instead of | Write |
| --- | --- |
| "Blazingly fast builds." | "Runs tasks in parallel and caches results by content." |
| "The only build tool you'll ever need." | "One build system across every language in the repo." |
| "Zero-config magic." | "Reads the tools already in each directory; override any command." |
| "Drop-in Turborepo replacement." | "A project with its own task runner becomes one workspace; Lattice schedules around it." |
| "Locks your builds down." | "Commands stay in plain `lattice.json`. Run them by hand anytime." |
| "Builds finish sooner." | "Change one file and only what depends on it runs again." |
