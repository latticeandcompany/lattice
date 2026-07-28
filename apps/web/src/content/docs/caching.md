---
title: Caching
description: How Lattice decides what to skip.
group: Concepts
order: 2
---

# Caching

When a task runs, Lattice hashes the inputs you declared for it and records the
result. Next time, if the hash matches, it restores the recorded outputs and skips
the command. The store is a directory under `.lattice` in the repo.

> This page is being written.
