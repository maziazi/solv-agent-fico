# Web-final UI Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the active worktree's complete frontend match the final UI in the
`web-final` worktree without changing the Rust API or backend behavior.

**Architecture:** Treat `/Users/maziazi/Coding/hackathon/OpenServ/wt/web-final` at
commit `9b63afb` as the UI source of truth. Copy only tracked frontend source,
generated browser assets, and the server-served stylesheet; leave backend,
templates, environment, state, and unrelated worktree files untouched.

**Tech Stack:** React 19, TypeScript, Vite, React Router, CSS, Rust/Axum
server static assets.

**Spec:** [docs/superpowers/specs/2026-11-16-web-final-ui-design.md](../specs/2026-11-16-web-final-ui-design.md)

## Global Constraints

- Preserve the project's two flagship axes: hot-path speed and decisions whose
  reasons are clear.
- Do not change backend/API/state/policy behavior.
- Do not copy secrets, `solv_state.json`, build caches, or `.claude/`.
- Keep the final UI routes, navigation, responsive behavior, accessibility
  labels, and evidence/reason rendering intact.
- Run `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` before completion.

## Review Focus

- Route parity: each final UI route resolves and the desk is not accidentally
  restored as the landing page; verify with the frontend build and route source.
- API contract parity: final components still call the existing `/api/v1`
  endpoints; verify with TypeScript compilation.
- Receipt safety: reason/evidence content remains escaped/rendered through the
  existing components; verify with `cargo test`.
- Static serving parity: generated `frontend/dist` and `static/app.css` remain
  aligned with the copied source; verify with a clean frontend build and diff.
- Scope safety: no backend, state, secret, or unrelated untracked files are
  changed; verify with `git diff --name-only` and `git status --short`.

---

### Task 1: Mirror the final frontend UI

**Files:**
- Modify: `frontend/src/**`
- Modify: `frontend/dist/**`
- Modify: `static/app.css`
- Do not modify: `src/**`, `templates/**`, `.env`, `solv_state.json`, `.claude/**`

**Interfaces:**
- Consumes: final UI files from `/Users/maziazi/Coding/hackathon/OpenServ/wt/web-final` at `9b63afb`.
- Produces: the same React routes/components and static styling as the
  reference worktree, while retaining the current Rust API integration.

- [ ] **Step 1: Confirm the source snapshot**

Run:

```bash
git -C /Users/maziazi/Coding/hackathon/OpenServ/wt/web-final rev-parse HEAD
git -C /Users/maziazi/Coding/hackathon/OpenServ/wt/web-final status --short
```

Expected: `9b63afb` is the reference commit and no local reference changes are
used as source.

- [ ] **Step 2: Copy only UI implementation files**

Copy the reference `frontend/src`, `frontend/dist`, and `static/app.css` into
the active worktree. Do not copy manifests, environment files, Rust sources,
templates, state, or unrelated worktree metadata.

- [ ] **Step 3: Build the frontend**

Run:

```bash
cd frontend && pnpm build
```

Expected: TypeScript and Vite complete successfully.

- [ ] **Step 4: Run frontend lint**

Run:

```bash
cd frontend && pnpm lint
```

Expected: no lint errors.

- [ ] **Step 5: Verify the backend remains healthy**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Expected: all commands pass without network-dependent tests or paid provider
calls.

- [ ] **Step 6: Verify the final diff scope**

Run:

```bash
git status --short
git --no-pager diff --name-only
git --no-pager diff --check
```

Expected: only the intended frontend/static files and the design/plan docs are
listed; `.claude/`, secrets, state snapshots, and backend files are absent.

- [ ] **Step 7: Commit the aligned UI**

```bash
git add frontend/src frontend/dist static/app.css docs/superpowers/specs/2026-11-16-web-final-ui-design.md docs/superpowers/plans/2026-11-16-web-final-ui-plan.md
git commit -m "feat(web): align frontend with final ui"
```

