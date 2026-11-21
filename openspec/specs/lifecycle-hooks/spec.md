# Lifecycle hooks

## Purpose

Describe the optional hook mechanism currently implemented by the Todoing CLI.

## Requirements

### Requirement: Optional project hooks

The system MUST allow project-specific hook executables to be placed under `.todoing/hooks/`.

A missing hook MUST be treated as a no-op. Todoing MUST NOT install standard hooks automatically during ordinary task operations.

#### Scenario: Missing hook

- **WHEN** an operation looks for a hook that is not installed
- **THEN** the operation continues without running that hook

#### Scenario: Hook accepts an operation

- **WHEN** an installed hook receives the task JSON on standard input and exits with code zero
- **THEN** the hook does not reject the operation

#### Scenario: Hook rejects an operation

- **WHEN** an installed hook exits with a non-zero code
- **THEN** the associated operation is rejected

### Requirement: Standard completion hooks

The CLI MUST provide optional installation of the bundled `before-done-git` and `before-done-openspec` hooks.

The CLI MUST NOT overwrite an existing hook file when installing a standard hook.

#### Scenario: Inspect hook installation

- **WHEN** the user runs `todoing hooks status`
- **THEN** the CLI reports whether each standard hook is installed
- **AND** indicates that an agent must ask the user before installing missing standard hooks

#### Scenario: Install selected standard hooks

- **WHEN** the user explicitly runs the hook installation command for a selected hook set
- **THEN** only the selected missing standard hook files are installed
- **AND** existing hook files are preserved

### Requirement: Completion hook order and triggers

When a task transitions to `done`, the CLI MUST run installed completion hooks in this order: `before-done-git`, `before-done-openspec`, then the project-specific `before-done` hook.

The CLI MUST run completion hooks when a task is created with its initial status set to `done`.

The `on-create` hook MUST run for every task creation after any applicable completion hooks.

#### Scenario: Completion is rejected

- **WHEN** any installed completion hook exits with a non-zero code
- **THEN** the completion operation fails
- **AND** the task mutation is not written

#### Scenario: Create a task as done

- **WHEN** the user creates a task with initial status `done`
- **THEN** installed completion hooks run in the documented order
- **AND** the `on-create` hook runs after the completion hooks
