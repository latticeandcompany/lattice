---
title: Introduction
description: A fast, local toolchain for managing monorepos.
group: Overview
order: 1
---

# Introduction

Lattice is a fast, local toolchain for managing monorepos. Point it at a repo with
projects in more than one language, and it builds, tests, and runs them for you with
one command.

Because it caches every result, a second build only redoes the parts that changed.
And because everything runs on your machine, you never wait on a remote service to
get work done.

## What you get

- **Builds that finish sooner.** Unchanged work comes back from cache instead of
  running again.
- **One command for the whole repo.** Every project builds the same way, whatever
  language it's written in.
- **The same result everywhere.** Lattice pins each project's tools, so a fresh
  clone builds the same on your machine and in CI.

## Next steps

- [Getting started](/docs/getting-started): install Lattice and run your first build.
- [Toolchains](/docs/toolchains): the languages and tools Lattice can drive.
- [Configuration](/docs/configuration): what goes in `lattice.json`.
