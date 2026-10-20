<template>
  <CardSection
    title="站点信息"
    description="自定义站点名称和副标题，影响导航栏、登录页、指南页面和邮件等全站显示"
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
          for="site-name"
          class="block text-sm font-medium"
        >
          站点名称
        </Label>
        <Input
          id="site-name"
          :model-value="siteName"
          type="text"
          placeholder="Arca"
          class="mt-1"
          @update:model-value="$emit('update:siteName', $event)"
        />
        <p class="mt-1 text-xs text-muted-foreground">
          显示在导航栏、登录页标题和邮件中
        </p>
      </div>
      <div>
        <Label
          for="site-subtitle"
          class="block text-sm font-medium"
        >
          站点副标题
        </Label>
        <Input
          id="site-subtitle"
          :model-value="siteSubtitle"
          type="text"
          placeholder="AI Gateway"
          class="mt-1"
          @update:model-value="$emit('update:siteSubtitle', $event)"
        />
        <p class="mt-1 text-xs text-muted-foreground">
          显示在导航栏品牌名称下方
        </p>
      </div>
    </div>
  </CardSection>
</template>

<script setup lang="ts">
import Button from '@/components/ui/button.vue'
import Input from '@/components/ui/input.vue'
import Label from '@/components/ui/label.vue'
import { CardSection } from '@/components/layout'

defineProps<{
  siteName: string
  siteSubtitle: string
  loading: boolean
  hasChanges: boolean
}>()

defineEmits<{
  save: []
  'update:siteName': [value: string]
  'update:siteSubtitle': [value: string]
}>()
</script>
