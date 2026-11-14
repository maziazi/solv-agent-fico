# AGENTS.md

Read `docs/PLAN.md`, then `docs/SPEC.md`. Both are the project's source of truth.
Development order, PR map, CI/CD, and commit convention live in
`docs/WORKFLOW.md`. The visual system lives in `docs/DESIGN.md`.

Solv is a pre-spend gate and credit bureau for AI agents. One Rust binary.

Two flagship axes that no change may sacrifice: **hot-path speed** and
**decisions whose reasons are clear**. Their measurable definitions are in
`docs/SPEC.md` §2.

## Commands

- Build: `cargo build`
- Release build: `cargo build --release`
- Run: `cargo run` (needs `.env`, see `.env.example`)
- Test: `cargo test`
- Test including network-dependent cases: `cargo test -- --ignored`
- Format: `cargo fmt --all` — check with `cargo fmt --all -- --check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Dependency audit: `cargo deny check`
- Hot-path latency benchmark: `cargo bench` (see `docs/SPEC.md` §2.1)
- Check a commit message: `bash tools/check_commit_msg.sh "<message>"`

Four commands must pass before opening a PR:
`cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test`,
`bash tools/check_commit_msg.sh "<PR title>"`.

## Working rules

- Work only on the features you own in `docs/PLAN.md`, together with their tests.
- Work on a branch and open a PR. Never push to `main` and never force-push.
- One PR, one capability. Do not bundle unrelated features.
- Open a PR only once every prerequisite PR listed in `docs/WORKFLOW.md` §3 has
  merged. Do not stack PRs on unmerged branches unless the PR map says to.
- PR titles and every commit message must follow Conventional Commits. Full
  rules in `docs/WORKFLOW.md` §5; CI rejects anything that does not conform.
- Never change or delete a test to make it pass.
- Never disable a CI job, and never merge with `--admin` or `--no-verify`.

## Hard prohibitions

- Never read, print, or commit API keys, tokens, or secrets. Keys live only in a
  local, gitignored `.env` and in GitHub Actions Secrets.
- Never commit `solv_state.json` or any state snapshot. It contains untrusted
  counterparty payloads.
- Never put paid credits in the automated test path. `cargo test` must pass with
  no network: Clef, SERV, and Reppo are reached through traits that are mocked.
  Tests that need the network are marked `#[ignore]`.
- Never spend Reppo quota without the project owner's approval. The free tier is
  1,000 credits (10 evaluations), 100 credits per settled evaluation, and cannot
  be renewed.
- Never add a network call to `POST /api/v1/pre-spend-check` beyond Tier 1 and
  Tier 2. Reppo is never called from the hot path — the reason is in
  `docs/SPEC.md` §5.3.
- Never return a decision without `reasons[]`. A decision without traceable
  reasons is a bug, not merely a UX gap. See `docs/SPEC.md` §6.
- Never state a claim in the README, the UI, or any document above the evidence
  that backs it. The claim verification legend is in `docs/SPEC.md` §9. Numbers
  that have not been measured are labelled `UNPROVEN`, never guessed.
- Never change the score mapping in `docs/SPEC.md` §4 without writing the reason
  in the PR. That mapping is a public promise printed on every receipt.
- Never submit to a hackathon, post to social media, or make the repository
  public without the project owner's approval.
