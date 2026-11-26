# Agent Instructions

Todoing is an independent, standalone product. Read `PRODUCT.md` for its
product boundary and `docs/project-boundaries-and-publication.md` for the
mandatory local-development and GitHub publication rules before changing
project governance or preparing a publication.

Keep consumer-specific orchestration and policy outside Todoing. Manifest and
other projects may use Todoing's documented interfaces, but Todoing must not
be coupled to their internal workflows.

Local development may use multiple branches and local-only supporting files.
Only reviewed, distilled product changes are integrated into and pushed from
the single public branch, `main`. Do not publish local working branches,
personal/customer data, local paths, secrets, temporary files, or unrelated
project material. Do not create private GitHub mirrors. When publication
eligibility is uncertain, keep the material local and ask the user.
