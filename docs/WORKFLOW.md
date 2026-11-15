# Solv: development workflow

How work moves from an idea to `main`. Feature ownership and milestones live in
`docs/PLAN.md`; product detail lives in `docs/SPEC.md`.

This document answers four questions:

1. What order do we build in, and why that order? (§2)
2. Which PR do I open now, and which must merge before it? (§3)
3. What does CI check, and what does CD deploy? (§4)
4. What must a commit message look like? (§5)

---

## 1. The loop, per change

```
1. git switch main && git pull
2. git switch -c <type>/<short-slug>          §5.4 branch naming
3. Write the test first, watch it fail
4. Write the code, watch the test pass
5. cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   bash tools/check_commit_msg.sh "<the commit subject>"
6. git commit                                 §5 Conventional Commits
7. git push -u origin <branch>
8. gh pr create --fill --base main
9. Wait for CI green. Red CI is never merged.
10. Squash merge. The PR title becomes the commit on main, so the PR
    title must itself be a valid Conventional Commit.
11. git switch main && git pull && git branch -d <branch>
```

Step 9 is not negotiable. A red check merged "because it is unrelated" is how a
guarantee in `docs/SPEC.md` §3.3 silently stops holding.

---

## 2. Why this order

Build order follows three constraints, in priority order.

**Constraint 1 — the policy gate must be testable before any provider exists.**
`policy_gate` is a pure function (SPEC §5.4). Writing it against mocks, before
any HTTP client exists, means every row of its decision table and the G1
fail-closed guarantee get tests that run in milliseconds with no network and no
spend. Writing the client first would make the gate's tests depend on a live
provider, and tests that cost money get skipped.

**Constraint 2 — Reppo's clock is on the critical path and nobody can speed it
up.** Reppo settles in ten minutes at the fastest, accepts one pending
evaluation at a time, and the free quota is ten evaluations total (SPEC §5.3).
The Reppo worker therefore starts as early as its dependencies allow and runs in
parallel with the hot-path work, so that waiting on a settlement never blocks
anything else.

**Constraint 3 — the UI needs a receipt that already exists.** The three pages
in `docs/DESIGN.md` render real decisions. Building them before the receipt store
produces placeholder screens that then need rewriting against the real shape.

The resulting phase order:

| Phase | What | Why here |
| --- | --- | --- |
| A | Scaffold + CI | Nothing is reviewable until CI can tell green from red |
| B | Pure logic: state, score mapping, policy gate | Testable with no network, no spend; locks the guarantees first |
| C | Tier 1 client, then the `pre-spend-check` endpoint | First real decision; first real latency number |
| D | Tier 2 escalation, receipts | Completes the hot path and makes it auditable |
| E | Reppo worker, learned rules | Runs in parallel with D; gated by a third party's clock |
| F | The three web pages | Needs real receipts from D |
| G | Attack Lab, benchmarks, claims pass | Converts UNPROVEN claims into PROVEN or deletes them |

---

## 3. The PR map

Sixteen PRs. The **Waits for** column is binding: do not open a PR until every
PR it lists has merged to `main`.

