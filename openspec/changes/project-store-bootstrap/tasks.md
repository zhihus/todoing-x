# Tasks

## 1. Contract and tests

- [ ] 1.1 Finalize stable CLI/API success and error contracts, including concurrent initialization behavior.
- [ ] 1.2 Add CLI parsing and integration tests for `todoing init --project-id`.
- [ ] 1.3 Add storage tests for default configuration creation, existing-config preservation, invalid-config refusal, and no-clobber database creation.
- [ ] 1.4 Add concurrent initialization and failure-injection tests on supported platforms.
- [ ] 1.5 Add API v1 schema, dispatcher, transport, request-ID, and exit-code tests for `store.init`.

## 2. Implementation

- [ ] 2.1 Add a shared initialization service used by CLI and JSON API.
- [ ] 2.2 Add atomic/no-clobber default configuration installation and validate configuration before database activation.
- [ ] 2.3 Add the `todoing init --project-id` CLI command.
- [ ] 2.4 Add the `store.init` JSON API v1 operation and update schemas/documentation.
- [ ] 2.5 Synchronize current-behavior OpenSpec specs only after implementation and verification.

## 3. Verification

- [ ] 3.1 Run formatting and full Rust test suite.
- [ ] 3.2 Verify release build, isolated install, and binary smoke tests on Windows and Linux.
- [ ] 3.3 Validate OpenSpec and JSON request/response schemas.
- [ ] 3.4 Review failure paths to confirm existing config/database files are never overwritten.
