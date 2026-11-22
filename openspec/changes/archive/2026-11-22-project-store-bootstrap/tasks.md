# Tasks

## 1. Contract and tests

- [x] 1.1 Finalize stable CLI/API success and error contracts, including concurrent initialization behavior.
- [x] 1.2 Add CLI parsing and integration tests for generated and supplied project IDs.
- [x] 1.3 Add storage tests for default configuration creation, existing-config preservation, invalid-config refusal, and no-clobber database creation.
- [x] 1.4 Add a concurrent initialization test and verify no-clobber activation on the supported CI platforms.
- [x] 1.5 Add API v1 schema, dispatcher, transport, request-ID, and exit-code tests for store.init.

## 2. Implementation

- [x] 2.1 Add a shared initialization service used by CLI and JSON API.
- [x] 2.2 Add atomic/no-clobber default configuration installation and validate configuration before database activation.
- [x] 2.3 Add the todoing init [--project-id <id>] CLI command.
- [x] 2.4 Add the store.init JSON API v1 operation and update schemas/documentation.
- [x] 2.5 Synchronize current-behavior OpenSpec specs only after implementation and verification.

## 3. Verification

- [x] 3.1 Run the full Rust test suite on Windows and Linux.
- [x] 3.2 Verify release build, isolated install, and binary smoke tests on Windows and Linux.
- [x] 3.3 Validate OpenSpec and JSON request/response schemas.
- [x] 3.4 Review failure paths to confirm existing config/database files are never overwritten.
