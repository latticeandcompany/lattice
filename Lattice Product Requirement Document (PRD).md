# **Lattice Product Requirement Document (PRD)**

## **Specification**

Lattice is a local-first, high-performance, and language-agnostic monorepo build system written in Rust. Heavily inspired by Turborepo, Lattice is built by and for Turborepo fans to extend the speed and simplicity of modern build graphs to polyglot codebases.  
Lattice provides robust dependency graph resolution, parallel task execution, local-first caching, and custom toolchain isolation, completely independent of external CDNs, URL imports, or global runtime configurations.

## **1\. Executive Summary & Design Philosophy**

Lattice is built on a foundational philosophy that prioritizes developer autonomy, local environment control, and clear execution flows.

### **1.1 Core Principles**

* **Local-First & Inspectable:** If a developer cannot cd into an asset, read its configuration in plain text, and delete it with a simple rm \-rf, Lattice does not use it. There are no hidden databases or globally scoped configurations. This governs Lattice's *runtime*: builds, caching, and graph resolution never depend on a network or remote service. Convenience actions that are explicitly opt-in and developer-initiated — installing the binary (§9) or adding a template from a URL (§8.2) — may reach the network, but Lattice never forces or requires a remote source, and everything they fetch lands as plain, inspectable files under ./.lattice/.  
* **Root-Level Centralized Management:** To prevent configuration scattering across sprawling directories, all custom workspace task commands, graph overrides, and settings live strictly inside a single, centralized root-level lattice.json file.  
* **Turborepo Synergy:** Lattice is not designed to replace Turborepo. It is built to seamlessly coexist with it. Developers can easily configure a sub-monorepo managed by Turborepo as a single workspace inside the broader Lattice parent graph.

## **2\. Project Discovery & Dependency Graph Construction**

Lattice maps polyglot codebases by parsing registered directories to build a Directed Acyclic Graph (DAG) of project tasks.

### **2.1 Workspace Resolution**

A Lattice monorepo is made up of **workspaces**. A workspace is a folder of files — its contents are identified by a glob — and each entry in the root "workspaces" list is that workspace's block of per-project settings. There is no separate projects key; naming, auto-detection, engines, dependencies, and task overrides are all configured per workspace, right here:

* **"glob"** (string, required): a path or glob pattern, relative to the repo root, identifying the folder of files that makes up the workspace. A plain path such as "tools/legacy-cpp" is one workspace; a pattern such as "apps/\*" is a convenience that expands to one workspace per matched directory, each inheriting the entry's settings.  
* **"name"** (string): the workspace name, used for "dependsOn" references and overrides. Defaults to the matched directory's basename.  
* **"auto"** (boolean, default true): when true, Lattice infers this workspace's engine and task commands from its first-class native manifest (§2.2, §3). When false, Lattice performs no inference and uses only what this entry declares — this is how non-first-class or bespoke projects (a hand-rolled C++ build, a wrapped Turborepo) are wired in.  
* **"engines"** (map): the toolchain(s) this workspace requires, with version constraints, validated before it builds (§7). When "auto" is true Lattice fills this in from the detected manifest; declaring it pins or overrides the inferred toolchain, and is required for custom engines on manual workspaces.  
* **"dependsOn"** (array): other workspaces this one depends on.  
* **"tasks"** (map): explicit task-name → command overrides.

A bare entry (just a "glob", auto by default) needs nothing else — Lattice discovers the folder, detects its toolchain, and infers its tasks. Auto and manual workspaces coexist in this one list.

**Overlap precedence:** If a directory is matched by more than one entry, the most specific entry wins — an explicit single-directory entry always takes precedence over a broad glob, so a manual workspace can override one that would otherwise be auto-detected.

**Strict failure:** If an entry's "glob" matches no directory, or a workspace is "auto": false with no "tasks" for a requested task, Lattice halts and reports the offending workspace by name.

### **2.2 Task Resolution Hierarchy for Non-First Class Languages**

