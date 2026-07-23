# **Lattice Product Requirement Document (PRD)**

## **Specification**

Lattice is a local-first, high-performance, and language-agnostic monorepo build system written in Rust. Heavily inspired by Turborepo, Lattice is built by and for Turborepo fans to extend the speed and simplicity of modern build graphs to polyglot codebases.

Lattice provides robust dependency graph resolution, parallel task execution, local-first caching, and custom toolchain isolation, completely independent of external CDNs, URL imports, or global runtime configurations. A future managed service, **Lattice Cloud**, will offer an opt-in remote cache — but the open-source runtime never requires it, and works fully offline by default.

## **1. Executive Summary & Design Philosophy**

Lattice is built on a foundational philosophy that prioritizes developer autonomy, local environment control, and clear execution flows.

### **1.1 Core Principles**

* **Local-First & Inspectable:** If a developer cannot `cd` into an asset, read its configuration in plain text, and delete it with a simple `rm -rf`, Lattice does not use it. There are no hidden databases or globally scoped configurations. This governs Lattice's *runtime*: builds, caching, and graph resolution never depend on a network or remote service. Convenience actions that are explicitly opt-in and developer-initiated — installing the binary (§9), adding a template from a URL (§8.2), or connecting an optional Lattice Cloud remote cache (§5.3) — may reach the network, but Lattice never forces or requires a remote source, and everything the local runtime fetches lands as plain, inspectable files under `./.lattice/`.
* **Root-Level Centralized Management:** To prevent configuration scattering across sprawling directories, all workspace definitions, task commands, graph overrides, and settings live strictly inside a single, centralized root-level `lattice.json` file.
* **Explicit Over Magic:** Workspaces are declared explicitly, one directory at a time. Lattice never crawls the filesystem guessing what is or is not a project. Within a declared workspace, Lattice can *auto-detect* the toolchain and task commands — but the set of workspaces itself is always exactly what the developer wrote down.
* **Turborepo Synergy:** Lattice is not designed to replace Turborepo. It is built to seamlessly coexist with it. Developers can wire an existing Turborepo-managed sub-monorepo in as a single manual workspace inside the broader Lattice parent graph. The CLI is intentionally *familiar* to Turborepo users — `run`, `--filter`, a `pipeline` — without being a clone; Lattice keeps its own branding, voice, and output.

## **2. Project Discovery & Dependency Graph Construction**

Lattice maps polyglot codebases by reading the explicitly declared workspaces and building a Directed Acyclic Graph (DAG) of project tasks.

### **2.1 Workspace Resolution**

A Lattice monorepo is made up of **workspaces**. A workspace is a single project directory that is the unit of task running and caching. Every workspace is declared explicitly as one object in the root `"workspaces"` array — there are no glob patterns and no automatic filesystem crawl. Naming, auto-detection, engines, dependencies, and task overrides are all configured per workspace, right here:

* **`"path"`** (string, required): the path, relative to the repo root, to the single directory that makes up the workspace (e.g. `"apps/web"` or `"tools/legacy-cpp"`). This is a literal directory path, **not** a glob — `"apps/*"` is not valid. Each directory you want managed gets its own entry.
* **`"name"`** (string): the workspace name, used for `"dependsOn"` references and in output. Defaults to the basename of `"path"`.
* **`"auto"`** (boolean, default `true`): when `true`, Lattice infers this workspace's engine, package manager, and task commands from its first-class native manifest (§2.2, §3). When `false`, Lattice performs no inference and uses only what this entry declares — this is how non-first-class or bespoke projects (a hand-rolled C++ build, a wrapped Turborepo) are wired in.
* **`"engines"`** (map): the toolchain(s) this workspace requires, with version constraints, validated before it builds (§7). When `"auto"` is `true`, Lattice fills this in from the detected manifest; declaring it pins or overrides the inferred toolchain, and is required for custom engines on manual workspaces.
* **`"dependsOn"`** (array): other workspaces this one depends on, referenced by name.
* **`"tasks"`** (map): explicit task-name → command overrides.

A bare entry (just a `"path"`, `auto` by default) needs nothing else — Lattice reads the directory, detects its toolchain, and infers its tasks. Auto and manual workspaces coexist in this one array.

**Strict failure:** Lattice halts and reports the offending workspace by name if:
* a workspace's `"path"` does not point to an existing directory;
* two workspaces resolve to the same name or the same path;
* a workspace is `"auto": false` and a requested task has no command in its `"tasks"` map;
* an `"auto": true` workspace's directory contains no recognized first-class manifest.

