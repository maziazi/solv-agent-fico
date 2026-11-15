# Web-final UI alignment

## Intent

Make the Solv frontend in this worktree match the final UI implementation in
`/Users/maziazi/Coding/hackathon/OpenServ/wt/web-final` across all routes, while
preserving the current Rust API, receipt behavior, and server-side contracts.

## Goals

- Mirror the final React UI source, generated frontend assets, and legacy static
  stylesheet from `web-final`.
- Keep the existing backend and API integration unchanged.
- Preserve the final UI's routes, navigation, responsive behavior, accessibility
  labels, and evidence/reason rendering.
- Verify the frontend build and the Rust test suite after the alignment.

## Non-goals

- No backend, API, state, receipt, or policy changes.
- No new visual design or feature work beyond the selected final UI.
- No dependency changes unless the final UI requires one already absent here.

## Anti-goals

- Do not copy secrets, local state, build caches, or unrelated worktree files.
- Do not overwrite the unrelated untracked `.claude/` directory.
- Do not silently retain newer UI differences that make the result diverge from
  the selected `web-final` source.

## Approaches

1. **Exact mirror (selected):** copy the frontend source, generated `dist`
   assets, and `static/app.css` from the final worktree. This gives visual and
   route parity with the requested reference and keeps the change scoped.
2. **Selective merge:** retain newer UI changes in this worktree while copying
   only missing pages. This reduces changed lines but produces a hybrid design
   and is contrary to the request for the final UI.

## Constraints and implementation

The reference commit is `9b63afb` in the `web-final` worktree. The implementation
will compare the target asset set, copy only frontend/UI files, and leave
`src/`, templates, docs other than this design record, environment files, and
state untouched. If generated assets are tracked, they will be regenerated or
copied consistently with the source so the embedded/static serving path remains
valid.

Validation consists of the frontend's existing build and lint commands, followed
by the smallest relevant Rust checks; the complete `cargo test` suite is run
because the UI is served by the Rust binary and must preserve existing behavior.
