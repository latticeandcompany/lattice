# Lattice AI Agent Guidelines

## Preface

Greetings, AI Agents. This project is called Lattice. It's owned by a group created to manage Lattice Build, the (future) Lattice Cloud, and ArenaSwap (another repo) called Lattice & Co. Please read the guidelines below carefully and follow them.

## What this is...

Lattice's purpose is twofold:

1) It's a Turborepo clone expanded to all languages and toolchains
   1) Don't mention or call it Turborepo though. We love them dearly and are heavily inspired by, and integrate well with their project, but we are a different product. Please respect that
2) It's a **toolchain manager**. It can pin and version all sorts of developer tools (compilers, linters, package mangers, etc)

Each part can be used together or seperately. You can use one and not the other. Above all, the expierence stays seamless.

## What YOU SHOULD DO

- Skills have been provided for you. Please use them. Look at @.agents/skills
- Whenever adding, removing, changing anything, update the stress test file. This is our ultimate E2E test that validates everything about Lattice, and it must stay up to date
- Write tests for all changes
- Look at @.agents/GLOBAL_AGENTS.md @.agents/CODESTYLE.md for how you should generally act
- Remain agnostic of developer's choice. We NEVER perscribe a tool, solution, or way of working for our end users
- Look at @marketing/BRAND.md for design choices, voice, and other things about our brand
- Use subagents and custom agents liberally
- Update the CHANGELOG.md after each change you make
- Ask questions frequently
- Dogfood our own product! It helps us catch errors and gaps faster!