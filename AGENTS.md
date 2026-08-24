# Lattice AI Agent Guidelines

## Preface

Greetings, AI Agents. This project is called Lattice. It's owned by a group created to manage Lattice Build, the (future) Lattice Cloud, and ArenaSwap (another repo) called Lattice & Co. Please read the guidelines below carefully and follow them.

## What this is...

Lattice's purpose is twofold:

1) It's a **task runner** for monorepos in any language — dependency-ordered, parallel, with a content-addressed cache
   1) The model is inspired by Turborepo, expanded to every language and toolchain. Never say that in user-facing copy, and never name Turborepo in prose. We're heavily inspired by them and interoperate well with their project, but we're a different product. Please respect that. A real `turbo` command inside a config example is fine — a fake one would be worse
2) It's a **toolchain manager**. It can pin and version all sorts of developer tools (compilers, linters, package managers, etc)

Each part can be used together or separately. You can use one and not the other.

## What YOU SHOULD DO

- Skills have been provided for you. Please use them. Look at @.agents/skills
- Whenever adding, removing, changing anything, update the stress test file. This is our ultimate E2E test that validates everything about Lattice, and it must stay up to date
- We ship an agent skill at @skills/lattice — this is what other people's agents learn Lattice from. Change a command, a flag, a `lattice.json` field, or an error message, and it is part of the change, same as the docs. It is symlinked into `.agents/skills/` so we use the copy we publish
- Write tests for all changes
- A test that runs a task must get its command from `crates/lattice-testkit`, never write shell inline. A task body goes to the platform shell, and a POSIX one-liner silently stops testing anything on Windows
- Look at @.agents/CODESTYLE.md for how you should generally act
- Remain agnostic of the developer's choice. We never prescribe a tool, solution, or way of working for our end users. This is an engineering rule, not a talking point — build it, don't write copy about it
- Use subagents and custom agents liberally
- Ask questions frequently
- Dogfood our own product! It helps us catch errors and gaps faster!

## What you SHOULDNT do
- Overcomment. Use comments sparingly to explain truly unique cases of ambiguous content. Do not write a doc comment that restates the name of the thing it documents. Do not decorate the file with `// ---- Section ----` banners
- Talk about our core principles like they are features. Things like "never prescribe", "declare or detect", and "local first" are important aspects of the project, but users barely care about that. Only talk about the benefits to the user
- Cite internal design docs in code or copy. There is no PRD anymore — `(PRD §6.1)` and `(decision #11)` are dangling references to a document that no longer exists
