---
title: Caching
description: How Lattice decides what to skip.
group: Guides
order: 3
---

# Caching

When a task runs, Lattice records its inputs and its result. Next time, if those
inputs haven't changed, it reuses the stored result instead of running the task
again.

> This page is being written. The short version: the cache is local and on your
> disk, so nothing leaves your machine and no network is required.
