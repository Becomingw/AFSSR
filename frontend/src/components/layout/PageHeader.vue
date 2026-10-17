<template>
  <header class="console-page-header flex min-w-0 flex-wrap items-center justify-between gap-3">
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-3">
        <slot name="icon">
          <div
            v-if="icon"
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary/10"
          >
            <component
              :is="icon"
              class="h-5 w-5 text-primary"
            />
          </div>
        </slot>

        <div class="min-w-0">
          <h1 class="console-page-title break-words text-xl font-semibold text-foreground">
            {{ title }}
          </h1>
          <p
            v-if="description"
            class="mt-1 break-words text-sm text-muted-foreground"
          >
            {{ description }}
          </p>
        </div>
      </div>
    </div>

    <div
      v-if="$slots.actions"
      class="flex min-w-0 max-w-full flex-wrap items-center gap-2 sm:justify-end"
    >
      <slot name="actions" />
    </div>
  </header>
</template>

<script setup lang="ts">
import type { Component } from 'vue'

interface Props {
  title: string
  description?: string
  icon?: Component
}

defineProps<Props>()
</script>
