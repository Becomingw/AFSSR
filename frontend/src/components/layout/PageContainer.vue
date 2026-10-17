<template>
  <div :class="containerClasses">
    <slot />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  maxWidth?: 'sm' | 'md' | 'lg' | 'xl' | '2xl' | 'full'
  padding?: 'none' | 'sm' | 'md' | 'lg'
}

const props = withDefaults(defineProps<Props>(), {
  maxWidth: '2xl',
  padding: 'md',
})

const containerClasses = computed(() => {
  const classes = ['console-page-container w-full min-w-0 mx-auto']

  // Max width
  const maxWidthMap = {
    sm: 'max-w-screen-sm',
    md: 'max-w-screen-md',
    lg: 'max-w-screen-lg',
    xl: 'max-w-screen-xl',
    '2xl': 'max-w-screen-2xl',
    full: 'max-w-full',
  }
  classes.push(maxWidthMap[props.maxWidth])

  // Padding
  const paddingMap = {
    none: '',
    sm: 'px-4 py-4',
    md: 'px-4 py-6 sm:px-6 lg:px-8',
    lg: 'px-6 py-8 sm:px-8 lg:px-12',
  }
  if (props.padding !== 'none') {
    classes.push(paddingMap[props.padding])
  }

  return classes.join(' ')
})
</script>