### **2.2 Task Resolution Hierarchy**

Lattice does not perform "no-op" task executions or guess commands for custom or unsupported language directories. For each workspace it resolves what script to execute for a task using a strict, centralized priority chain:

1. **Per-Workspace Task Override:** Lattice checks the workspace's own `"tasks"` map in the root `lattice.json` for a command matching the requested task. If present, it wins.
2. **First-Class Ecosystem Inference:** If the workspace is `"auto": true` and declares no override, Lattice reads its recognized native manifest (such as `package.json`, `Cargo.toml`, or `go.mod`) and infers the correct toolchain script (§3).
3. **Strict Failure Safety:** If no override is declared and the workspace is either `"auto": false` or does not match a first-class ecosystem, Lattice immediately halts execution and prints an error pointing to the exact workspace.

## **3. First-Class Ecosystem Support Matrix**

Lattice provides native parser logic to detect the toolchain and infer task commands for the following ecosystems. All are supported in v1; **JavaScript/TypeScript, Rust, Go, and Python are the core priority tier** and receive the deepest testing coverage.

| Language | Package Manager | Native Manifest File | Default Task Command |
| :---- | :---- | :---- | :---- |
| **JavaScript / TypeScript** | npm / pnpm / yarn / bun | package.json | Runs package script (e.g., `bun run <task>`) |
| **Python** | poetry / pip / uv | pyproject.toml | Runs project task configuration |
| **Rust** | cargo | Cargo.toml | Executes corresponding cargo flag (`cargo <task>`) |
| **Go** | go mod | go.mod | Runs native go command (`go <task>`) |
| **Ruby** | bundler / gem | Rakefile / Gemfile | Invokes `rake <task>` |
| **C# / .NET** | NuGet | .csproj / project.json | Runs `dotnet <task>` commands |
| **Java (Gradle)** | Gradle | build.gradle | Invokes gradle wrapper (`./gradlew <task>`) |
| **Java (Maven)** | Maven | pom.xml | Invokes maven wrapper (`./mvnw <task>`) or system `mvn <task>` |
| **C / C++** | Make | Makefile | Invokes `make <task>` targets. Projects without a Makefile (e.g., raw CMake or bespoke builds) are not auto-inferred and must be wired in as a manual workspace (`"auto": false`) with an explicit `"tasks"` map. |

## **4. Task Execution Engine & Parallel Pipeline**

Lattice schedules pipeline actions across the DAG with non-blocking concurrency.

```
┌────────────────────────┐
│ 1. Build Topo Order    │ ──► Milliseconds. Generates execution sequence.
└────────────────────────┘
            │
            ▼
┌────────────────────────┐
│ 2. Assess Cache Status │ ──► Under 10ms. Matches unique hash fingerprint.
└────────────────────────┘
            │
            ▼
┌────────────────────────┐
│ 3. Process Execution   │ ──► Uncached tasks run concurrently off-thread.
└────────────────────────┘
```

### **4.1 Persistent Dev Mode**

For interactive services that do not terminate (such as development web servers), developers can tag the pipeline definition with `"persistent": true` in the root `lattice.json`.

* Once a persistent task boots, Lattice keeps the process active in the background.
* **DAG Constraint:** Because persistent tasks run indefinitely and never exit, they cannot be depended on by other tasks in the pipeline. Lattice validates this during DAG construction and will refuse to execute pipelines that place downstream dependencies on persistent nodes.

## **5. Caching, Hashing, & Local-First Isolation**

### **5.1 Smart Hashing**

Lattice computes task hashes using the content of matched workspace files, registered environment variables, the command string, and native lockfile states. Files that should not trigger rebuilds are managed via the `"ignore"` key in the root `lattice.json` pipeline configuration.

### **5.2 Local Inspectable Cache**

All cache outputs are written to the local directory under `./.lattice/cache/`. Each cached run creates up to two files:

* `<hash>.tar.gz`: The zipped output directory specified by the `"outputs"` configuration. Tasks that declare no `"outputs"` produce no archive; only the descriptor is written.
* `<hash>.meta.json`: A completely transparent JSON descriptor containing basic execution metrics, target workspace properties, and execution durations.

Persistent tasks and any task marked `"cache": false` are never cached and write neither file.

