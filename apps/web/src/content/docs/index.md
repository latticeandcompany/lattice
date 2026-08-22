---
title: Introduction
description: What Lattice is, and where to go for tutorials, guides, concepts, and reference.
group: Overview
order: 1
---

# Introduction

**A high-performance, local toolchain for managing monorepos.**

Lattice is one CLI that runs tasks across the workspaces of a monorepo and pins
the tool versions those tasks run with. Both halves read one `lattice.json` at
the repo root, and either half works without the other. A workspace with no
`engines` still runs and caches tasks. A workspace with no tasks of its own can
still pin a tool version.

## Start here

These pages get you from an empty repo to a cached run.

- [Installation](/lattice/docs/installation) gets the `lattice` binary onto
  your machine and shows what a pinned version does.
- [Getting started](/lattice/docs/getting-started) runs `lattice init`, then
  `lattice run build` twice, so the second run comes back from cache.
- [Upgrading](/lattice/docs/upgrading) covers moving a repo to a new Lattice
  version.

## Do one thing in your own repo

Each guide solves one problem and assumes you already have a repo.

- [Adopting Lattice](/lattice/docs/adopting-lattice) declares an existing
  monorepo one workspace at a time.
- [A multi-language monorepo](/lattice/docs/multi-language-monorepo) wires
  JavaScript, Rust, and Python workspaces into one task graph.
- [Nested repos](/lattice/docs/nested-repos) covers a repo that contains
  another repo with its own `lattice.json`.
- [Pinning tool versions](/lattice/docs/pinning-tool-versions) constrains and
  provisions a compiler, linter, or package manager.
- [Dev servers and watchers](/lattice/docs/dev-servers) runs processes that
  never exit.
- [Continuous integration](/lattice/docs/continuous-integration) wires
  `lattice run` into a CI job and shares the cache.
- [Troubleshooting](/lattice/docs/troubleshooting) works backward from a
  symptom.
- [The desktop app](/lattice/docs/desktop-app) opens a project in a window
  instead of a terminal.

## Understand how it decides

These pages explain the models behind the behavior. Read them when the output
surprises you.

- [Workspaces](/lattice/docs/workspaces) is the unit everything else is scoped
  to.
- [Task graph](/lattice/docs/task-graph) covers `dependsOn`, the `^` prefix,
  and what runs in parallel.
- [Caching](/lattice/docs/caching) covers what makes a task hit or miss.
- [Driver detection](/lattice/docs/drivers) covers how Lattice picks the tool
  that runs a workspace's tasks, and when it stops to ask.
- [Engines and provisioning](/lattice/docs/engines) covers the three things a
  version constraint can mean.
- [Selecting what runs](/lattice/docs/filtering) covers `--filter` and stacked
  tasks.
- [Persistent tasks](/lattice/docs/persistent-tasks) covers tasks that run
  until you stop them.
- [Output and logging](/lattice/docs/output-modes) covers the interactive
  display and the raw stream.

## Look something up

- [Configuration](/lattice/docs/configuration) is the field reference for
  `lattice.json`.
- [CLI reference](/lattice/docs/cli) is every command, flag, and exit code.
- [Toolchains](/lattice/docs/toolchains) is the built-in driver table and the
  well-known engine list.
- [Environment variables](/lattice/docs/environment-variables) is what Lattice
  reads from the environment and what it sets for a task.
- [Errors](/lattice/docs/errors) is every error message and what causes it.
- [Cache internals](/lattice/docs/cache-internals) is the exact key
  composition and on-disk layout.
- [Architecture](/lattice/docs/architecture) is the crate layout, for
  contributors.
- [Glossary](/lattice/docs/glossary) defines every term these pages use.
- [Changelog](/lattice/docs/changelog) is what changed in each release.
