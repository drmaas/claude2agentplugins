---
name: bug-fix-workflow
description: Workflow for fixing bugs
---

# Bug Fix Workflow

Use this workflow when debugging and fixing issues.

## Steps

1. **Tests** — Write a test that reproduces the bug (must fail)
2. **Fix** — Write minimum fix to pass the test
3. **Verify** — Run all tests, lint, typecheck (all green)
4. **Review** — Review fix for correctness, side effects, edge cases
5. **Loop** — Iterate steps 2-4 until review passes
6. **Docs** — Update documentation if behavior changed
