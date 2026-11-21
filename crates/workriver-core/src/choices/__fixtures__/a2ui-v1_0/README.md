# A2UI v1.0 JSON Schemas (vendored for a test)

Copied unchanged from https://github.com/a2ui-project/a2ui at commit
`496df0e76e2ad9aba4cc2a12abe066f43d4f171d` (2026-11-21), under the Apache License 2.0
(`LICENSE`, from the same commit):

- `agent_to_renderer.json`, `common_types.json`, `renderer_to_agent.json`: from
  `specification/v1_0/json/`;
- `catalog.json`: the basic catalog, from `catalogs/basic/v1/catalog.json`; byte for byte the
  copy served at https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json.

Only `choices/tests.rs` reads them: every choice core builds must be a valid A2UI v1.0
message (`docs/design/guards.md`). Replace them only on purpose, with a new commit named here.