```json
{
  "hash": "8f3a9d2c1e",
  "task": "build",
  "workspace": "rust-api",
  "durationMs": 1140,
  "lastUsed": "2026-07-16T22:04:10Z",
  "env": {
    "CARGO_PROFILE": "release"
  }
}
```

Lattice manages disk space automatically using a Least Recently Used (LRU) pruning strategy bounded by the `"maxCacheSize"` value defined in the root `lattice.json`.

### **5.3 Lattice Cloud (Future — Remote Cache)**

Lattice Cloud is a planned, **opt-in, paid** managed remote cache — analogous to Vercel's remote cache for Turborepo — that lets a team share task artifacts across machines and CI. It is **out of scope for v1**, but the local cache is deliberately designed so that adding it later is a clean extension, not a rewrite:

* The cache is addressed purely by the content hash (§5.1), which is transport-agnostic — the same key that names a local `<hash>.tar.gz`/`<hash>.meta.json` pair names a remote object.
* Cache read/write is isolated behind a single storage boundary in the runner, so a remote backend slots in behind the same interface as the local filesystem store.
* Local-first remains the default and the guarantee: Lattice Cloud is never required, is always explicitly enabled, and a repo with it disabled behaves exactly as it does today.

## **6. CLI Output & Logging Management**

Lattice has exactly two output modes, and it chooses between them automatically. The **interactive TUI** is the default for humans; **raw output** is the fallback for machines and for anyone who explicitly wants the unadorned stream.

### **6.1 Interactive TUI (default)**

The default local experience is a full interactive TUI: concurrent tasks are tracked live with per-task progress, spinners, and status, and long log lines are truncated or collapsed into clean, expandable sections.

### **6.2 Raw Output Mode**

Lattice drops the TUI entirely and streams plain, sequential, line-by-line child process output — exactly what each task prints, in order — whenever **either** of these is true:

* **CI / no TTY:** Lattice does not detect a standard interactive terminal (e.g. running in CI, piped, or redirected). Detection is automatic.
* **`--loquacious` / `-l`:** the developer explicitly asks for raw output. In addition to child output, loquacious surfaces the detailed line-by-line flow of environment evaluation, hashing details, and cache reads — the full trace, no interactive screens.

The two triggers produce the same non-interactive stream; `-l` simply forces it on a machine that would otherwise render the TUI.

## **7. Toolchain & Custom Engine Validation**

Lattice checks that host machines are running the appropriate interpreters or compilers before spawning a workspace's build environment. Engines are a per-workspace setting (§2.1): each workspace declares the toolchain(s) it needs and their version constraints. For first-class languages an auto workspace has its engine filled in from the detected manifest; declaring `"engines"` explicitly pins or overrides that, and is how non-first-class toolchains are validated. A custom engine maps an exact binary and a CLI version check inline:

```json
{
  "name": "sim-core",
  "path": "packages/sim-core",
  "auto": false,
  "engines": {
    "node": ">=20.0.0",
    "custom": {
      "alpes": {
        "bin": "alp",
        "version": ">=2.6.7",
        "versionCmd": "alp --version"
      }
    }
  }
}
```

**Shared defaults (optional):** A root-level `"engines"` block, if present, is merged into every workspace as defaults — use it for repo-wide constraints (e.g., one Node version everywhere) and to define custom engines once. A workspace's own `"engines"` always takes precedence over the root defaults.

## **8. Bootstrapping & Code Generation**

### **8.1 lattice setup**

This command parses all workspaces concurrently, runs their native dependency installers (e.g., `cargo fetch`, `npm install`, or `uv sync`), and tracks modification times on project lockfiles to skip dependency runs if nothing has changed.

### **8.2 Templates & Code Generators**

Custom templates are checked directly into `./.lattice/templates/`. Running `lattice generate <template>` reads these files to scaffold new workspaces inside the monorepo.

Optionally, teams can add community-built templates from any text/repo endpoint they choose. Lattice does not require or enforce any specific source; a Git host and a plain CDN are both supported:

```bash
# Add from a Git host
lattice template add github.com/lattice-community/go-gin-boilerplate

# Or from any other endpoint
lattice template add cdn.amazonaws.com/rust-service-template

# Sources are opt-in; Lattice is not reliant on GitHub and can parse any text/repo files.
```

## **9. Distribution, Version Pinning, & Local Installs**

