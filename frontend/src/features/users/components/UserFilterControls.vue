<template>
  <div class="grid min-w-0 w-full grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:items-center">
    <div class="relative col-span-2 min-w-0 sm:w-64 sm:flex-none">
      <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground z-10 pointer-events-none" />
      <Input
        :id="mobile ? 'users-search-mobile' : 'users-search'"
        :model-value="searchQuery"
        type="text"
        :aria-label="legacyT('搜索用户名或邮箱')"
        :placeholder="searchPlaceholder"
        class="h-9 w-full pl-8 pr-3 text-sm"
        @update:model-value="$emit('update:searchQuery', $event)"
      />
    </div>

    <div class="min-w-0 sm:w-44">
      <Select
        :model-value="filterRole"
        @update:model-value="updateRole"
      >
        <SelectTrigger class="h-9 w-full text-xs" :aria-label="legacyT('角色')">
          <SelectValue :placeholder="legacyT(mobile ? '角色' : '全部角色')" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem
            v-for="option in roleOptions"
            :key="option.value"
            :value="option.value"
          >
            {{ legacyT(option.label) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>

    <div class="min-w-0 sm:w-28">
      <Select
        :model-value="filterStatus"
        @update:model-value="updateStatus"
      >
        <SelectTrigger class="h-9 w-full text-xs" :aria-label="legacyT('状态')">
          <SelectValue :placeholder="legacyT(mobile ? '状态' : '全部状态')" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem
            v-for="option in statusOptions"
            :key="option.value"
            :value="option.value"
          >
            {{ legacyT(option.label) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>

    <Select
      :model-value="filterGroup"
      @update:model-value="$emit('update:filterGroup', $event)"
    >
      <SelectTrigger class="h-9 w-full text-xs sm:w-36" :aria-label="legacyT('分组')">
        <SelectValue :placeholder="legacyT(mobile ? '分组' : '全部分组')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">
          {{ legacyT(mobile ? '全部' : '全部分组') }}
        </SelectItem>
        <SelectItem
          v-for="group in userGroups"
          :key="group.id"
          :value="group.id"
        >
          {{ group.name }}
        </SelectItem>
      </SelectContent>
    </Select>

    <div class="min-w-0 sm:w-40">
      <Select
        :model-value="sortOption"
        @update:model-value="updateSort"
      >
        <SelectTrigger class="h-9 w-full text-xs" :aria-label="legacyT('排序')">
          <SelectValue :placeholder="legacyT('排序')" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem
            v-for="option in sortOptions"
            :key="option.value"
            :value="option.value"
          >
            {{ legacyT(option.label) }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Search } from 'lucide-vue-next'
import Input from '@/components/ui/input.vue'
import Select from '@/components/ui/select.vue'
import SelectTrigger from '@/components/ui/select-trigger.vue'
import SelectValue from '@/components/ui/select-value.vue'
import SelectContent from '@/components/ui/select-content.vue'
import SelectItem from '@/components/ui/select-item.vue'
import type { UserGroup, UserRole } from '@/api/users'
import { useI18n } from '@/i18n'

type FilterRole = 'all' | UserRole
type FilterStatus = 'all' | 'active' | 'inactive'
type SortOption = 'default' | 'created_at_desc' | 'created_at_asc'

interface FilterOption<TValue extends string = string> {
  value: TValue
  label: string
}

const props = withDefaults(defineProps<{
  searchQuery: string
  filterRole: FilterRole
  filterGroup: string
  filterStatus: FilterStatus
  sortOption: SortOption
  userGroups: UserGroup[]
  roleOptions: FilterOption<FilterRole>[]
  statusOptions: FilterOption<FilterStatus>[]
  sortOptions: FilterOption<SortOption>[]
  mobile?: boolean
}>(), {
  mobile: false,
})

const emit = defineEmits<{
  'update:searchQuery': [value: string]
  'update:filterRole': [value: FilterRole]
  'update:filterGroup': [value: string]
  'update:filterStatus': [value: FilterStatus]
  'update:sortOption': [value: SortOption]
}>()

function updateRole(value: string) {
  const option = props.roleOptions.find((option) => option.value === value)
  if (option) emit('update:filterRole', option.value)
}

function updateStatus(value: string) {
  const option = props.statusOptions.find((option) => option.value === value)
  if (option) emit('update:filterStatus', option.value)
}

function updateSort(value: string) {
  const option = props.sortOptions.find((option) => option.value === value)
  if (option) emit('update:sortOption', option.value)
}

const { legacyT } = useI18n()
const searchPlaceholder = computed(() => props.mobile ? legacyT('搜索...') : legacyT('搜索用户名或邮箱...'))
</script>
