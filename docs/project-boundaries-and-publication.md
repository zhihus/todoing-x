# Project Boundaries and Publication Rules

## Project independence

Todoing is a standalone product. Its architecture, implementation, roadmap,
and quality criteria are developed autonomously. Other projects may consume
Todoing through its documented public interfaces when useful, but reuse is
optional. Todoing must not be modified merely to satisfy another project's
internal workflow. Consumers own their integration and policy layers.

## Local development

Local development may use multiple branches and may include experiments,
temporary files, reports, helper scripts, local configuration, test data, and
other supporting material. These files are not automatically product files.

Do not create private GitHub repositories or private mirrors for local work.
Keep local-only materials on the developer's computer.

## Single publication branch

The sole public integration branch for this repository is `main`. Work may
proceed in multiple local branches. Select and integrate approved changes into
`main`, then push only `main`. Do not publish local working branches or
create parallel public development lines without explicit user authorization.

## Distilled public contents

GitHub must contain only material directly related to the Todoing product and
needed to use, build, test, maintain, or understand it. Keep product source,
relevant tests, API contracts, product documentation, licenses, and required
build/release metadata.

Exclude personal or customer data, identifying examples, local paths,
developer-specific configuration, secrets, tokens, passwords, private keys,
temporary files, scratch directories, intermediate outputs, personal reports,
local databases, backups, and unrelated material from other projects.

Use synthetic examples. Anonymization is insufficient if surrounding context
still identifies a person or customer. Never weaken tests or security checks
to make a publication pass.

## Publication gate

Before pushing to `main`:

1. Review the complete diff and all added, changed, deleted, and untracked files.
2. Confirm every file belongs to Todoing's public product scope.
3. Check for personal data, customer material, local paths, secrets, and
   unrelated files.
4. Run applicable tests and project checks.
5. Push only the reviewed, distilled result to `main`.

Do not mirror the local working directory to GitHub. Publication is a deliberate
selection of reviewed product changes. If a file's public status is uncertain,
keep it local and ask the user.

These rules apply to all agents and contributors.
