# Arca Isolated Rollout

This directory supports the 2026-10-16 side-by-side rollout. It is not an updater
for the original Aether deployment. Production credentials must never be placed
in this checkout or source artifacts.

## Initial Review

- All product GitHub icons use the shared Arca repository constant. Original
  Aether attribution/license links remain intact.
- The extensive uncommitted/untracked feature implementation must be snapshotted
  from the working tree, not just the current Git commit.
- Formatting/Clippy, complete frontend checks, Linux protocol/policy/role tests,
  real PostgreSQL migration and quota concurrency tests, and real HTTP/MCP
  acceptance are release gates. A mock-browser preview is not backend acceptance.
- Inherited upstream self-update is not an Arca release channel. The independent
  deployment must explicitly use `AETHER_UPDATE_STRATEGY=manual` until a verified
  Arca release supply chain is configured.
- Never point the new app at the live Aether database or Redis. Preserve effective
  credential encryption keys only in protected on-host deployment configuration.
- Copy compatible identities, key hashes/ciphertext, providers/models/routing,
  preferences, announcements and history from a consistent database backup.
  Transient sessions/caches and retired commercial configuration need deliberate
  exclusion/quarantine rather than becoming active in the new service.
- User spending/usage quotas will be unlimited initially; model/IP permissions,
  revocation/disabled/expiry state remain security boundaries.
- Existing tunnel agents belong to the old gateway. Do not re-register or rotate
  them for Arca: test an independent authenticated relay/agent arrangement without
  taking existing Aether nodes offline.
- HK root storage is constrained; backups and isolated restored data belong on
  its already-mounted dedicated data disk. Build/test workloads are CPU/memory
  capped and must not prune unrelated images or kill unrelated jobs.

## Execution Order

1. Review and snapshot complete source with `snapshot_source.py`.
2. Compile Linux test artifacts/debug runtime on 12 using the existing toolchain
   container (not a new production image). Execute the actual tests on Korea,
   single-worker where appropriate, with disposable PostgreSQL/Redis and synthetic
   local upstreams. Exercise all new surfaces, not only unit tests.
3. Only after Korean acceptance, compile the production release and assemble a
   pinned Docker image on 12. Recheck the release runtime before HK migration.
4. Make a protected consistent HK backup, restore into isolated Arca storage,
   verify preservation, apply supported additive migrations and scoped compatibility
   transformations only to the new copy. Preserve backup and exact exclusions.
5. Start Arca on an independently checked free port; keep Aether untouched and
   verify both old and new services after every consequential step.

Do not claim completion without source/image hashes, test evidence, restore and
row/hash comparisons, authenticated operation and original-service health proofs.

## Small-Host Resource Gate

Never overlap frontend static checks, browser checks, Rust suites or full restore
on the 2 GiB Korean host. After verifying that previous jobs have actually ended,
run heavy commands through `resource_guard.py --memory-mib N -- command ...`
from `/opt/arca-test-20261016/source`; N must match the Docker physical-memory
limit. The guard requires that limit plus 256 MiB physical headroom, at least
4 GiB free disk and a bounded host load. Swap does not count as physical headroom.
The process-held advisory lock serializes guarded jobs; a stale filename alone
does not imply a running job. The Rust bundle runner acquires this slot itself.
An unknown job state after an SSH timeout must be inspected, not rerun or assumed
successful. Resource refusal is not a test pass. Temporary test-only swap may
buffer a single large TypeScript heap, but must not be made persistent and must
be safely removed after the memory-heavy phase.
