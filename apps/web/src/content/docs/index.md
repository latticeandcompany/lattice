---
title: Introduction
description: What Lattice is, and where to go for tutorials, guides, concepts, and reference.
group: Overview
order: 1
---

# Introduction

**A high-performance, local toolchain for managing monorepos.**

Lattice is one CLI that runs tasks across the workspaces of a monorepo and pins
the tool versions those tasks run with. It reads the workspaces, the
dependencies between them and their tasks from the manifests the repo already
has, so a `lattice.json` often holds only a few lines. Either half works without
the other: a repo can run and cache tasks with no pinned versions, or pin
versions for tasks it runs some other way.

## Start here

- [Installation](/lattice/docs/installation) gets the `lattice` binary onto
  your machine.
- [Getting started](/lattice/docs/getting-started) runs `lattice init` in a
  small monorepo, then builds it twice so the second run comes back from cache.
- [Upgrading](/lattice/docs/upgrading) covers moving a repo to a new version,
  including a 1.x config to 2.0 with `lattice migrate`.

## Do one thing in your own repo

- [Adopting Lattice](/lattice/docs/adopting-lattice) adds Lattice to a repo
  that already builds.
- [A multi-language monorepo](/lattice/docs/multi-language-monorepo) runs
  JavaScript, Rust, Python and Go workspaces in one task graph.
- [Nested repos](/lattice/docs/nested-repos) brings in a subtree that has its
  own task runner.
- [Pinning tool versions](/lattice/docs/pinning-tool-versions) checks or
  installs a compiler, runtime, or package manager.
- [Dev servers](/lattice/docs/dev-servers) runs processes that never exit.
- [Continuous integration](/lattice/docs/continuous-integration) wires
  `lattice run` into a CI job and keeps its cache.
- [Troubleshooting](/lattice/docs/troubleshooting) works backward from a
  symptom.
- [The desktop app](/lattice/docs/desktop-app) opens a project in a window
  instead of a terminal.

## Understand how it decides

- [Workspaces](/lattice/docs/workspaces) covers where the list of workspaces,
  their names and their dependencies come from.
- [Task graph](/lattice/docs/task-graph) covers how one task name becomes a
  node per workspace, `dependsOn`, and what runs in parallel.
- [Caching](/lattice/docs/caching) covers what makes a task hit or miss.
- [Driver detection](/lattice/docs/drivers) covers which tool runs a
  workspace's tasks and which tasks it can run.
- [Engines and provisioning](/lattice/docs/engines) covers the three things a
  version pin can mean.
- [Selecting what runs](/lattice/docs/filtering) covers `--filter`, running
  from a subdirectory, and passing arguments.
- [Persistent tasks](/lattice/docs/persistent-tasks) covers tasks that run
  until you stop them.
- [Output and logging](/lattice/docs/output-modes) covers the live display and
  the plain stream.

## Look something up

- [Configuration](/lattice/docs/configuration) is the field reference for
  `lattice.json`.
- [CLI reference](/lattice/docs/cli) is every command, flag, and exit code.
- [Toolchains](/lattice/docs/toolchains) is the driver table, the tasks each
  tool can run, and the well-known engine list.
- [Environment variables](/lattice/docs/environment-variables) is what Lattice
  reads from the environment and what it sets for a task.
- [Errors](/lattice/docs/errors) is every error message and what causes it.
- [Cache internals](/lattice/docs/cache-internals) is the exact key
  composition and on-disk layout.
- [Architecture](/lattice/docs/architecture) is the crate layout, for
  contributors.
- [Glossary](/lattice/docs/glossary) defines every term these pages use.
- [Changelog](/lattice/docs/changelog) is what changed in each release.
