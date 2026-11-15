# Deploying Solv to Railway

Solv ships as one Docker image (`Dockerfile`). The `cd` workflow
(`.github/workflows/cd.yml`, docs/WORKFLOW.md §4.3) deploys it to Railway when
the repository variable `DEPLOY_TARGET` is `railway`. Render is the other
target (`render`).

Facts below marked **(docs)** were read from Railway's documentation on
2026-11-15. Nothing here was run against a real Railway account: see
"Not verified" at the end.

## Lesson from the first Railway attempt

The Railway build log showed the real failure: the Docker build context was
missing `benches/`. `Cargo.toml` declares a `[[bench]]`, and cargo refuses to
parse the manifest when that file is absent, even for `cargo build --release`.
That was a bug in our `Dockerfile` (fixed in PR #27), not a problem with
Railway or with GitHub's Render workflow. Read the build log before blaming the
platform. CI now builds the image on pull requests so this cannot hide again.

## What the plan gives you (docs)

- Free plan: 1 replica, 0.5 GB RAM, 1 vCPU, 0.5 GB volume, 4 GB image size,
  and $1 of credit per month that does not roll over. A new account first gets
  a trial: 30 days or $5, whichever ends first, then reverts to Free.
- Usage prices: RAM $10/GB/month, CPU $20/vCPU/month, egress $0.05/GB, volume
  $0.15/GB/month. Builds are free.
- Free-plan builds and deploys can be **paused and queued** when Pro/Enterprise
  need capacity ("Limited Access"). A deploy on the day of a demo may wait.
- Trial volumes are deleted 30 days after the credits expire.
- Whether `$1/month` keeps Solv running 24/7 is **not measured**. Watch the
  usage page during the first week.

## One-time setup (the owner does this)

1. Create a Railway project and a service from this GitHub repository. Railway
   builds the `Dockerfile` (`railway.toml` pins `builder = "DOCKERFILE"`, the
   health check `/healthz`, and restart-on-failure up to 5 times).
2. In the service Variables, set (names only; never commit values):
   `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`, `SERV_API_KEY`,
   `SOLV_TIER1_TIMEOUT_MS`, `SOLV_TIER2_TIMEOUT_MS`, `SOLV_LATENCY_CEILING_MS`,
   `STATE_FILE=/data/solv_state.json`,
   `SOLV_SEED_FILE=/usr/local/share/solv/seed/demo_profiles.json`.
   Do **not** set `REPPO_API_KEY` or `SOLV_REPPO_ENABLED` on a public host:
   they let a visitor trigger paid audits.
3. Add a volume mounted at `/data`. **(docs)** Volumes mount as `root`, and the
   image runs as uid 1000, so also set `RAILWAY_RUN_UID=0` or the snapshot
   writes fail. This is the documented workaround; running Solv as root inside
   the container is the price, so keep the service's variables minimal.
4. Generate a public domain (Settings, Networking). Custom domains: the docs
   state limits for Trial (1) and Hobby (2); the Free plan's limit was not
   found. Check before you rely on one.
5. Create a **project token** (project settings, Tokens). **(docs)** It is
   scoped to one environment and can deploy. Do not use an account token.
6. In GitHub, Settings, Secrets and variables, Actions:
   - Secret `RAILWAY_TOKEN` (the project token)
   - Variables `DEPLOY_TARGET=railway`, `RAILWAY_SERVICE` (the service name),
     `RAILWAY_SERVICE_HOST` (hostname only, no `https://`)
   - Environment `production` with yourself as required reviewer.
7. Turn off the service's automatic deploys from GitHub so only an approved
   tag deploys (Settings, Source). Otherwise a merge to `main` deploys.

## Releasing

Push a tag (`git tag v0.1.0 && git push origin v0.1.0`), approve the
`production` environment in the Actions run. The workflow builds and pushes
the image to `ghcr.io/maziazi/solv-agent-fico`, runs `railway up --ci`
(uploads this checkout; Railway builds the Dockerfile), polls
`railway deployment list --json` until `SUCCESS` (it fails on `FAILED`,
`CRASHED` or `REMOVED`), then probes `/healthz` on `RAILWAY_SERVICE_HOST`.
The ghcr image is a host-neutral artefact; Railway does not pull it today.

## Behind Railway's proxy

**(docs)** Railway's edge terminates TLS and sets `X-Real-IP` (client IP),
`X-Forwarded-Proto` and `X-Forwarded-Host`. It documents `X-Real-IP`, not
`X-Forwarded-For`. Any per-IP rate limit must read the header the host
actually sets, and only when the operator opts in to trusting a proxy.

## State and the seed

The filesystem outside the volume is wiped on every deploy. With the volume at
`/data`, `solv_state.json` survives restarts. The demo seed
(`seed/demo_profiles.json`, PR #25) fills an empty store on first start. It
stops rating around 2027-02-13 (score decay, SPEC §5.6).

## Not verified

No Railway project exists yet, so none of this has run:

- `railway up --ci`, the `railway deployment list --json` field shape
  (`.[0].status` is assumed from the docs' "watch `.status`"), and the
  unpinned `npm install --global @railway/cli` step.
- That a project token alone is enough for `railway up --service`.
- That `RAILWAY_RUN_UID=0` fixes the volume permission error for this image.
- That the Free plan's credit keeps Solv up for a month, and the Free
  custom-domain limit.
- The Docker build itself on Railway with the `benches/` fix from PR #27.