Lattice does not perform "no-op" task executions or guess commands for custom or unsupported language directories. It resolves what script to execute for a task using a strict, centralized priority chain:

1. **Root-Level Project Overrides:** Lattice checks the workspaces list in the root lattice.json for an object entry matching the workspace name that defines a command for the task.  
2. **First-Class Ecosystem Inference:** If no root override exists, Lattice scans the workspace directory for recognized native manifests (such as package.json, Cargo.toml, or go.mod) and infers the correct toolchain script.  
3. **Strict Failure Safety:** If no override is declared in the root lattice.json and the folder does not match a first-class ecosystem, Lattice immediately halts execution and prints an error pointing to the exact workspace.

## **3\. First-Class Ecosystem Support Matrix**

Lattice provides native parser logic to extract workspace boundaries and manage internal dependency graphs for the following configurations:

| Language | Package Manager | Native Manifest File | Default Task Command |
| :---- | :---- | :---- | :---- |
| **JavaScript / TypeScript** | npm / pnpm / yarn / bun | package.json | Runs package script (e.g., bun run \<task\>) |
| **Python** | poetry / pip / uv | pyproject.toml | Runs project task configuration |
| **Rust** | cargo | Cargo.toml | Executes corresponding cargo flag (cargo \<task\>) |
| **Go** | go mod | go.mod | Runs native go command (go \<task\>) |
| **Ruby** | bundler / gem | Rakefile / Gemfile | Invokes rake \<task\> |
| **C\# / .NET** | NuGet | .csproj / project.json | Runs dotnet \<task\> commands |
| **Java (Gradle)** | Gradle | build.gradle | Invokes gradle wrapper (./gradlew \<task\>) |
| **Java (Maven)** | Maven | pom.xml | Invokes maven wrapper (./mvnw \<task\>) or system mvn \<task\> |
| **C / C++** | Make | Makefile | Invokes make \<task\> targets. Projects without a Makefile (e.g., raw CMake or bespoke builds) are not auto-inferred and must be wired in as a manual workspace ("auto": false) with an explicit "tasks" map. |

## **4\. Task Execution Engine & Parallel Pipeline**

Lattice schedules pipeline actions across the DAG with non-blocking concurrency.

┌────────────────────────┐  
│ 1\. Build Topo Order    │ ──► Milliseconds. Generates execution sequence.  
└────────────────────────┘  
            │  
            ▼  
┌────────────────────────┐  
│ 2\. Assess Cache Status │ ──► Under 10ms. Matches unique hash fingerprint.  
└────────────────────────┘  
            │  
            ▼  
┌────────────────────────┐  
│ 3\. Process Execution   │ ──► Uncached tasks run concurrently off-thread.  
└────────────────────────┘

### **4.1 Persistent Dev Mode**

For interactive services that do not terminate (such as development web servers), developers can tag the pipeline definition with "persistent": true in the root lattice.json.

* Once a persistent task boots, Lattice keeps the process active in the background.  
* **DAG Constraint:** Because persistent tasks run indefinitely and never exit, they cannot be depended on by other tasks in the pipeline. Lattice validates this during DAG construction and will refuse to execute pipelines that place downstream dependencies on persistent nodes.

## **5\. Caching, Hashing, & Local-First Isolation**

### **5.1 Smart Hashing**

Lattice computes task hashes using the content of matched workspace files, registered environment variables, the command string, and native lockfile states. Files that should not trigger rebuilds are managed via the "ignore" key in the root lattice.json pipeline configuration.

### **5.2 Local Inspectable Cache**

All cache outputs are written to the local directory under ./.lattice/cache/. Each cached run creates up to two files:

* \<hash\>.tar.gz: The zipped output directory specified by the "outputs" configuration. Tasks that declare no "outputs" produce no archive; only the descriptor is written.  
* \<hash\>.meta.json: A completely transparent JSON descriptor containing basic execution metrics, target workspace properties, and execution durations.

