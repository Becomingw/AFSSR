# Arca Internal Gateway

## Status, 2026-10-15

This is a local development change, not a deployed production release. HK and
Korean staging have not been restarted or changed by this implementation.
No production data, credentials, or paid model requests were used. The new
PostgreSQL migration and admission query were also exercised on a disposable
local database: per-model isolation, shared caps, idempotency, identity conflicts,
and concurrent admission passed without overselling. CI now requires that test
to actually start PostgreSQL rather than silently skipping it.

The first implementation milestone covers internal user budgets and optional model sub-pools,
commercial UI/API retirement, and three primary client generation protocols.
It does not complete every item in the broader optimization roadmap below.

## Product Boundary

- Primary client APIs: OpenAI Responses (including Codex), Claude Messages,
  and OpenAI Chat Completions. Native Gemini remains an upstream adapter;
  its public generation routes are retired in internal mode.
- Provider credentials, model mappings, cost pricing, routing, auth, audit,
  encryption, backup, and tunnel egress remain core infrastructure.
- Payment, recharge, refunds, redeem codes, referrals, and plan purchase UI
  routes are removed. Their operational API paths return 404 in internal mode.
  Referral rewards and new-account gift credits are disabled.
- Historical financial tables and compatibility implementations are retained;
  there is no destructive financial-data migration. Internal requests carry a
  server-generated `internal_accounting` fact: provider cost and usage remain
  recorded, but user wallets and paid daily quotas are not charged. Historical
  requests without this fact retain their original settlement behavior.
- Pricing rules/collectors are kept for internal cost measurement, not payments.

`ARCA_INTERNAL_MODE` defaults to true in normal binaries. The false setting is
a legacy migration/regression escape hatch, not the intended internal deployment.
Legacy unit tests default to legacy mode; new pure-policy tests exercise internal
rules explicitly. The frontend is an internal-only build regardless of this flag.

## Entitlement Semantics

The admin page is `/admin/access-policies`, named User Benefits (`用户权益`).
It manages total budgets and optional model sub-pools, not model permissions.
Model availability is edited in the individual user's profile. In internal mode,
an explicit personal model list or deny-all policy replaces group model policy;
only the inherit setting reads group grants. Provider and API-key restrictions
remain separate security constraints and cannot be bypassed by quota settings.

The configuration key is `internal_access_policy`:

```json
{
  "timezone": "Asia/Shanghai",
  "groups": {
    "REPLACE_WITH_REAL_GROUP_ID": {
      "unlimited": false,
      "five_hour_budget_usd": 5,
      "weekly_budget_usd": 30,
      "monthly_budget_usd": 100,
      "models": {
        "PUBLIC_MODEL_NAME": {
          "five_hour_budget_usd": 2.5,
          "daily_budget_usd": 5,
          "weekly_budget_usd": 20,
          "monthly_budget_usd": 60
        }
      }
    }
  },
  "users": {
    "REPLACE_WITH_REAL_USER_ID": { "unlimited": true, "models": {} }
  }
}
```

This is a shape example, not a proposed allocation to existing users. The numbers
are not applied automatically. Group and user identifiers must be deliberately
selected by an administrator.

- Personal allocation replaces all inherited group allocation. Removing it
  restores group benefits. This is a full allocation replacement, not a merge:
  omitted personal budgets are unbounded, and omitted personal sub-pools do not
  fall back to group sub-pools. It can both increase and reduce an individual's quota.
- Optional model sub-pools union across assigned groups when no personal
  allocation exists. For the same model/window, the
  larger USD allowance wins; a missing model money bound means no bound for that
  window. Different windows are enforced together, not added into one number.
- The largest explicitly configured group total budget wins separately for each
  rolling-five-hour, weekly and monthly window; an omitted total budget does not
  cancel another group's budget. `unlimited: true` is
  the explicit quota exemption, including for administrator inference keys.
- `default` is optional and is used only when no assigned group has an allocation.
  No configured budget means no quota limit; it does not grant model access.
  Configure a deliberate default total budget before enabling internal traffic.
  An empty model map means no extra sub-pools: every otherwise authorized model
  still uses the total pool. Removing a sub-pool does not revoke model access.
- Model sub-pool keys are exact public catalog names. Recognized reasoning suffixes
  and catalog-resolved names normalize before counting. Quota entries are never
  used to grant model access.
