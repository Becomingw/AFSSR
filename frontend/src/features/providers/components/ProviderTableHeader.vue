<template>
  <div class="pb-3 border-b border-border">
    <div class="flex min-w-0 flex-col gap-4">
      <!-- 左侧：标题 -->
      <h1 class="console-page-title">
        {{ legacyT('提供商管理') }}
      </h1>

      <!-- 右侧：操作区 -->
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <!-- 搜索框 -->
        <div class="relative min-w-40 flex-1 sm:w-64 sm:flex-none">
          <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/70 z-10 pointer-events-none" />
          <Input
            id="provider-search"
            :model-value="searchQuery"
            type="text"
            :aria-label="legacyT('搜索提供商')"
            :placeholder="legacyT('搜索提供商...')"
            class="w-full pl-8 pr-3 h-9 text-sm"
            @update:model-value="$emit('update:searchQuery', $event)"
          />
        </div>

        <!-- 状态筛选 -->
        <div :class="{ 'xl:hidden': !cardView }">
          <Select
            :model-value="filterStatus"
            @update:model-value="$emit('update:filterStatus', $event)"
          >
            <SelectTrigger class="w-20 sm:w-28 h-8 text-xs border-border/60">
              <SelectValue :placeholder="legacyT('全部状态')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem
                v-for="status in statusFilters"
                :key="status.value"
                :value="status.value"
              >
                {{ legacyT(status.label) }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- API 格式筛选 -->
        <div :class="{ 'xl:hidden': !cardView }">
          <Select
            :model-value="filterApiFormat"
            @update:model-value="$emit('update:filterApiFormat', $event)"
          >
            <SelectTrigger class="w-20 sm:w-28 h-8 text-xs border-border/60">
              <SelectValue :placeholder="legacyT('全部格式')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem
                v-for="fmt in apiFormatFilters"
                :key="fmt.value"
                :value="fmt.value"
              >
                {{ legacyT(fmt.label) }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- 模型筛选 -->
        <div :class="{ 'xl:hidden': !cardView }">
          <Select
            :model-value="filterModel"
            @update:model-value="$emit('update:filterModel', $event)"
          >
            <SelectTrigger class="w-20 sm:w-36 h-8 text-xs border-border/60">
              <SelectValue :placeholder="legacyT('全部模型')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem
                v-for="model in modelFilters"
                :key="model.value"
                :value="model.value"
              >
                {{ legacyT(model.label) }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- 重置筛选 -->
        <Button
          v-if="hasActiveFilters"
          variant="ghost"
          size="icon"
          class="h-8 w-8"
          :title="legacyT('重置筛选')"
          @click="$emit('resetFilters')"
        >
          <FilterX class="w-3.5 h-3.5" />
        </Button>

        <div class="hidden sm:block h-4 w-px bg-border" />

        <!-- 操作按钮 -->
        <Button
          variant="ghost"
          size="icon"
          class="h-8 w-8"
          :title="legacyT('批量处理提供商')"
          :disabled="loading"
          @click="$emit('batchProcess')"
        >
          <Users class="w-3.5 h-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          class="h-8 w-8"
          :title="legacyT('新增提供商')"
          @click="$emit('addProvider')"
        >
          <Plus class="w-3.5 h-3.5" />
        </Button>
        <RefreshButton
          :loading="loading"
          @click="$emit('refresh')"
        />
        <Button
          variant="ghost"
          size="icon"
          class="h-8 w-8"
          :class="{ 'bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary': cardView }"
          :title="legacyT(cardView ? '切换到列表视图' : '切换到卡片视图')"
          :aria-label="legacyT('卡片视图')"
          :aria-pressed="cardView"
          @click="$emit('toggleView')"
        >
          <List
            v-if="cardView"
            class="w-3.5 h-3.5"
          />
          <LayoutGrid
            v-else
            class="w-3.5 h-3.5"
          />
        </Button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Search, Plus, FilterX, Users, LayoutGrid, List } from 'lucide-vue-next'
import Button from '@/components/ui/button.vue'
import Input from '@/components/ui/input.vue'
import Select from '@/components/ui/select.vue'
import SelectTrigger from '@/components/ui/select-trigger.vue'
import SelectValue from '@/components/ui/select-value.vue'
import SelectContent from '@/components/ui/select-content.vue'
import SelectItem from '@/components/ui/select-item.vue'
import RefreshButton from '@/components/ui/refresh-button.vue'
import type { FilterOption } from '@/features/providers/composables/useProviderFilters'
import { useI18n } from '@/i18n'

defineProps<{
  searchQuery: string
  filterStatus: string
  filterApiFormat: string
  filterModel: string
  statusFilters: FilterOption[]
  apiFormatFilters: FilterOption[]
  modelFilters: FilterOption[]
  hasActiveFilters: boolean
  loading: boolean
  cardView: boolean
}>()

defineEmits<{
  'update:searchQuery': [value: string]
  'update:filterStatus': [value: string]
  'update:filterApiFormat': [value: string]
  'update:filterModel': [value: string]
  'resetFilters': []
  'batchProcess': []
  'addProvider': []
  'refresh': []
  'toggleView': []
}>()

const { legacyT } = useI18n()
</script>
