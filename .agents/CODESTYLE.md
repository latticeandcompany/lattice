# CODESTYLE.md

> Project coding standards and style guide. This document defines **how code should be written**, not architectural decisions or agent behavior.

This repo is two codebases with different conventions:

| Location | Language | Section |
|---|---|---|
| `crates/` | Rust — the CLI and its libraries | [Rust](#rust) |
| `apps/web/` | Astro, TypeScript, SCSS — the docs site | [Web](#web) |

The [General Principles](#general-principles), [Comments](#comments), [Error Handling](#error-handling), [Testing](#testing), [Dependency Style](#dependency-style) and [Decision Order](#decision-order) sections apply to both.

---

# General Principles

Prioritize:

- Readability over cleverness
- Explicitness over implicit behavior
- Simplicity over unnecessary abstraction
- Consistency with the existing codebase

If something feels overengineered, it probably is.

---

# Formatting

## Both codebases

- Indentation: **Tabs**
- Line endings: **CRLF**, except `.sh` and `.txt`, which are LF. `.gitattributes` enforces this — a CRLF shebang makes a script unrunnable, and `rosette.txt` is embedded with `include_str!` and printed to a terminal
- Final newline: **required**
- No trailing whitespace, except in Markdown

## Web only

- Quotes: **Single quotes (`'`)**
- Semicolons: **Always required**

---

# Rust

Everything under `crates/`.

## Formatting

`rustfmt` is the only authority. `rustfmt.toml` sets `hard_tabs = true` so Rust matches the repo-wide indentation rule; every other setting is a rustfmt default, so there is nothing to decide by hand.

Two commands gate every change, and CI runs both:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Clippy warnings are errors. Do not `#[allow(...)]` your way past one without a comment saying why the lint is wrong here.

## Naming

Standard Rust conventions — do not carry the Web section's camelCase over.

| Item | Example |
|---|---|
| Crates | `lattice-cache` |
| Modules and files | `toolchain.rs`, `mod commands` |
| Functions and variables | `restore_outputs`, `cache_key` |
| Types and traits | `PipelineTask`, `WorkspaceKind` |
| Constants and statics | `LOCKFILES`, `ROSETTE_ART` |

Library crates are named `lattice-<role>` for the role they own. `dagger` is the one deliberate exception: it is the task graph and scheduler, named for the DAG it builds, and it is documented as such in the README's Architecture section.

## Crate layout

One library crate per responsibility, each entered through `src/lib.rs`. A crate splits into further modules only once `lib.rs` covers two genuinely separate concerns — `lattice-workspace` has `toolchain.rs` for that reason. The binary crate `lattice` is the only one with a `src/commands/` tree, one module per subcommand.

Nothing in `crates/` may depend on `apps/web`, and no crate may reach back into the binary.

## Errors

`anyhow` throughout; there is no custom error enum and no `thiserror`. Return `anyhow::Result<T>`.

Add `.context(...)` or `.with_context(...)` wherever a failure would otherwise reach the user without naming what it was doing — filesystem and process boundaries especially, which is where the existing calls are. Do not add context that only restates the function name.

## Documentation comments

Every crate opens `lib.rs` with a `//!` header saying what the crate owns and, where it matters, what it deliberately does not. Use `///` on an item only when it adds something the signature does not already say. A doc comment that restates the item's name is worse than no comment — see [Comments](#comments) and the "Overcomment" rule in `AGENTS.md`.

---

# Web

Everything under `apps/web/`. The docs site is Astro with React islands, Bootstrap, Tailwind v4 and SCSS.

## Naming

Use **camelCase** for authored code.

| Item | Example |
|-------|---------|
| Variables | `userProfile` |
| Functions | `calculateScore` |
| Constants | `defaultTimeout` |
| Files | `userCard.tsx` |

Astro layouts under `src/layouts/` are **PascalCase** (`Layout.astro`, `DocsLayout.astro`), matching Astro's own convention for the files you pass to a page's `layout`.

## Exceptions

Keep required filenames exactly as required by frameworks, tooling, or standards.

Examples include:

- `package.json`
- `package-lock.json`
- `tsconfig.json`
- `astro.config.mjs`
- `content.config.ts`
- `Cargo.toml`
- `AGENTS.md`
- `.github/copilot-instructions.md`

Never rename required files solely to satisfy naming conventions.

---

# Functions

Applies to the Web codebase. Rust uses `fn`.

## Required

Always use arrow functions.

```ts
const doSomething = () => {
};

export default () => {
};
```

## Forbidden

```ts
function doSomething() {
}

export default function Component() {
}
```

Exceptions are only acceptable when required by the language, runtime, or framework.

---

# Exports

Applies to the Web codebase.

Use:

- **Default exports** for the primary item in a file.
- **Named exports** for supporting utilities.

Rust has no equivalent choice: export from `lib.rs` with `pub` and keep everything else private.

---

# TypeScript

Use TypeScript for:

- Business logic
- Helper functions
- Complex React components

JavaScript is acceptable for:

- Small reusable modules
- Components with minimal complexity
- Situations where type safety provides little value

TypeScript should be transpiled rather than executed directly at runtime.

Prefer **interfaces** over object `type` aliases for object shapes.

Use `any` pragmatically when it improves clarity without sacrificing correctness.

---

# Imports

Applies to the Web codebase. Rust imports are `use` statements at the top of the file, grouped and ordered by rustfmt.

Preferred:

- Relative imports
- Import aliases when appropriate
- Barrel exports when they improve organization

Use dynamic imports for:

```ts
const module = await import('module');
```

Dynamic imports are encouraged for:

- Large libraries
- Optional functionality
- Non-critical dependencies

Do **not** lazy-load:

- Application entry points
- Critical runtime logic

---

# React

Applies to the React islands under `apps/web/src/components/`.

- Component filename should match the component name.
- Keep components focused and modular.
- Move business logic outside components whenever practical.
- Reactive or component-specific logic may remain inside the component.

Split components when they become difficult to understand or maintain.

---

# Styling

Applies to the Web codebase. For terminal output, see `lattice-output`.

## Priority Order

Always style in this order:

### 1. Bootstrap Components

Use Bootstrap components for application structure.

Examples:

- Buttons
- Cards
- Forms
- Navbars
- Modals
- Alerts

Customize through Bootstrap variable overrides whenever possible.

---

### 2. Tailwind Utilities

Use Tailwind for:

- Spacing
- Typography
- Layout
- Responsive behavior
- Color
- Sizing
- Dark mode

Prefer chaining utility classes over introducing custom styles.

Arbitrary values such as:

```html
text-[0.6rem]
w-[3.5rem]
```

are preferred over creating custom classes.

---

### 3. SCSS

Use SCSS only when Bootstrap and Tailwind cannot reasonably express the desired styling.

Typical acceptable cases include:

- Keyframe animations
- `@font-face`
- Complex pseudo-elements
- Attribute selectors
- Advanced media queries
- Bootstrap property overrides affected by the Tailwind v4 cascade

Only `.scss` files are permitted.

Do not create application `.css` files.

---

## Dark Mode

Always include appropriate `dark:` Tailwind variants when styling UI.

---

## Forbidden

Do not introduce additional UI libraries such as:

- Material UI
- Chakra UI
- Headless UI
- shadcn/ui

---

# Comments

Comments should explain:

- Complex algorithms
- Non-obvious implementation decisions
- Important architectural constraints

Avoid:

- Explaining obvious code
- Excessive inline comments
- Unnecessary JSDoc

---

# Error Handling

Favor straightforward error handling.

Avoid excessive defensive programming or complex recovery layers unless clearly justified.

When fixing bugs:

1. Start from the last known working behavior.
2. Prefer the simplest effective solution.
3. Avoid introducing unnecessary abstraction.

---

# Testing

Tests should be:

- Focused
- Readable
- Easy to maintain

Avoid overly complicated test setups.

In Rust, unit tests live in an inline `mod tests` in the file they cover; end-to-end tests are `crates/lattice/tests/e2e_*.rs` and share their fixtures through `tests/common/`. `AGENTS.md` also requires `scripts/stress-test.sh` to stay current with every change — that suite, not the unit tests, is what proves Lattice still works end to end.

---

# Dependency Style

Dependencies should be:

- Installed locally
- Easy to inspect
- Easy to remove

Avoid:

- CDN imports
- URL imports
- Remote-only runtime dependencies

Prefer introducing mature, well-supported dependencies over niche packages.

In Rust, weigh every addition against compile time, and keep a dependency's version string identical across the crates that share it — `anyhow = "1"` reads the same in all six.

---

# Preferred Coding Style

Prefer:

- Explicit control flow
- Descriptive names
- Small, cohesive modules
- Readable implementations
- Existing project patterns

Avoid:

- Hidden behavior
- Magic abstractions
- Premature optimization
- Deep nesting
- Over-commenting
- Clever code that sacrifices readability

---

# Decision Order

When multiple implementations are reasonable, prefer:

1. Explicit code
2. Simpler implementation
3. Smaller modules
4. Fewer dependencies
5. Readability
6. Consistency with the existing codebase