| PR | Conventional Commit title | Waits for | Can run in parallel with | Done when |
| --- | --- | --- | --- | --- |
| **PR-01** | `chore: scaffold rust service and ci` | — | — | `cargo build` succeeds, `GET /healthz` returns 200, CI green on this very PR |
| **PR-02** | `feat(state): in-memory store with atomic json snapshot` | PR-01 | PR-03, PR-05, PR-16 | State reloads after restart; a kill mid-write leaves no corrupt file |
| **PR-03** | `feat(score): reppo verdict to solv fico mapping` | PR-01 | PR-02, PR-05, PR-16 | Every row of the SPEC §4.2 table and every decay branch has a test |
| **PR-04** | `feat(policy): deterministic policy gate and reason taxonomy` | PR-03 | PR-05 | All 12 rows of SPEC §5.4 tested; G1 and G5 tests pass |
| **PR-05** | `feat(tier1): cloudflare clef-flash client behind a trait` | PR-01 | PR-02, PR-03, PR-04 | Mock passes offline; one `#[ignore]` test hits the real endpoint |
| **PR-06** | `feat(api): pre-spend-check endpoint` | PR-02, PR-04, PR-05 | PR-09 | Returns a real decision with non-empty `reasons[]`; per-tier latency recorded |
| **PR-07** | `feat(tier2): serv reasoning escalation with prompt guard` | PR-06 | PR-08, PR-09 | Guard refusal maps to `SOLV-INJECTION-001`; G2 budget test passes |
| **PR-08** | `feat(receipt): evidence hash and receipt store` | PR-06 | PR-07, PR-09 | `evidence_sha256` recomputable by a third party; G4 test passes |
| **PR-09** | `feat(bureau): reppo worker with single-slot queue` | PR-02, PR-03 | PR-06, PR-07, PR-08 | One real evaluation reaches `SETTLED` and updates a profile |
| **PR-10** | `feat(bureau): compile learned rules into clef questions` | PR-05, PR-09 | PR-11, PR-12 | A rule born from a real critique appears as a `noul` question, capped at 8 |
| **PR-11** | `feat(web): underwriting desk page` | PR-06, PR-08 | PR-12, PR-13 | Three presets and the custom form produce live decisions |
| **PR-12** | `feat(web): public trust receipt page` | PR-08 | PR-11, PR-13 | Renders any receipt id; counterparty quotes are escaped, never HTML |
| **PR-13** | `feat(web): bureau directory page` | PR-09 | PR-11, PR-12 | Lists real counterparties, real learned rules, real metrics |
| **PR-14** | `test(attack): prompt injection corpus` | PR-07, PR-10 | PR-15 | ≥20 cases, zero `APPROVE`, every reason points at the right span |
| **PR-15** | `docs: readme with claim verification labels` | PR-14 | — | Every claim carries a SPEC §9 label; no UNPROVEN number in the headline |
| **PR-16** | `ci: deploy workflow with manual approval gate` | PR-01 | PR-02, PR-03, PR-05 | A tagged release deploys; secrets come only from Actions Secrets |

### 3.1 Dependency graph

```
PR-01 ──┬── PR-02 ──┬───────────────┐
        │           │               │
        ├── PR-03 ──┴── PR-04 ──┐   │
        │       │               │   │
        ├── PR-05 ──────────────┴───┴── PR-06 ──┬── PR-07 ──┐
        │       │                               │           │
        │       │                               └── PR-08 ──┼── PR-11
        │       │                                   │       │
        ├── PR-16                                   └───────┼── PR-12
        │                                                   │
        └── PR-02, PR-03 ── PR-09 ──┬── PR-10 ──────────────┤
                                    │                       │
                                    └── PR-13               │
                                                            │
                                      PR-07 + PR-10 ── PR-14 ── PR-15
```

### 3.2 What to do while waiting for a merge

The three real bottlenecks, and what to do instead of idling:

| Blocked on | Do this meanwhile |
| --- | --- |
| **PR-01 review** — blocks all sixteen | Nothing in code. Draft the Attack Lab corpus as plain text fixtures; they are data, not code, and PR-14 will consume them unchanged. |
| **PR-06 review** — the first three-way integration point (PR-02 + PR-04 + PR-05) | Open PR-09. It depends only on PR-02 and PR-03, so it is legitimately parallel, and it starts the Reppo clock early. |
| **A Reppo settlement** — ten minutes minimum, one at a time | Never wait idle. Reppo polling is a background worker; continue on PR-07, PR-08, or the web PRs. If you find yourself waiting on Reppo, the worker is wrong. |

### 3.3 Stacked PRs

Default: do not stack. Branch from `main`, merge to `main`.

The one sanctioned exception is PR-11, PR-12, and PR-13. All three touch the
same template and asset layout, and all three depend on PR-08. Open PR-12 first,
merge it, then branch PR-11 and PR-13 from the updated `main`. Opening all three
from `main` simultaneously guarantees three-way conflicts in the shared layout.

### 3.4 PR hygiene

- One PR, one capability. If the title needs the word "and", split it.
- PR description states: what changed, which SPEC section it implements, which
  tests prove it, and any `#[ignore]` test it adds and why.
- A PR that changes behaviour covered by a guarantee (SPEC §3.3) names the
  guarantee in its description.
- A PR that adds a reason code edits SPEC §6.2 in the same PR. The taxonomy is
  closed; code and document move together.
- Self-review the full diff in the GitHub PR view before merging — see §4.2. Run
  `git status` after any broad `git add` and confirm no `.env`, no
  `solv_state.json`, no key material.