Lattice operates with a strictly zero-global footprint. The version a repository runs on is pinned by the same plain-text config it already checks in — no separate registry or global install to drift out of sync.

1. **The pin lives in the repo.** The authoritative version is the `"latticeVersion"` field already present in the root `lattice.json`. Because it is plain JSON sitting at a known path, the bootstrap script can resolve the target version *before* a binary exists — it simply reads `"latticeVersion"` out of the local `lattice.json`. There is no chicken-and-egg problem: the file the team commits *is* the lockfile.

2. Developers download the native Rust executable via a lightweight, standard bootstrap shell script run from the repo root:
   ```bash
   curl -fsSL https://{url} | sh
   ```

3. The script reads `"latticeVersion"` from `./lattice.json`, downloads the matching pre-compiled binary from the public GitHub Release artifacts, and saves it to a version-stamped path:
   ```bash
   ./.lattice/bin/lattice-<version>
   ```
   A stable `./.lattice/bin/lattice` symlink is pointed at the resolved version. Keeping the versioned binary on disk means switching branches that pin different versions is a symlink swap, not a re-download.

4. **Self-healing drift check.** On every invocation the binary compares its own version against `"latticeVersion"` in the nearest `lattice.json`. On a mismatch it prints a single-line notice and transparently re-bootstraps the pinned version before proceeding, so a developer who switches branches never silently runs the wrong build.

5. Developers execute commands using the local path `./.lattice/bin/lattice`. To uninstall Lattice completely from a machine, developers simply delete the folder using `rm -rf .lattice`.

## **10. Core Rust Crate Specifications**

Lattice is a Cargo workspace split into focused library crates driven by a thin binary. This separation is what keeps the Lattice Cloud storage seam (§5.3) and the TUI (§6) cleanly swappable.

* **`lattice`** — the binary: clap-based CLI, command dispatch, and the interactive TUI shell.
* **`lattice-config`** — `lattice.json` parsing, the workspace/engine/pipeline schema, and root resolution.
* **`lattice-workspace`** — workspace loading, first-class language detection, and task-command inference.
* **`lattice-dag`** — DAG construction, topological ordering, and persistent-node validation.
* **`lattice-cache`** — hashing, the local content-addressed store, and the storage boundary remote caching will extend.
* **`lattice-runner`** — the async task executor that orchestrates parallel child processes.
* **`lattice-output`** — terminal rendering, interactive vs. CI vs. loquacious modes.

Key third-party stack: **clap (v4)** for CLI parsing; **tokio** for the async runner; **petgraph** for the DAG; **serde + serde_json** for config and cache metadata; **ignore** for fast, gitignore-aware file crawling during hashing; **tar + flate2** for local output archives; and **indicatif** (with supporting terminal crates) for the interactive multi-progress TUI.

## **11. Complete Root lattice.json Example**

