# Model Family UI

## Scope

The user-entitlements admin page owns a single local draft for family definitions,
timezone, default allocation, group allocations and personal overrides. No production
deployment or backend entitlement evaluation is performed by this UI change.

- `InternalAccessConfig.model_families?: Record<stableId, { name, models }>` stores
  up to 32 definitions. New IDs use `f-${crypto.randomUUID()}` and are read-only after
  creation; editing the display name does not change allocations or ledger identity.
- Members are 1..256 unique exact public model names from the paginated admin global
  model catalog (`name`, never `id`, display name, upstream name or prefix). Existing
  members missing from the current catalog remain visible as invalid selections and
  can be removed; they are not silently erased on unrelated budget edits.
- `Allocation.families?: Record<familyId, ModelAllowance>` adds shared family sub-pools
  without replacing `models`. Each finite allocation permits at most 15 model and
  family sub-pools combined. Family windows are rolling 5h, day, week and month, in USD.
- Explicit unlimited saves `{ unlimited: true, models: {} }`, dropping both sub-pool
  types and total caps. Empty optional family fields are omitted for compatibility.
- Definitions do not grant model access or add money to the total pool. A personal
  allocation fully replaces group allocations. Same-group/multi-group family merging
  uses the backend's existing most-permissive allowance semantics. All applicable
  windows from overlapping families constrain a request, with one total-pool ledger
  charge, not an additional family credit or charge.

## Draft and CAS

`InternalAccessPolicies.vue` keeps raw per-subject editors separately from loaded
config so incomplete rows and edits survive subject switches and failed saves.
Merely viewing a subject never creates a new override. A save validates all changed
subjects and definitions, then writes one complete config through `saveInternalAccess`
and its existing `expected_value` snapshot. A rejected write does not replace the
snapshot or clear drafts. A CAS conflict requires reconciliation with the latest
server policy; retrying without reconciliation still retains the original CAS guard.

The refresh action explicitly reloads server state and replaces local drafts. Saving
does not depend on having a subject selected, so global family edits can be committed
independently. Catalog load failure is shown separately and disables member selection
without preventing budget-only edits.

Deletion checks all default/group/user references, including unseen subjects and
unsaved raw rows. Referenced families cannot be deleted. Remove the references
explicitly first, then delete the definition in the same atomic save. Toggling
unlimited does not silently remove raw rows before save; reference protection stays
conservative until those references are explicitly removed or unlimited is saved.
Failed personal-override removal stays pending and can be retried via Save or
cancelled with the keep-personal-override action.

The existing `canManageUsers` gate permits super/user admins to edit and leaves audit
admins read-only. Existing admin routing denies service admins access to this page.

## Entitlement Preview Integration

The independent `EntitlementPreview` in `InternalAccessPolicies.vue` sits after subject
selection and before the allocation editor. The main agent owns
`components/EntitlementPreview.vue` and `api/access-preview.ts`; this change does not
implement the preview or change `api/account.ts` / `Profile.vue`.

`buildDraft(): InternalAccessConfig` is synchronous, exposed by the page, and is the
same validation/assembly path used by Save. It returns a detached config containing
all unsaved definitions and subject edits, not just the visible allocation. It does
not write config, update the CAS snapshot or discard editor state. Invalid drafts
throw the same validation errors as Save.

The integrated component receives:

```vue
<EntitlementPreview
  :users="users"
  :get-draft="buildDraft"
  :selected-user="targetType === 'user' ? targetId : undefined"
  :disabled="loading || !loaded"
/>
```

It is imported from `./components/EntitlementPreview.vue`. The component owns the call to
`POST /api/admin/access-preview` with `{ user_id, api_key_id?, config? }` and current /
proposed scenario rendering. Preview must not persist draft state.

## Verification

From `frontend/`:

```sh
npm run test:run
npm run type-check
```

API tests cover atomic family payloads and immutable CAS snapshots across rejected
writes. Page tests cover multiple families, catalog pagination, stable IDs, exact
public member selection, shared-window amounts, subject switching, CAS retry,
referenced deletion, optional-field compatibility, amount/size limits, unlimited,
audit read-only behavior, and existing model/total budget behavior.
