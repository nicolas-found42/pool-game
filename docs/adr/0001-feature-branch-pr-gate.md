# 0001 — Feature-branch PR gate on `main`

Date: 2026-09-10

## Status

Accepted

## Context

The repo is maintained by a solo maintainer, but we want every change to land
on `main` through review and a visible diff, and we want the rule enforced by
GitHub rather than by convention.

## Decision

- All work merges to `main` via pull request only; direct pushes to `main` are blocked.
- Branch protection: `required_pull_request_reviews` (0 approvals — solo maintainer), `enforce_admins: true` so the gate binds the owner too, force pushes and deletions disabled, `delete_branch_on_merge` on.
- A required CI status check (job id `ci`) gates every merge. Until a build stack
  is chosen, `ci.yml` is a checkout-only placeholder; it must be replaced with
  the real build+test commands when the stack lands.
- PRs as a triage/request surface: off (`docs/agents/issue-tracker.md`).
