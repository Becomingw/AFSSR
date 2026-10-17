<template>
  <button
    data-ui="button"
    :data-variant="props.variant"
    :type="props.type"
    :class="buttonClass"
    :disabled="disabled"
    v-bind="$attrs"
  >
    <slot />
  </button>
</template>

<script setup lang="ts">
import type { ClassValue } from 'clsx'
import { computed } from 'vue'
import { cn } from '@/lib/utils'

interface Props {
  variant?: 'default' | 'destructive' | 'outline' | 'secondary' | 'ghost' | 'link'
  size?: 'default' | 'sm' | 'lg' | 'icon'
  disabled?: boolean
  class?: ClassValue
  type?: 'button' | 'submit' | 'reset'
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  size: 'default',
  disabled: false,
  class: undefined,
  type: 'button'
})

const buttonClass = computed(() => {
  const baseClass =
    'inline-flex min-w-0 max-w-full items-center justify-center gap-1.5 rounded-md text-sm font-medium leading-5 [overflow-wrap:anywhere] [&_svg]:shrink-0 transition-colors duration-150 ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50'

  const variantClasses = {
    default: 'bg-primary text-primary-foreground hover:bg-primary/90',
    destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/85',
    outline:
      'border border-border bg-card text-foreground hover:border-primary/50 hover:bg-accent',
    secondary:
      'bg-secondary text-secondary-foreground hover:bg-secondary/80',
    ghost: 'hover:bg-accent hover:text-accent-foreground',
    link: 'text-primary underline-offset-4 hover:underline',
  }

  const sizeClasses = {
    default: 'h-9 px-3.5',
    sm: 'h-8 px-3 text-xs',
    lg: 'h-10 px-5',
    icon: 'h-9 w-9 shrink-0 p-0',
  }

  return cn(
    baseClass,
    variantClasses[props.variant],
    sizeClasses[props.size],
    props.class
  )
})
</script>
