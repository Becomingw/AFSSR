import { computed, ref } from 'vue'
import { useDocumentVisibility, usePreferredReducedMotion } from '@vueuse/core'

export function useFeatureMotion(active: () => boolean) {
  const paused = ref(false)
  const visibility = useDocumentVisibility()
  const preference = usePreferredReducedMotion()
  const reducedMotion = computed(() => preference.value === 'reduce')
  const running = computed(() => active() && !paused.value && !reducedMotion.value && visibility.value === 'visible')
  function toggle() { paused.value = !paused.value }
  return { paused, running, reducedMotion, toggle }
}