- `*` is a compatibility aggregate-budget entry, not a permission wildcard. Its money limits are aggregate
  user limits across models; named model caps remain additional constraints. When
  a wildcard and a total budget address the same window, the stricter bound wins
  without duplicate windows or duplicate charging.
- An omitted limit is unlimited; zero is invalid, not unlimited. Remove the model
  entry to remove only its extra sub-pool. Revoke model access on the user's profile.
  A model entry with `{}` adds no further quota limit.
- Five hours is a sliding 18,000-second window. Days and months reset at local
  midnight; weeks start Monday in the configured IANA timezone.
- All internal limits are monetary, not request counts. Total USD caps cover
  rolling five hours, calendar week and month; each model also supports a calendar
  day cap. All keys belonging to a user share these budgets. HTTP and Responses
  WebSocket turns use the same policy engine.
- USD uses the gateway's configured actual-cost accounting. Estimated
  request cost is reserved before execution and reconciled against terminal usage.
  Concurrent pending reservations consume budget. Missing usable pricing or usage
  infrastructure fails closed for a bounded-cost user. This is not a guarantee
  that an upstream vendor will never violate its estimated output bound.
- A request is stored once in the cost ledger with its normalized public model;
  total windows sum all models, scoped windows sum that model only. Reconciliation
  and failure release update this same reservation, not separate copies per cap.
  A sub-pool is included in the total, not additional credit: one USD of scoped
  spend consumes one USD of each applicable total window and sub-pool window.
  A request must fit every applicable window; unused sub-pool credit cannot bypass
  a depleted total pool, and unused total credit cannot bypass a depleted sub-pool.
- Legacy `*_requests` configurations are rejected, never interpreted as dollars
  or silently cleared by the editor. An administrator must explicitly replace
  the policy with approved money allocations using expected-old-value matching.
- Old cost reservations have NULL model: they continue to affect total windows,
  but unknown historical model assignments are not guessed/backfilled. Traffic
  without any active money budget (including unlimited users) has no quota cost
  reservation; switching such users to bounded allocations does not reconstruct
  historical quota spend. General usage audit records remain unchanged.
- Unlimited exempts usage quotas, not identity verification, model/key ACLs,
  global server capacity, or other existing safety/rate/concurrency protections.

## Administrative Roles

| Role | Stored value | Responsibility |
| --- | --- | --- |
| Super administrator / 超级管理员 | `admin` | All administrative domains; existing super accounts keep this value. |
| Audit administrator / 审计管理员 | `audit_admin` | Read service health, model usage and ordinary administrative views; no mutations or credential reveals. |
| User and benefits administrator / 用户及权益管理员 | `user_admin` | Ordinary users, model permissions, group memberships, budgets, announcements and downstream API keys. |
| Service administrator / 服务管理员 | `service_admin` | Providers, upstream models/credentials, account pools, routing, proxy/tunnel nodes and backend tasks. |

Only a super administrator can assign administrative roles, change administrative
accounts or their user credentials, manage management tokens, change global
authentication/security settings, or perform whole-site import/export and recovery.
Delegated administrators cannot demote, disable, delete or reset another
administrator through the ordinary user-management routes. Existing last-super
protection remains in force. Group membership never grants an administrative role.

Role checks run server-side before administrative dispatch, including session
requests. Management-token permissions are intersected with the account's current
role; an old broad token cannot preserve privileges after a role downgrade.
Tokens can narrow but cannot widen role permissions. Operational APIs and tunnel
authentication enforce the same domain boundaries. Sensitive audit payloads and
credential execution/reveal actions retain their existing stronger permissions.

Scoped roles have filtered menus and route guards. User administrators may read
model/provider metadata for user model selectors, but not change upstreams;
service administrators may read the group list for routing selectors, but not
change memberships or benefits. Shared usage and health views are read-only.
The benefits editor is read-only for audit administrators.

`20261015000200_split_admin_roles.sql` adds two PostgreSQL enum values without
changing any existing user role. Run it before assigning the new roles. A rollback
to software that does not recognize scoped roles requires a super administrator
to deliberately reassign those accounts first; do not drop enum values or rewrite
all administrators automatically. This local change has not assigned live roles.

