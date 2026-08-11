# Chronobreak verification

This file is the authoritative entry point for project-wide verification.

## Current state

The exact repository commands must be discovered from the existing Chronobreak project during the one-time repository audit.

Do not guess build, test, lint, type-check, Rust, Tauri, media, or benchmark commands.

The audit should replace this placeholder with the actual commands and any required environment/setup notes.

## Required categories

Once discovered, document at least the applicable commands for:
- frontend type-check/build;
- Rust formatting/lint/build/test;
- Tauri application build/check;
- recorder/capture component tests;
- media/FFmpeg validation if present;
- integration tests;
- feature-list schema validation;
- performance/benchmark entry points;
- any Windows-specific validation needed by the project.

## Verification principles

- Record exact commands rather than prose such as "run tests".
- Distinguish fast routine verification from expensive/full verification.
- Document prerequisites and safe fixture/test-data locations.
- Performance-sensitive capture changes require the project-defined benchmark protocol.
- Destructive storage/recovery tests must use designated test data, never a real user recording library.
