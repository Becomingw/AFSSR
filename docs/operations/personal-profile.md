# Personal Profile

The self-service account page is `/dashboard/settings` (`/dashboard/profile` is
an alias). It contains usage and entitlements, personal information, security,
and preferences. It reuses existing profile, session, password, OAuth, and
preference handlers. It does not expose wallet or payment controls in internal
mode. Email is read-only here because changing it requires a verified token.

## Entitlements Snapshot

`GET /api/users/me/entitlements` requires a live local access-token session.
It accepts no filters or caller-selected subject. Administrators also receive
only their own snapshot. Responses are `Cache-Control: no-store`.

When internal mode is disabled, the result is `{ "enabled": false }`.
Otherwise it includes effective allocation source, policy timezone, generation
time, and pool windows. Personal allocations replace group/default settings.
Model allowances do not grant model access; the available-models endpoint
remains authoritative for that. Wildcard allowances merge into the total pool
using the same stricter-window rule as admission.

The snapshot reads `usage_cost_reservations`, matching cost admission:

- Finalized reservations contribute `actual_cost_units`.
- Unexpired reserved entries contribute `reserved_cost_units`.
- Expired or released entries do not consume a pool.
- Windows use `admitted_at`, with inclusive start and exclusive end.
- Model subpools remain included in total usage, not additional credits.

The total pool always shows rolling five-hour, calendar-week and calendar-month
usage, even without a cap. Calendar periods reuse admission's timezone-aware
bounds. Rolling five-hour usage releases gradually and has no fixed reset time.
Quota read failure returns 503 rather than fabricated zero usage or unlimited
access. Queries are bounded to 64 windows of at most 32 days, with an eight-second
PostgreSQL statement timeout. No schema migration is required.

The separate call overview uses `/api/users/me/usage`. Its standard-rate cost is
not necessarily the actual cost charged to entitlements; the UI labels these
separately. Interval changes discard stale responses.

## Token Activity

The profile also reads the existing self-scoped
`GET /api/users/me/usage/heatmap` endpoint for its last 365 days. This endpoint
currently buckets dates in UTC; the graph explicitly labels that timezone.
Colors measure Token totals, not request counts. Daily cells use a Monday-first
calendar; weekly bars sum each visible calendar week, including partial edge
weeks. Cumulative cells show running totals within the displayed date range,
not lifetime usage. Switching modes is local and does not issue new requests.

The activity section has its own loading/error state and explicit refresh that
invalidates the existing 30-minute heatmap cache. It does not follow the shorter
call-overview date filter. Hover, keyboard focus and touch expose exact values;
the year scrolls horizontally on small screens without overflowing the page.
No schema or backend endpoint changes are required.

## Verification

Frontend tests cover self-scoped API calls, subpools, active reservations,
unlimited caps, clamped progress, independent error states, and interval races.
Gateway tests exercise session ownership for all five roles and reject subject
overrides. The PostgreSQL integration test requires an explicitly supplied
loopback `afssr_feedback_test` database and creates/removes its own random schema:

```sh
AFSSR_ACCOUNT_USAGE_TEST_DATABASE_URL=postgres://afssr_feedback_test@127.0.0.1:55841/afssr_feedback_test \
  cargo test -p aether-data-postgres --lib personal_usage_ledger_isolation -- --ignored
```
