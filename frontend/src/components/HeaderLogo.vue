<template>
  <div
    class="relative group cursor-pointer"
    :class="containerClass"
  >
    <!-- Logo SVG -->
    <svg
      xmlns="http://www.w3.org/2000/svg"
      :viewBox="viewBox"
      class="transition-colors duration-500 ease-out"
      :class="[logoClass]"
    >
      <path
        :d="brandPath"
        fill-rule="evenodd"
        fill="currentColor"
      />
    </svg>
  </div>
</template>

<script setup lang="ts">
import { ARCA_LOGO_PATH, ARCA_LOGO_VIEWBOX } from '@/constants/arcaBrand'

interface Props {
  size?: string
  className?: string
}

const props = withDefaults(defineProps<Props>(), {
  size: 'h-8 w-8',
  className: ''
})

const brandPath = ARCA_LOGO_PATH
const viewBox = ARCA_LOGO_VIEWBOX
const containerClass = `${props.size} ${props.className}`.trim()
const logoClass = 'w-full h-full'
</script>

<style scoped>
</style>