The following is a complete `lattice.json` showing auto-detected first-class workspaces alongside manual ones — a Rust API and a JS web app (auto), a bespoke C++ utility and an existing Turborepo monorepo (manual) — wired into one clean build hierarchy:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0",
  "workspaces": [
    {
      "path": "apps/web"
    },
    {
      "name": "rust-api",
      "path": "apps/api"
    },
    {
      "name": "legacy-cpp-utility",
      "path": "tools/legacy-cpp",
      "auto": false,
      "engines": {
        "custom": {
          "gcc": {
            "bin": "g++",
            "version": ">=13.0.0",
            "versionCmd": "g++ --version"
          }
        }
      },
      "tasks": {
        "build": "g++ -O3 main.cpp -o dist/util",
        "test": "./dist/util --run-tests"
      }
    },
    {
      "name": "sub-turborepo",
      "path": "vendor/sub-turborepo",
      "auto": false,
      "engines": { "node": ">=20.0.0" },
      "dependsOn": ["legacy-cpp-utility"],
      "tasks": {
        "build": "npx turbo run build",
        "test": "npx turbo run test"
      }
    }
  ],
  "engines": {
    "rust": ">=1.75.0",
    "node": ">=20.0.0",
    "custom": {
      "uv": {
        "bin": "uv",
        "version": ">=0.1.0",
        "versionCmd": "uv --version"
      }
    }
  },
  "pipeline": {
    "setup": {
      "outputs": []
    },
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*"],
      "ignore": ["**/*.test.*", "**/README.md"],
      "outputs": ["dist/**", "target/release/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["src/**/*", "tests/**/*"],
      "env": ["DATABASE_URL"]
    },
    "dev": {
      "dependsOn": ["build"],
      "persistent": true
    }
  },
  "maxCacheSize": "10GB",
  "logging": "default"
}
```

> Note: `"inputs"`, `"outputs"`, and `"ignore"` inside `pipeline` are file globs used for hashing and artifact capture — these are the *only* place glob patterns appear. Workspace `"path"` values are always literal directories (§2.1).

## **12. Roadmap & Release Milestones**

Lattice is built engine-first — the graph, execution, caching, and config model are hardened first — but **brand, design, and the web presence run as an early parallel track**, not a final afterthought. Each milestone is a coherent, shippable increment.

### **v0.1 — Core Engine & Self-Hosting**
The correct engine under the new config model, dogfooded by this very repo.
* Unified per-workspace config (`workspaces` array of objects, explicit `path`, `auto`/manual, per-workspace `engines`/`dependsOn`/`tasks`); `projects` key removed.
* Explicit-workspace resolution with strict-failure validation (§2.1).
* Parallel task execution across the DAG.
* Local content-addressed caching correctness (§5.1–5.2).
* Full interactive TUI with automatic raw-output fallback (§6).
* Self-hosting: the Lattice repo's own `lattice.json` migrated to the new model, plus a one-command dev-binary hotswap (§13).

### **v0.2 — Ecosystems, Correctness & Brand Foundations**
Breadth and rigor across the matrix — and Lattice starts to look like a product.
* All nine first-class ecosystems verified (§3), core tier deeply tested.
* Toolchain/engine version validation (§7).
* Persistent dev mode (§4.1).
* Cache `ignore` globs, `meta.json` schema conformance, and LRU pruning bounded by `maxCacheSize` (§5).
* Structured, actionable error messages.
* Per-crate unit tests, end-to-end CLI tests, and CI (fmt + clippy + test).
* **Brand foundations:** logo, visual identity, product messaging/voice, and branded CLI help/banner.

### **v0.3 — Developer Experience & Website**
Getting into and around a Lattice repo — and a public place to learn about it.
* `lattice init` scaffolding.
* Templates and generators, including non-GitHub HTTP/CDN sources (§8.2).
* Shell completions (bash/zsh/fish/pwsh).
* Turborepo-as-a-workspace: no engine code — it is simply a manual workspace (`auto: false`) whose `tasks` shell out to `turbo run <task>`. Delivered as a documented pattern, a worked example, and a passthrough integration test (not a DAG feature).
* **Website:** landing page and documentation-site infrastructure live, seeded with the core docs.

### **v1.0 — Distribution & Trust**
Something a stranger can install and rely on.
* `curl | sh` bootstrap installer, cross-platform release binaries, and `lattice upgrade` (§9).
* `latticeVersion` pinning and self-healing drift check (§9).
* A real OSS license and the core user documentation set.
* Community health files (README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, issue/PR templates) and AI-assisted contribution guidelines.

### **Launch — Public Launch**
* Polished site + docs, announcement, and go-to-market. (Brand and website already exist from v0.2/v0.3; this milestone is the coordinated public push, not first-time creation.)

### **Future — Lattice Cloud**
* Opt-in, paid remote/shared cache built on the storage seam from §5.3.

## **13. Self-Hosting & Dogfooding**

Lattice manages Lattice. This repository is itself a Lattice monorepo: the root `lattice.json` declares the `crates/*` as explicit workspaces (§2.1) and every build/test/lint task is run through Lattice. Dogfooding is a first-class requirement — if a workflow is awkward for the maintainers running Lattice on Lattice, it is a bug.

Because Lattice is the tool *and* the code under development, contributors constantly need to swap between a stable pinned binary and a freshly built dev binary. This is deliberately a **symlink swap, not a reinstall**, reusing the distribution model from §9:

* `./.lattice/bin/lattice` is the symlink every command resolves.
* A single command — e.g. `lattice dev-link` (with a plain `scripts/` fallback for bootstrapping before a binary exists) — runs `cargo build` and repoints that symlink at the freshly compiled `target/debug/lattice`.
* Restoring the pinned release is the same swap in reverse (`lattice dev-unlink`), pointing the symlink back at the `latticeVersion`-stamped binary.

The hotswap must be fast and non-destructive: switching to the dev binary, running the suite, and switching back should never re-download or corrupt the pinned install, and the currently-linked build should always be obvious from `lattice version`.
