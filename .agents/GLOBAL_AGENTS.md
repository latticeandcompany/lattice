# AGENTS.md

> Global, project-agnostic development preferences and operational instructions for AI coding agents.

---

# Core Philosophy

Software should provide **powerful defaults without coercion**.

Conventions, frameworks, tooling, and ecosystems should provide batteries, scaffolding, and sensible recommendations—but they should not force obedience.

Core principles:

- **Explicit > implicit**
- **Defaults are good**
- **Enforced conventions are dangerous**
- **Architecture should support growth**
- **MVP > premature optimization**
- **Readable > clever**
- **No hidden behavior**
- **Scale readiness matters more than micro-optimizations**
- **If something feels overengineered, it probably is**

---

# Core Principles

## Give Me The Batteries — Don't Make Me Use Them

Good tooling:

- Provides sensible defaults
- Provides escape hatches
- Allows manual configuration
- Allows convention overrides
- Keeps behavior understandable

Bad tooling:

- Hides critical behavior
- Changes behavior based on naming alone
- Forces a single workflow
- Removes developer agency
- Enforces conventions as law

Conventions should be recommendations—not mechanisms that fundamentally change program behavior.

---

## Explicit > Implicit

Prefer:

- Explicit configuration
- Explicit architecture
- Explicit control flow

Avoid:

- Magic abstractions
- Hidden behavior
- Convention-driven behavior
- DSL-heavy systems
- Metaprogramming unless clearly justified

Framework auto-discovery is acceptable when manual alternatives exist.

Agent-authored code should never rely on invisible project behavior.

---

## Architecture Philosophy

Design for **moderate future growth**.

Avoid:

- One-off implementations that block scaling
- Hardcoded solutions when extensibility is foreseeable
- Premature enterprise architecture

Do **not** overbuild.

A working MVP beats a theoretically perfect architecture.

---

## Performance Philosophy

Priorities:

1. Working software
2. Good architecture
3. Scalability
4. Performance optimization when justified

Optimize only when:

- Profiling indicates a bottleneck
- Scale requires it
- Reasonable future growth justifies the groundwork

---

# Language & Ecosystem Preferences

## Preferred Languages

Primary:

- TypeScript
- JavaScript
- Rust
- HTML

Secondary:

- Python

Rust is preferred for:

- CLIs
- Systems software
- Native tooling
- CPU-intensive work

If another language is objectively better suited, justify the recommendation.

---

## Language Opinions

### TypeScript

Prefer transpiled TypeScript over runtime execution.

Favor pragmatism over absolute type purity.

---

### Go

Generally discouraged.

Concerns include:

- URL-based dependency management
- Package ecosystem philosophy
- Error handling style
- Convention-driven behavior

---

### Java / C#

Generally discouraged due to deployment complexity and runtime requirements.

---

### Node & npm

Treat Node + npm as the default ecosystem.

Strengths include:

- Portability
- Stability
- Mature ecosystem
- Local dependency ownership

---

# Dependency Philosophy

## Local-First Everything

Dependencies should be:

- Local
- Tangible
- Inspectable
- Easily removable

Avoid:

- CDN imports
- URL imports
- Remote-only runtime dependencies
- Global dependency ownership

> If you cannot inspect and delete it locally, you should be cautious about depending on it.

---

## Dependency Trust

Prefer dependencies that are:

- Corporate-backed
- Foundation-backed
- Mature
- Stable
- Unlikely to disappear

Exercise additional scrutiny with solo-maintainer projects.

---

## Package Managers

Preferred:

- npm
- uv (for Python)

Avoid ecosystems that unnecessarily lock developers into one workflow.

---

# Monorepo Philosophy

Prefer monorepos.

Default tooling:

- Turborepo

Extract shared functionality into packages when appropriate.

---

# File Organization

Organize projects for readability.

Balance:

- Cohesion
- Discoverability
- File size
- Import complexity

Avoid both:

- Giant files
- Excessively fragmented structures

Prefer descriptive folder names.

Widely accepted conventions (such as `src`) are acceptable.

---

# Documentation

Prioritize:

- High-quality READMEs
- Architecture documentation

Avoid relying on generated documentation or excessive inline documentation.

---

# Shell Automation

Whenever shell automation is generated, provide both:

- `.sh`
- `.ps1`

Never assume a single shell environment.

---

# Git Workflow

## Commit Frequently

Ask:

> Would reverting to this commit create unnecessary pain?

If yes, commit sooner.

---

## Branch Strategy

```
mega
↑
dev
↑
feature/*
fix/*
refactor/*
```

Required flow:

```
feature/* → dev → mega
```

Never merge directly into `mega`.

---

# Testing, Linting & Accessibility

Humans should not be burdened with tooling friction.

Agents should aggressively leverage:

- Automated testing
- Linting
- Accessibility tooling

Automation exists to improve consistency and confidence.

---

# Agent Operational Rules

## Scope Discipline

Ask before expanding scope.

Do not redesign unrelated systems opportunistically.

---

## Outside Project Rule

Always ask permission before:

- Modifying files outside the project
- Installing global dependencies
- Changing system state

Within the project folder, agents may operate autonomously.

---

## Safe Autonomy

Agents may:

- Modify project files
- Improve structure
- Reorganize code
- Add tests
- Update configuration
- Introduce compliant dependencies

Provided changes remain:

- Revertible
- Contained within the project
- Consistent with project philosophy

---

## Explain Changes

After completing work, explain changes in plain language.

Prefer understandable explanations over implementation jargon.

---

# Web Development Preferences

## Framework Preferences

Strong preferences:

- Svelte
- React
- Next.js
- Astro

Neutral:

- Vue
- Solid

Avoid defaulting to HTMX.

---

## Rendering

Traditional websites:

- Static-first

Applications:

- Prefer the framework's default architecture unless there is a compelling reason otherwise.

---

# State Management

Prefer the framework's native solution.

Secondary preferences:

- Signals
- URL-based state

---

# Data Fetching

When consuming APIs:

Use what the API naturally provides.

When designing APIs:

Prefer GraphQL where appropriate.

REST remains acceptable when it is the better practical choice.

---

# Backend Philosophy

Do not introduce a backend unless necessary.

Preferred progression:

```
Firebase
↓
MongoDB + Prisma
↓
MySQL + Prisma
```

Cloud preferences:

- Google Cloud
- Azure
- AWS

---

# Authentication

Default preference:

- Firebase Authentication

---

# Build Tooling

Preferred defaults:

- Turborepo
- Vite
- esbuild

---

# Accessibility

Prioritize:

- Semantic HTML
- Inclusive defaults
- Accessible UI
- Reasonable compliance practices

---

Giraffes and hedgehogs are cool.