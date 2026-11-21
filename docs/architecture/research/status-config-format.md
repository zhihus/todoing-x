# Project configuration format evaluation

**Date:** 2026-11-21  
**Status:** Recommendation; format and schema not yet approved  
**Scope:** User-editable project configuration, especially workflow statuses and transitions

## 1. Requirements

The project configuration must be:

- editable by a human without a dedicated UI;
- simple enough for status definitions and transition rules;
- parseable into validated Rust types;
- deterministic and suitable for version control;
- independently versionable so future schema changes can be migrated;
- separate from the versioned JSON contract exposed to machine clients.

The format must not determine the primary task storage engine.

## 2. Candidate comparison

| Format | Advantages | Costs | Assessment |
|---|---|---|---|
| TOML | Comments; readable key/value and table structure; Serde integration; common in Rust tooling. | Less suitable than JSON for arbitrary deeply nested machine payloads; configuration remains a separate parser/schema. | Best fit for user-edited project configuration. |
| JSON | Strict syntax; broad tooling; direct alignment with machine interfaces; mature Serde support. | No comments; less convenient for user-maintained configuration. | Good for API payloads and internal interchange, weaker for a hand-edited workflow file. |
| YAML | Readable nested structures; comments; compatible with the existing task frontmatter. | YAML implicit typing and parser complexity; current serde_yaml dependency is explicitly unmaintained. A maintained official YAML fork exists, but that does not make YAML necessary for the new configuration. | Keep for legacy task-file migration; do not choose for new status configuration without a concrete need. |

## 3. Rust ecosystem evidence

The current TOML crate documentation describes TOML as a readable configuration format commonly used in the Rust ecosystem and provides Serde serialization/deserialization.

The current repository depends on `serde_yaml 0.9.34+deprecated`. Its own documentation states that the project is no longer maintained. The official YAML organization maintains a compatible fork published as `yaml_serde`. This matters for the legacy migration path: the current parser dependency should not be carried forward without a maintenance decision.

JSON remains the appropriate format for the public machine interface. Choosing TOML for user configuration does not imply changing JSON task responses.

## 4. Recommendation

Use TOML for the new project configuration, stored inside the project-local Todoing directory (proposed path: `.todoing/config.toml`).

Reasons:

1. Status and transition definitions are configuration, not an API payload.
2. Comments make user-managed workflows easier to explain and maintain.
3. TOML has direct Serde support and is conventional in Rust tooling.
4. It avoids introducing new YAML parser behavior into the new configuration model.
5. It keeps the machine-facing JSON contract independent of the human-edited configuration syntax.

Use JSON for the stable CLI machine interface and JSON task/history snapshots where appropriate. Use a maintained YAML parser for reading legacy Markdown frontmatter during migration.

This recommendation does not approve a final TOML schema. Field names, schema versioning, default configuration, validation rules, and migration behavior remain to be designed.

## 5. Illustrative shape only

The following shows the general TOML structure, not an approved schema:

```toml
schema_version = 1

[[statuses]]
id = "todo"
label = "To do"
position = 10
enabled = true

[[transitions]]
from = "todo"
to = "in_progress"
```

The final schema must also represent disabled/retired statuses and permitted transitions. Completion and cancellation outcomes must remain distinguishable; their relationship to workflow status is still open.

## 6. Required verification

- Verify TOML parse/serialize round trips for the final schema.
- Reject duplicate status IDs and invalid transition endpoints.
- Validate transition references to disabled/retired statuses.
- Preserve retired status values in historical task records.
- Define behavior for missing, malformed, and unsupported configuration versions.
- Verify deterministic serialization and useful line/field diagnostics.
- Test initialization without overwriting a user-owned configuration file.
- Test configuration changes against existing task records without silently rewriting history.

No configuration parser, schema, or migration code has been implemented as part of this research.

## 7. Sources

- [toml crate documentation](https://docs.rs/toml/latest/toml/)
- [serde_yaml documentation and maintenance status](https://docs.rs/serde_yaml/latest/serde_yaml/)
- [Official YAML Serde fork](https://github.com/yaml/yaml-serde)
