<template>
  <div :class="mobile ? 'flex flex-wrap items-center justify-end gap-1 border-t border-border pt-3' : 'flex justify-center gap-1'">
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      :title="legacyT('编辑用户')"
      @click="$emit('edit')"
    >
      <SquarePen :class="iconClass" />
      <span class="sr-only">{{ legacyT('编辑') }}</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      :title="legacyT('资金操作')"
      v-show="!INTERNAL_GATEWAY"
      @click="$emit('wallet')"
    >
      <DollarSign :class="iconClass" />
      <span class="sr-only">{{ legacyT('资金') }}</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      :title="legacyT('套餐')"
      v-show="!INTERNAL_GATEWAY"
      @click="$emit('plans')"
    >
      <PackageCheck :class="iconClass" />
      <span class="sr-only">{{ legacyT('套餐') }}</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      title="API Keys"
      @click="$emit('api-keys')"
    >
      <Key :class="iconClass" />
      <span class="sr-only">API Keys</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      :title="legacyT('登录设备')"
      @click="$emit('sessions')"
    >
      <MonitorSmartphone :class="iconClass" />
      <span class="sr-only">{{ legacyT('设备') }}</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9"
      :title="isActive ? legacyT('禁用用户') : legacyT('启用用户')"
      @click="$emit('toggle-status')"
    >
      <PauseCircle
        v-if="isActive"
        :class="iconClass"
      />
      <PlayCircle
        v-else
        :class="iconClass"
      />
      <span class="sr-only">{{ legacyT(isActive ? '禁用' : '启用') }}</span>
    </Button>
    <Button
      v-if="canOperateAdmin"
      :variant="mobile ? 'outline' : 'ghost'"
      size="icon"
      class="h-9 w-9 text-destructive hover:text-destructive"
      :title="legacyT('删除用户')"
      @click="$emit('delete')"
    >
      <Trash2 :class="iconClass" />
      <span class="sr-only">{{ legacyT('删除') }}</span>
    </Button>
  </div>
</template>

<script setup lang="ts">
import { INTERNAL_GATEWAY } from '@/config/internalGateway'
import {
  DollarSign,
  Key,
  MonitorSmartphone,
  PackageCheck,
  PauseCircle,
  PlayCircle,
  SquarePen,
  Trash2,
} from 'lucide-vue-next'
import Button from '@/components/ui/button.vue'
import { useI18n } from '@/i18n'

withDefaults(defineProps<{
  canOperateAdmin: boolean
  isActive: boolean
  mobile?: boolean
}>(), {
  mobile: false,
})

defineEmits<{
  edit: []
  wallet: []
  plans: []
  'api-keys': []
  sessions: []
  'toggle-status': []
  delete: []
}>()

const { legacyT } = useI18n()
const iconClass = 'h-4 w-4'
</script>