Persistent tasks and any task marked "cache": false are never cached and write neither file.

JSON  
{  
  "hash": "8f3a9d2c1e",  
  "task": "build",  
  "workspace": "rust-api",  
  "durationMs": 1140,  
  "lastUsed": "2026-07-16T22:04:10Z",  
  "env": {  
    "CARGO\_PROFILE": "release"  
  }  
}

Lattice manages disk space automatically using a Least Recently Used (LRU) pruning strategy bounded by the "maxCacheSize" value defined in the root lattice.json.

## **6\. CLI Output & Logging Management**

### **6.1 Interactive vs. CI Log States**

* **Local Mode:** Lattice utilizes an interactive terminal UI with clean, high-contrast text layout. Long log lines are truncated or collapsed into clean, expandable sections.  
* **CI Mode:** Lattice automatically falls back to a clean, non-interactive, sequential plain-text stream when a standard TTY terminal is not detected.  
* **Loquacious Mode:** Developers can run with \--loquacious (or \-l) to bypass interactive terminal screens entirely and stream a detailed, line-by-line flow of environment evaluation steps, hashing details, cache reads, and child outputs.

## **7\. Toolchain & Custom Engine Validation**

Lattice checks that host machines are running the appropriate interpreters or compilers before spawning a workspace's build environment. Engines are a per-workspace setting (§2.1): each workspace declares the toolchain(s) it needs and their version constraints. For first-class languages an auto workspace has its engine filled in from the detected manifest; declaring "engines" explicitly pins or overrides that, and is how non-first-class toolchains are validated. A custom engine maps an exact binary and a CLI version check inline:

JSON  
{  
  "name": "sim-core",  
  "glob": "packages/sim-core",  
  "auto": false,  
  "engines": {  
    "node": "\>=20.0.0",  
    "custom": {  
      "alpes": {  
        "bin": "alp",  
        "version": "\>=2.6.7",  
        "versionCmd": "alp \--version"  
      }  
    }  
  }  
}

**Shared defaults (optional):** A root-level "engines" block, if present, is merged into every workspace as defaults — use it for repo-wide constraints (e.g., one Node version everywhere) and to define custom engines once. A workspace's own "engines" always takes precedence over the root defaults.

## **8\. Bootstrapping & Code Generation**

### **8.1 lattice setup**

This command parses all workspaces concurrently, runs their native dependency installers (e.g., cargo fetch, npm install, or uv sync), and tracks modification times on project lockfiles to skip dependency runs if nothing has changed.

### **8.2 Templates & Code Generators**

Custom templates are checked directly into ./.lattice/templates/. Running lattice generate \<template\> reads these files to scaffold new workspaces inside the monorepo.

Optionally, teams can add community-built templates from any text/repo endpoint they choose. Lattice does not require or enforce any specific source; a Git host and a plain CDN are both supported:

Bash  
\# Add from a Git host  
lattice template add github.com/lattice-community/go-gin-boilerplate

\# Or from any other endpoint  
lattice template add cdn.amazonaws.com/rust-service-template

\# Sources are opt-in; Lattice is not reliant on GitHub and can parse any text/repo files.

## **9\. Distribution, Version Pinning, & Local Installs**

Lattice operates with a strictly zero-global footprint. The version a repository runs on is pinned by the same plain-text config it already checks in — no separate registry or global install to drift out of sync.

1. **The pin lives in the repo.** The authoritative version is the "latticeVersion" field already present in the root lattice.json. Because it is plain JSON sitting at a known path, the bootstrap script can resolve the target version *before* a binary exists — it simply reads "latticeVersion" out of the local lattice.json. There is no chicken-and-egg problem: the file the team commits *is* the lockfile.

2. Developers download the native Rust executable via a lightweight, standard bootstrap shell script run from the repo root:  
   Bash  
   curl \-fsSL https://{url} | sh

