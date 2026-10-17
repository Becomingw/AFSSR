<template>
  <article class="min-w-0 border-b border-border bg-card px-4 py-4">
    <div class="space-y-3">
      <div class="flex items-start gap-3">
        <Checkbox
          class="mt-2 shrink-0"
          :checked="selected"
          :disabled="selectionDisabled"
          @update:checked="(checked) => $emit('toggle-selected', checked === true)"
        />
        <UserIdentityCell
          :row="row"
          :show-groups="false"
        />
      </div>

      <UserStatusBadges
        :row="row"
        mobile
      />

      <UserWalletSummary
        :row="row"
        mobile
      />

      <div class="grid grid-cols-2 gap-2.5 text-xs">
        <div>
          <div class="mb-1 text-muted-foreground">
            {{ legacyT('请求次数') }}
          </div>
          <div class="font-semibold text-foreground">
            {{ row.requestCountLabel }}
          </div>
        </div>
        <div>
          <div class="mb-1 text-muted-foreground">
            Tokens
          </div>
          <div class="font-semibold text-foreground">
            {{ row.tokensLabel }}
          </div>
        </div>
      </div>

      <div class="text-xs text-muted-foreground">
        <div class="flex items-center justify-between gap-2">
          <span>{{ legacyT('创建时间') }}</span>
          <span class="font-medium text-foreground">{{ row.createdAtLabel }}</span>
        </div>
      </div>

      <UserActionButtons
        :can-operate-admin="canOperateAdmin"
        :is-active="row.user.is_active"
        mobile
        @edit="$emit('edit')"
        @wallet="$emit('wallet')"
        @plans="$emit('plans')"
        @api-keys="$emit('api-keys')"
        @sessions="$emit('sessions')"
        @toggle-status="$emit('toggle-status')"
        @delete="$emit('delete')"
      />
    </div>
  </article>
</template>

<script setup lang="ts">
import Checkbox from '@/components/ui/checkbox.vue'
import { useI18n } from '@/i18n'
import UserActionButtons from './UserActionButtons.vue'
import UserIdentityCell from './UserIdentityCell.vue'
import UserStatusBadges from './UserStatusBadges.vue'
import UserWalletSummary from './UserWalletSummary.vue'
import type { UserManagementRow } from './user-management-types'

defineProps<{
  row: UserManagementRow
  selected: boolean
  selectionDisabled: boolean
  canOperateAdmin: boolean
}>()

defineEmits<{
  'toggle-selected': [checked: boolean]
  edit: []
  wallet: []
  plans: []
  'api-keys': []
  sessions: []
  'toggle-status': []
  delete: []
}>()

const { legacyT } = useI18n()
</script>
