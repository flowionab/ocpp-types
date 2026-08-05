## What & why

<!-- What does this change, and what problem does it solve? Link any related issue. -->

## Where the change was made

<!--
Types in ocpp-types are generated, not hand-written. Confirm this touches the generator
(ocpp-codegen) or the schemas (schemas/), not crates/ocpp-types/src directly — unless this PR
*is* a change to the generator/tooling itself.
-->

- [ ] `crates/ocpp-codegen` and/or `schemas/`
- [ ] `crates/ocpp-types` (non-generated code, e.g. hand-written examples/docs)
- [ ] Other (CI, docs, etc.)

## Checklist

- [ ] Added a failing test first, then made it pass (see [CONTRIBUTING.md](../CONTRIBUTING.md#test-driven-development))
- [ ] `cargo test --workspace` passes locally
- [ ] Ran `./scripts/generate.sh` and committed any resulting diff (if schemas/generator changed)
- [ ] Added a `CHANGELOG.md` entry under `[Unreleased]`