3. The script reads "latticeVersion" from ./lattice.json, downloads the matching pre-compiled binary from the public GitHub Release artifacts, and saves it to a version-stamped path:  
   Bash  
   ./.lattice/bin/lattice-\<version\>  
   
   A stable ./.lattice/bin/lattice symlink is pointed at the resolved version. Keeping the versioned binary on disk means switching branches that pin different versions is a symlink swap, not a re-download.

4. **Self-healing drift check.** On every invocation the binary compares its own version against "latticeVersion" in the nearest lattice.json. On a mismatch it prints a single-line notice and transparently re-bootstraps the pinned version before proceeding, so a developer who switches branches never silently runs the wrong build.

5. Developers execute commands using the local path ./.lattice/bin/lattice. To uninstall Lattice completely from a machine, developers simply delete the folder using rm \-rf .lattice.

## **10\. Core Rust Crate Specifications**

For the developers writing Lattice, the CLI toolchain utilizes this core library stack:

* **clap (v4):** Terminal parsing and help generation for filtering and verbosity flags.  
* **tokio:** High-performance async runner to orchestrate parallel child processes.  
* **petgraph:** Mathematical representation to construct and resolve the workspace DAG.  
* **glob:** Path-pattern matching used to expand each workspace's "glob" during discovery.  
* **ignore:** Fast, multithreaded directory crawler that automatically respects local .gitignore rules.  
* **tar \+ flate2:** Native file compression to zip and unzip outputs locally.  
* **serde \+ serde\_json:** Ultra-fast parsing of the central configuration and cache metadata files.  
* **indicatif:** Multi-progress bar render support for concurrent terminal tracking.

## **11\. Complete Root lattice.json & Turborepo Wiring Example**

The following is a complete, production-ready lattice.json file. It showcases how a Python application, a C++ project, a Go microservice, and an existing Turborepo monorepo are wired together into a unified, clean build hierarchy:

JSON  
{  
  "$schema": ".lattice/schema.json",  
  "latticeVersion": "1.0.0",  
  "workspaces": \[  
    {  
      "name": "legacy-cpp-utility",  
      "glob": "tools/legacy-cpp",  
      "auto": false,  
      "engines": {  
        "custom": {  
          "gcc": {  
            "bin": "g++",  
            "version": "\>=13.0.0",  
            "versionCmd": "g++ \--version"  
          }  
        }  
      },  
      "tasks": {  
        "build": "g++ \-O3 main.cpp \-o dist/util",  
        "test": "./dist/util \--run-tests"  
      }  
    },  
    {  
      "name": "sub-turborepo",  
      "glob": "vendor/sub-turborepo",  
      "auto": false,  
      "engines": { "node": "\>=20.0.0" },  
      "dependsOn": \["legacy-cpp-utility"\],  
      "tasks": {  
        "build": "npx turbo run build",  
        "test": "npx turbo run test"  
      }  
    }  
  \],  
  "engines": {  
    "rust": "\>=1.75.0",  
    "node": "\>=20.0.0",  
    "custom": {  
      "uv": {  
        "bin": "uv",  
        "version": "\>=0.1.0",  
        "versionCmd": "uv \--version"  
      }  
    }  
  },  
  "pipeline": {  
    "setup": {  
      "outputs": \[\]  
    },  
    "build": {  
      "dependsOn": \["^build"\],  
      "inputs": \["src/\*\*/\*"\],  
      "ignore": \["\*\*/\*.test.\*", "\*\*/README.md"\],  
      "outputs": \["dist/\*\*", "target/release/\*\*"\]  
    },  
    "test": {  
      "dependsOn": \["build"\],  
      "inputs": \["src/\*\*/\*", "tests/\*\*/\*"\],  
      "env": \["DATABASE\_URL"\]  
    },  
    "dev": {  
      "dependsOn": \["build"\],  
      "persistent": true  
    }  
  },  
  "maxCacheSize": "10GB",  
  "logging": "default"  
}  
