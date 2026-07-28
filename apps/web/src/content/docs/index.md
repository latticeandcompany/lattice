---
title: Introduction
description: A high-performance, local toolchain for managing monorepos.
group: Overview
order: 1
---

# Introduction

Lattice is a high-performance, local toolchain for managing monorepos. One `lattice.json` at the
repo root lists your projects and the tasks that run in them. From there, `lattice run`
orders the work, runs each project's own build command, and records the result so the
next run can skip it.

## Next steps

- [Getting started](/lattice/docs/getting-started): install Lattice and run your first task.
- [Configuration](/lattice/docs/configuration): what goes in `lattice.json`.
- [Task graph](/lattice/docs/task-graph): how tasks are ordered and parallelized.
- [Caching](/lattice/docs/caching): what gets hashed and what gets skipped.
- [Toolchains](/lattice/docs/toolchains): the languages Lattice can build.
