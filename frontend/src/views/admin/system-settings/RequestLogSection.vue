<template>
  <CardSection
    title="请求记录"
    description="控制请求/响应详情的入库方式和内容"
  >
    <template #actions>
      <Button
        size="sm"
        :disabled="loading || !hasChanges"
        @click="$emit('save')"
      >
        {{ loading ? '保存中...' : '保存' }}
      </Button>
    </template>
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <div>
        <Label
          for="request-log-level"
          class="block text-sm font-medium mb-2"
        >
          记录详细程度
        </Label>
        <Select
          :model-value="requestRecordLevel"
          @update:model-value="$emit('update:requestRecordLevel', $event)"
        >
          <SelectTrigger
            id="request-log-level"
            class="mt-1"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="basic">
              BASIC - 基本信息 (~1KB/条)
            </SelectItem>
            <SelectItem value="headers">
              HEADERS - 含请求头 (~2-3KB/条)
            </SelectItem>
            <SelectItem value="failures">
              FAILURES - 仅失败请求保存完整请求响应
            </SelectItem>
            <SelectItem value="full">
              FULL - 完整请求响应
            </SelectItem>
          </SelectContent>
        </Select>
        <p class="mt-1 text-xs text-muted-foreground">
          敏感信息会自动脱敏。排查上游兼容问题时建议使用 FAILURES：成功请求不留正文，失败请求保留转换后发给上游的请求体和上游返回的错误。
        </p>
      </div>

      <div>
        <Label
          for="sensitive-headers"
          class="block text-sm font-medium"
        >
          敏感请求头
        </Label>
        <Input
          id="sensitive-headers"
          :model-value="sensitiveHeadersStr"
          placeholder="authorization, x-api-key, cookie"
          class="mt-1"
          @update:model-value="$emit('update:sensitiveHeadersStr', $event)"
        />
        <p class="mt-1 text-xs text-muted-foreground">
          逗号分隔，这些请求头会被脱敏处理
        </p>
      </div>
    </div>
  </CardSection>
</template>

<script setup lang="ts">
import Button from '@/components/ui/button.vue'
import Input from '@/components/ui/input.vue'
import Label from '@/components/ui/label.vue'
import Select from '@/components/ui/select.vue'
import SelectTrigger from '@/components/ui/select-trigger.vue'
import SelectValue from '@/components/ui/select-value.vue'
import SelectContent from '@/components/ui/select-content.vue'
import SelectItem from '@/components/ui/select-item.vue'
import { CardSection } from '@/components/layout'

defineProps<{
  requestRecordLevel: string
  sensitiveHeadersStr: string
  loading: boolean
  hasChanges: boolean
}>()

defineEmits<{
  save: []
  'update:requestRecordLevel': [value: string]
  'update:sensitiveHeadersStr': [value: string]
}>()
</script>