---

## 4. CI/CD on GitHub

Two workflows. CI runs on every PR and every push to `main`. CD runs only on a
tag, behind a manual approval gate.

Both files land in **PR-01** (CI) and **PR-16** (CD), alongside `Cargo.toml`.
Adding `ci.yml` before `Cargo.toml` exists would make the very first push red for
no reason.

### 4.1 `.github/workflows/ci.yml`

```yaml
name: ci

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  commit-lint:
    name: commit messages
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Check PR title and commit subjects
        run: |
          if [ -n "${{ github.event.pull_request.title }}" ]; then
            bash tools/check_commit_msg.sh "${{ github.event.pull_request.title }}"
          fi
          base="${{ github.event.pull_request.base.sha }}"
          head="${{ github.event.pull_request.head.sha }}"
          if [ -n "$base" ] && [ -n "$head" ]; then
            git log --format=%s "$base..$head" | while read -r subject; do
              bash tools/check_commit_msg.sh "$subject"
            done
          fi

  fmt:
    name: rustfmt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    name: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests with no network credentials present
        run: cargo test --all-features --locked

  audit:
    name: dependency audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check

  secret-scan:
    name: secret scan
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: gitleaks/gitleaks-action@v2
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

  forbidden-files:
    name: forbidden files
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Reject committed state and env files
        run: |
          if git ls-files --error-unmatch .env solv_state.json 2>/dev/null; then
            echo "::error::.env or solv_state.json is tracked; see AGENTS.md"
            exit 1
          fi
          exit 0

  smoke:
    name: healthz smoke
    needs: [test]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release
      - name: Boot and probe /healthz
        run: |
          ./target/release/solv &
          pid=$!
          for i in $(seq 1 30); do
            if curl -fsS http://127.0.0.1:8080/healthz; then ok=1; break; fi
            sleep 1
          done
          kill $pid
          test "$ok" = "1"
        env:
          PORT: 8080
          STATE_FILE: ci_state.json
```

Notes on deliberate choices:

- **The `test` job runs with no provider secrets present.** That is the point:
  it enforces the `AGENTS.md` rule that `cargo test` passes offline. If someone
  adds a test that needs a live key, this job goes red, which is the correct
  outcome.
- **`--locked`** means `Cargo.lock` must be committed and current. A dependency
  that resolves differently in CI than locally is a debugging trap.
- **`secret-scan` and `forbidden-files` are separate jobs** so their failure
  message is unambiguous. A key leak discovered inside a generic "lint" job gets
  misread as a style failure.
- **`smoke`** catches the class of bug unit tests never see: the binary compiles
  and the tests pass, but the service cannot actually boot.

### 4.2 Branch protection for a solo repository

Configure branch protection on `main` to require all of:
`commit messages`, `rustfmt`, `clippy`, `test`, `dependency audit`,
`secret scan`, `forbidden files`, `healthz smoke`.

Also enable: require a PR before merging, require branches up to date before
merging, and **disallow force pushes and deletions on `main`**.

**Do not enable "require approvals".** This is a solo project: GitHub will not
let an author approve their own PR, so a required approval count of 1 makes
`main` unmergeable and the only way out is `--admin`, which bypasses every check
above. One bypassed merge teaches the habit, and the habit is what eventually
lets a red `secret scan` through.

Enable instead:

- **Require conversation resolution before merging.** Solo, this turns your own
  review comments into a checklist you cannot forget.
- **Do not allow bypassing the above settings** — including for administrators.
  Tick this. It is the setting that makes the other settings true.

The resulting shape: CI is the sole gate, it cannot be skipped, and self-review
happens in the PR diff view before you press merge. Read the diff on GitHub
rather than in your editor; a different surface catches different mistakes.

### 4.3 `.github/workflows/cd.yml`

Hosts: Render (free web service) or Railway (Free plan), chosen by the
repository variable `DEPLOY_TARGET` (`render` or `railway`). Both build the
committed `Dockerfile` themselves; the workflow also builds the image once and
pushes it to `ghcr.io/maziazi/solv-agent-fico` as a host-neutral artefact.
Railway setup and its unverified points: `docs/DEPLOY-RAILWAY.md`. fly.io was
dropped on 2026-11-15 (the account was flagged high-risk at creation).