## Policy Write Safety

Administrative writes use the existing system-config API, validation and audit.
They require `expected_value` equal to the last read JSON (null for first creation).
The shared import/configuration lock serializes the check and write; stale writes
return 409 instead of overwriting another administrator's allocation. Deletion
of this policy through the generic config DELETE API is disabled.

Multi-gateway deployments must share the runtime coordination backend. Policy
reads use strong config reads rather than the ordinary five-minute stale cache.
Bulk data import remains a privileged operational operation, not the normal
policy editor. Imported invalid policy fails closed on use.

## Schema And Release Safety

The additive migrations `20261015000000_internal_model_request_quotas.sql` and
`20261015000100_internal_model_cost_quotas.sql` add nullable model columns and
lookup indexes to request admissions and cost reservations. Request-count ledger
support is retained for legacy mode but is not used by internal money allocations.
These migrations do not delete,
rewrite, or migrate users, wallets, provider secrets, or historical usage.
Bootstrap and logical schema are updated alongside the migration.

Before switching any running instance:

1. Take a protected database backup and retain the existing application image.
2. Build a pinned image from the complete checkout, including existing Go fixes.
3. Run migration and quota integration tests on a disposable PostgreSQL database.
4. Configure real group/user allocations and personal model permissions; verify personal-over-group precedence.
5. Validate admission, 429/reset behavior, model isolation, all total/model money windows,
   all three client protocols, HTTP/SSE and WebSocket with a synthetic upstream.
6. Only then schedule an application switch. Preserve all existing secrets.

Application rollback does not require dropping the nullable columns or indexes,
but the previous application cannot parse the new money policy. Before a code
rollback, restore its compatible policy snapshot under maintenance and CAS checks.
Do not convert money back into request counts automatically.
Do not regenerate encryption/JWT/DB secrets or run destructive down migrations.
Do not mix legacy and internal-mode usage workers during rollout: every writer
must preserve the internal accounting marker before internal traffic is enabled.

## Verification

Offline checks require no real API key:

```sh
cargo test --locked -p aether-gateway --lib internal_access::tests
cargo test --locked -p aether-gateway --lib plan_usage_policy::tests
RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway --lib scoped_admin
RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway --lib management_token
RUST_MIN_STACK=16777216 cargo test --locked -p aether-gateway --lib tests::control::admin::users
cargo test --locked -p aether-data --lib repository::settlement
cargo test --locked -p aether-usage-runtime --lib settlement::tests
cargo test --locked -p aether-ai-formats --test internal_gateway_protocols
bash crates/aether-data/runtime/schema/compose_schema.sh check
npm --prefix frontend run type-check
npm --prefix frontend run test:run
```

The protocol matrix covers all three client formats against Responses, Chat,
Messages and Gemini upstream formats: function declarations, returned arguments,
tool identity and tool-result continuation. It is a conversion contract test,
not live proof that every model performs arbitrary coding tasks reliably.

Console presentation uses scoped neutral light/dark tokens, compact shared
controls, consistent page headings and unframed settings sections. User filters
are a single accessible set at all breakpoints; initial loading and empty users
are distinct states. Model sub-pool fields reflow into two columns on phones.

Browser verification uses synthetic users and local fixture API responses,
not real login. Sample user, benefits, provider, model and dashboard views were
checked at 1440px desktop and 390px mobile, including dark mode and audit read-only
controls. Fixture statistics are illustrative, never production health evidence.

The gateway frontend builds directly with Vite. The inherited `npm run build`
also attempts to install/build the optional remote Codex sidecar; that path needs
its own dependencies and is not proof that sidecar packaging is ready.

## Remaining Roadmap

- Korean staging migration and HTTP/WebSocket enforcement acceptance before deployment.
- User-visible remaining quotas, pending reservations and exact reset times.
- Per-model/client/version compatibility records and capability-aware selection.
- Production-owned CI/release images instead of inherited upstream publishing.
- Unified effective channel configuration and request failure diagnosis.
- Shared compatibility normalization across planning paths, with regression tests.
- Merge overlapping observability navigation without removing metrics.
- Optional-module packaging, followed by measured performance optimization.
- Long-history, compaction, interruption and real-client/model acceptance; ordinary
  HTTP 200 must never be promoted to full tool/task compatibility.
