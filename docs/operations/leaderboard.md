# Internal Usage Leaderboard

Routes: `/admin/leaderboard` (all four admin roles) and
`/dashboard/leaderboard` (authenticated users, masked).
The read-only API is `/api/admin/stats/leaderboard/overview` for administrators
and `/api/leaderboard` for ordinary sessions. The latter always masks, even
when an administrator calls it. No new tables, migrations or production writes.
The MCP tool `arca_leaderboard_overview` uses the existing `admin:stats:read`
permission intersected with the key owner's current role.

## Metrics

- Model request ranking and user request/token/billed-USD rankings reuse the
  existing accounting leaderboard. Finalized failed/cancelled requests can count
  as requests; pending/streaming and placeholder providers are excluded.
  Tokens and billed amounts use the existing cache-aware accounting projections,
  not a second pricing calculation. Unattributed calls count in model/global
  totals but not user rankings.
- Estimated activity: each local calendar day's successful calls occupy unique
  five-minute buckets per user. Each occupied bucket counts five minutes.
  Multiple API keys, concurrent requests and retries do not multiply a bucket.
  This is a usage proxy, NOT measured human working time or job performance.
  A long-running request occupies its start bucket only; automated agents count.
- Daily activity refers to the range's last date. Weekly activity is the calendar
  week containing that date, starting Monday. Supported ranges all contain that
  week's observed portion: this week, last week, last 7/30 days.
- Active days and longest consecutive-day streak are limited to the selected
  range. No extrapolation of lifetime streaks.
- Hourly totals and top-three members use successful calls by start time, summed
  over the selected range in the selected fixed UTC offset.
- Early bird: earliest successful request at or after local 06:00 each day,
  ending at 24:00. Calls before 06:00 never win that day's early-bird award. Ties at
  second precision share first place. No-call days have no winner. Early wins
  count shared victories as one day for each winner.
- Night owl: estimated active minutes between local 22:00 inclusive and the next
  06:00 exclusive. Post-midnight minutes belong to the preceding night's start
  date. The range includes nights whose start dates fall in the selected range;
  its last night reads the next day's 00:00-06:00 too. That extension does not
  enter ordinary daily/hourly/activity-day statistics or accounting totals.
  `night_active` ranks cumulative minutes, `night_wins` counts nightly victories,
  and `night_owls` lists each night's winners, with equal minutes sharing first
  place. In-progress nights use observed records only, not future estimates.
- Rankings use dense ranks (1, 1, 2) and stable ID tie-breaking, with 20 rows
  maximum per board. A larger workload is not automatically better productivity.

Activity/early-bird/night-owl calculations require retained `usage_billing_facts` details;
historical accounting rollups may outlive those details. The two metric families
can therefore cover different retention histories. `activity_observed_from`
reports the earliest observed local day, not a guarantee that no earlier data
was deleted. The UI explicitly labels detail-dependent metrics.

## Privacy and Resource Boundaries

Ordinary users receive generated `member-N` IDs/labels, never real user IDs,
usernames, email addresses, per-user USD values, aggregate USD spend or precise
early-bird times. Member numbers are only report-local pseudonyms, not permanent
identities or a formal anonymity guarantee. Public user boards are suppressed
when the report includes fewer than three users. Hourly leaders require three
distinct members in that hour. No prompts, API keys, request IDs or bodies are
read for activity aggregation.

The user endpoint accepts only `preset` and `tz_offset_minutes`; duplicate or
unknown filters, arbitrary exact ranges and invalid offsets are rejected. This
prevents requesting named users or drilling down to per-request public records.
The fixed UTC offset is explicit (default +480 minutes); it is not an IANA zone
with historical DST transitions. All responses use `Cache-Control: no-store`.
Disabled/revoked user sessions use the existing login-session checks.

PostgreSQL groups in the adapter, bounds scans to at most 31 days (the 30-day
leaderboard may read an additional six hours for its final night) and imposes an
eight-second activity-query statement timeout. More than 100,000 grouped rows
fails rather than publishing a truncated ranking. Other summary queries retain
their existing accounting guards. No storage reader returns 503, not fake zeros.
There is no automatic refresh loop. Production capacity should be profiled before
broad rollout; a precomputed daily activity rollup is a future scale option.

Port 5188 is a synthetic frontend demo. Its fake leaderboard is not evidence of
production data or tunnel status. No production deployment is performed here.