```yaml
name: cd

on:
  push:
    tags: ['v*']
  workflow_dispatch:

concurrency:
  group: cd-production
  cancel-in-progress: false

env:
  IMAGE: ghcr.io/maziazi/solv-agent-fico

jobs:
  image:
    name: build and push the image
    runs-on: ubuntu-latest
    environment: production        # requires a reviewer to approve
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - name: Log in to ghcr.io
        run: echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u "${{ github.actor }}" --password-stdin
      - name: Build
        run: docker build -t "$IMAGE:${{ github.sha }}" .
      - name: Push the commit tag and, on a tag push, the release tag
        run: |
          docker push "$IMAGE:${{ github.sha }}"
          if [ "${{ github.ref_type }}" = "tag" ]; then
            docker tag "$IMAGE:${{ github.sha }}" "$IMAGE:${{ github.ref_name }}"
            docker push "$IMAGE:${{ github.ref_name }}"
          fi

  deploy:
    name: deploy to ${{ vars.DEPLOY_TARGET }}
    needs: image
    runs-on: ubuntu-latest
    environment: production
    steps:
      - uses: actions/checkout@v4
      - name: Refuse an unknown or missing DEPLOY_TARGET
        run: |
          case "${{ vars.DEPLOY_TARGET }}" in
            render|railway) ;;
            *) echo "::error::DEPLOY_TARGET must be 'render' or 'railway', got '${{ vars.DEPLOY_TARGET }}'"; exit 1 ;;
          esac

      - name: Trigger the Render deploy hook
        if: vars.DEPLOY_TARGET == 'render'
        run: |
          curl -fsS -X POST "${{ secrets.RENDER_DEPLOY_HOOK }}" \
            -H "Content-Type: application/json" \
            -d '{"clearCache": "do_not_clear"}'

      - name: Deploy to Railway
        if: vars.DEPLOY_TARGET == 'railway'
        env:
          RAILWAY_TOKEN: ${{ secrets.RAILWAY_TOKEN }}
        run: |
          npm install --global @railway/cli
          railway up --ci --service "${{ vars.RAILWAY_SERVICE }}"

      - name: Wait for Railway to report the deployment
        if: vars.DEPLOY_TARGET == 'railway'
        env:
          RAILWAY_TOKEN: ${{ secrets.RAILWAY_TOKEN }}
        run: |
          for i in $(seq 1 60); do
            status=$(railway deployment list --service "${{ vars.RAILWAY_SERVICE }}" --json --limit 1 | jq -r '.[0].status')
            echo "railway deployment status: $status"
            case "$status" in
              SUCCESS) exit 0 ;;
              FAILED|CRASHED|REMOVED) exit 1 ;;
            esac
            sleep 10
          done
          exit 1

      - name: Verify the deployment answers
        run: |
          case "${{ vars.DEPLOY_TARGET }}" in
            render) host="${{ vars.RENDER_SERVICE_HOST }}" ;;
            railway) host="${{ vars.RAILWAY_SERVICE_HOST }}" ;;
          esac
          for i in $(seq 1 30); do
            if curl -fsS "https://$host/healthz"; then exit 0; fi
            sleep 10
          done
          exit 1
```

Two safety properties worth keeping:

- **`environment: production` forces a human approval** before anything reaches
  production. Deploys trigger on a tag, not on every merge to `main`, so merging
  is never an accidental release. Turn off the host's own auto-deploy from
  GitHub, or a merge would deploy anyway.
- **Provider credentials never touch the repository.** They are set once in the
  host's environment settings. The only deploy secrets in Actions are
  `RENDER_DEPLOY_HOOK` or `RAILWAY_TOKEN` (a project token, scoped to one
  environment).

Required Actions Variables: `DEPLOY_TARGET`, plus `RENDER_SERVICE_HOST` for
Render or `RAILWAY_SERVICE` and `RAILWAY_SERVICE_HOST` for Railway. The
Cloudflare, SERV and Reppo credentials live in the host, not in Actions; a
public host must not carry `REPPO_API_KEY` or `SOLV_REPPO_ENABLED`.

### 4.4 Optional: scheduled claim audit

Once PR-14 and PR-15 merge, a weekly job that reruns the Attack Lab and the
benchmarks is worth adding. Its purpose is narrow: catch the day a `PROVEN`
claim in SPEC §9.1 stops being true because a provider changed behaviour.
Out of scope until M8.

