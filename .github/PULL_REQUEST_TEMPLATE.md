<!--
Read CONTRIBUTING.md before opening this. Pull requests target `mega`, stay
single-scope, and ship tests. Merges are not squashed, so your commit messages
survive. Write them properly.
-->

## What this changes

<!-- What it does and why. Reference the issue it closes: Closes #123 -->

## How it was verified

<!--
Name the tests you added and what you ran: `cargo test --workspace`,
`scripts/stress-test.sh`, and a real run against examples/polyglot or
examples/nested-repo. Say which of those you ran, and what the second (cached)
run did.
-->

## Requirements

- [ ] **Tests.** This change ships tests. A bug fix ships a regression test that failed before it
- [ ] **CHANGELOG.** An entry is added under `## Unreleased`
- [ ] **Stress test.** `scripts/stress-test.sh` is updated if behavior changed, and it exits `0`
- [ ] **Docs.** `apps/web/src/content/docs/` is updated if behavior or configuration changed
- [ ] **Scope.** No opportunistic refactors, no unrelated changes
- [ ] **Detection.** Nothing here hardcodes a tool, or acts on a tool the user did not declare and no evidence identifies

## AI assistance

<!--
Required. AI-assisted contributions are welcome and must be disclosed. You are
accountable for every line either way.
-->

- [ ] AI assistance was used on this change

Tools used: <!-- for example: Claude Code, Cursor, Copilot, or "none" -->

## Notes for the reviewer

<!-- Trade-offs, anything you're unsure about, follow-up work you deliberately left out. -->
