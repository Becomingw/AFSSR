<template>
  <section class="min-w-0 overflow-hidden border-y border-border bg-card">
    <!-- 标题和操作栏 -->
    <div
      v-if="$slots.header || title"
      class="px-4 sm:px-6 py-3 sm:py-3.5 border-b border-border/60"
    >
      <slot name="header">
        <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 sm:gap-4">
          <!-- 左侧：标题 -->
          <h3 class="text-sm sm:text-base font-semibold shrink-0">
            {{ title }}
          </h3>

          <!-- 右侧：操作区 -->
          <div
            v-if="$slots.actions"
            class="flex flex-wrap items-center gap-2"
          >
            <slot name="actions" />
          </div>
        </div>
      </slot>
    </div>

    <!-- 表格内容 -->
    <slot />

    <!-- 分页 -->
    <slot name="pagination" />
  </section>
</template>

<script setup lang="ts">

interface Props {
  title?: string
}

defineProps<Props>()
</script>