---

## 5. Commit convention

Conventional Commits 1.0.0. CI rejects anything that does not conform, for PR
titles and for every commit subject in the PR.

### 5.1 Format

```
<type>(<scope>): <subject>

<body, optional, wrapped at 72 columns>

<footers, optional>
```

### 5.2 Allowed types

| Type | Use for | Example |
| --- | --- | --- |
| `feat` | A new capability | `feat(tier1): cloudflare clef-flash client behind a trait` |
| `fix` | A bug fix | `fix(policy): block before cap check so safety cannot be outranked` |
| `docs` | Documentation only | `docs(spec): withdraw the 40x savings claim` |
| `test` | Tests only | `test(attack): add zero-width injection variants` |
| `refactor` | No behaviour change | `refactor(state): extract snapshot writer` |
| `perf` | A measured speed or cost change | `perf(tier1): drop a round-trip by batching learned rules` |
| `ci` | Workflow or pipeline changes | `ci: require commit-message check on pull requests` |
| `build` | Dependencies or build config | `build: pin axum to 0.8` |
| `chore` | Maintenance with no src or test change | `chore: add .env.example` |
| `revert` | Reverting a previous commit | `revert: feat(bureau): auto-promote learned rules` |

`perf` carries an obligation: the subject or body must state the measured
before-and-after number. A `perf` commit without a number is a `refactor`.

### 5.3 Allowed scopes

Scope is optional, but prefer it. Use one of: `state`, `score`, `policy`,
`reason`, `tier1`, `tier2`, `bureau`, `api`, `receipt`, `web`, `attack`, `bench`,
`spec`, `plan`, `deps`.

### 5.4 Rules CI enforces

- `type` is from the §5.2 list
- `scope`, if present, is from the §5.3 list
- subject is lowercase, imperative mood ("add", not "added" or "adds")
- subject has no trailing period
- the full subject line is at most 72 characters
- a breaking change uses `!` after the scope **and** a `BREAKING CHANGE:` footer
- references use a footer, not the subject: `Refs: #12` or `Closes: #12`

Branch names mirror the type: `feat/clef-flash-client`, `fix/policy-order`,
`ci/deploy-gate`, `docs/spec-claims`.

### 5.5 `tools/check_commit_msg.sh`

```bash
#!/usr/bin/env bash
# Validates one Conventional Commit subject line.
# Usage: check_commit_msg.sh "<subject>"   or   check_commit_msg.sh <file>
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 \"<subject>\" | <file>" >&2
  exit 2
fi

if [ -f "$1" ]; then
  subject=$(head -n 1 "$1")
else
  subject="$1"
fi

# Merge and revert commits generated by tooling are exempt.
case "$subject" in
  "Merge "*|"Revert "*) exit 0 ;;
esac

types='feat|fix|docs|test|refactor|perf|ci|build|chore|revert'
scopes='state|score|policy|reason|tier1|tier2|bureau|api|receipt|web|attack|bench|spec|plan|deps'
pattern="^(${types})(\((${scopes})\))?!?: [a-z][^A-Z]*[^.]$"

fail() {
  echo "invalid commit subject: $subject" >&2
  echo "  reason: $1" >&2
  echo "  format: <type>(<scope>): <lowercase imperative subject>" >&2
  echo "  types:  ${types//|/, }" >&2
  echo "  see docs/WORKFLOW.md section 5" >&2
  exit 1
}

[ "${#subject}" -le 72 ] || fail "longer than 72 characters (${#subject})"
printf '%s' "$subject" | grep -Eq "$pattern" || fail "does not match the required pattern"

echo "ok: $subject"
```

Install it as a local hook so a bad message is caught before it is written,
rather than in CI:

```bash
cat > .git/hooks/commit-msg <<'HOOK'
#!/usr/bin/env bash
exec bash tools/check_commit_msg.sh "$1"
HOOK
chmod +x .git/hooks/commit-msg
```

`.git/hooks/` is not version-controlled, so this step is per-clone. CI is the
authority; the hook is a convenience.

### 5.6 Why squash merge

The PR title becomes the single commit on `main`. That gives `main` a history
where one commit equals one capability, which makes `git log --oneline` a usable
changelog and `git revert` a safe single-step operation. It is also why §5.4
applies to PR titles and not only to commits: with squash merge, the PR title is
the message that survives.
