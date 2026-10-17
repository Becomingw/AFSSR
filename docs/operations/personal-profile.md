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

## Profile Picture

Every user can upload a picture in **设置 → 基本信息 → 头像** (PNG, JPEG, WebP or GIF). The browser
crops it to the centre square, scales it to 256x256 and re-encodes it (WebP, PNG where WebP is
unavailable), which also drops any metadata such as GPS positions; only that small version is sent
and stored. The previous free-text "头像 URL" field is gone: the stored `avatar_url` preference is
kept but no longer shown anywhere, because loading arbitrary third-party URLs in every viewer's
browser leaks who is looking and often fails on hotlink protection.

```text
PUT    /api/avatars/me        raw PNG/JPEG/WebP bytes, session required, 256 KiB at most
DELETE /api/avatars/me        removes it
GET    /api/avatars/{user_id}?v=<version>
```

- The server checks the image's magic bytes, so SVG (which can carry scripts) and anything else is
  refused with 415, and oversize bodies with 413. Pictures live in the `user_avatars` table and go
  away with their user (migration `20261020000000_user_avatars.sql`, additive and idempotent).
- `GET` needs **no session**: browsers cannot attach the in-memory access token to an `<img>`.
  The only handle is the unguessable user id, which the API only reveals to logged-in users, and
  the response is sandboxed (`nosniff`, `Content-Security-Policy: default-src 'none'; sandbox`).
  Treat avatars as visible to everyone on the deployment.
- A picture's version (Unix milliseconds of the last change) is part of its URL. Requests naming the
  current version are cached for a year (`immutable`); an old or missing version is revalidated
  after five minutes and answers `304` for an unchanged ETag.
- Responses that mention people carry the versions: `GET /api/users/me` has `avatar_version`, and the
  discussion list, detail and notification responses have `avatars: {user_id: version}` (people
  without a picture are absent), so pages never request pictures that do not exist.
- Shown on the profile page, in discussions, and in the notification bell; elsewhere (for example the
  administrator user list) users still show an initial.

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
