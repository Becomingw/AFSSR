<template>
  <div
    class="line-by-line-logo"
    :style="{ width: `${size}px`, height: `${size}px` }"
  >
    <svg
      :viewBox="viewBox"
      class="logo-svg"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <!-- Metallic gradient -->
        <linearGradient
          :id="gradientId"
          x1="0%"
          y1="0%"
          x2="100%"
          y2="100%"
        >
          <stop
            offset="0%"
            :stop-color="metallicColors.dark"
          />
          <stop
            offset="25%"
            :stop-color="metallicColors.base"
          />
          <stop
            offset="50%"
            :stop-color="metallicColors.light"
          />
          <stop
            offset="75%"
            :stop-color="metallicColors.base"
          />
          <stop
            offset="100%"
            :stop-color="metallicColors.dark"
          />
        </linearGradient>
      </defs>

      <!-- Layer 0: Ghost Tracks (Always visible, faint) -->
      <g class="ghost-layer">
        <path
          v-for="line in orderedLines"
          :key="`ghost-${line.order}`"
          :d="line.path"
          class="ghost-path"
          fill="none"
          :stroke="currentColors.primary"
          :stroke-width="strokeWidth"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </g>

      <!-- Layer 1: Fill (fade in/out) -->
      <path
        class="fill-path"
        :d="fullPath"
        :fill="`url(#${gradientId})`"
        fill-rule="evenodd"
        :style="fillStyle"
      />

      <!-- Layer 2: Animated lines -->
      <g class="lines-layer">
        <path
          v-for="line in orderedLines"
          :key="`line-${line.order}`"
          :ref="(el) => setPathRef(el as SVGPathElement, line.order)"
          :d="line.path"
          class="line-path"
          fill="none"
          :stroke="currentColors.primary"
          :stroke-width="strokeWidth"
          :style="getLineStyle(line.order)"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </g>
    </svg>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick, computed } from 'vue'
import {
  AETHER_SVG_VIEWBOX,
  AETHER_ORDERED_LINES,
  AETHER_FULL_PATH,
  getAetherLineDelayMultiplier
} from '@/constants/logoPaths'
import { ARCA_LOGO_PATH, ARCA_LOGO_VIEWBOX } from '@/constants/arcaBrand'

// Animation phases
type AnimationPhase = 'idle' | 'drawOutline' | 'fillFadeIn' | 'hold' | 'fillFadeOut' | 'eraseOutline'

// Color scheme type
interface ColorScheme {
  primary: string
  secondary: string
}

const props = withDefaults(
  defineProps<{
    brand?: 'aether' | 'arca'
    size?: number
    lineDelay?: number
    strokeDuration?: number
    fillDuration?: number
    autoStart?: boolean
    loop?: boolean
    loopPause?: number
    outlineColor?: string
    gradientColor?: string
    strokeWidth?: number
    cycleColors?: boolean
    isDark?: boolean
  }>(),
  {
    brand: 'aether',
    size: 400,
    lineDelay: 60,
    strokeDuration: 1200,
    fillDuration: 800,
    autoStart: true,
    loop: true,
    loopPause: 600,
    outlineColor: '#cc785c',
    gradientColor: '#e8a882',
    strokeWidth: 2.5,
    cycleColors: false,
    isDark: false
  }
)
const emit = defineEmits<{
  (e: 'animationComplete'): void
  (e: 'phaseChange', phase: AnimationPhase): void
  (e: 'colorChange', colors: ColorScheme): void
}>()
// Constants
const orderedLines = props.brand === 'arca'
  ? [{ order: 0, path: ARCA_LOGO_PATH }]
  : AETHER_ORDERED_LINES
const LINE_COUNT = orderedLines.length
const DEFAULT_PATH_LENGTH = 3000

// Light mode color schemes
const LIGHT_MODE_SCHEMES: ColorScheme[] = [
  { primary: '#9a5a42', secondary: '#c4866a' },
  { primary: '#8b4557', secondary: '#b87a8a' },
  { primary: '#996b2e', secondary: '#c49a5c' },
  { primary: '#7a5c3a', secondary: '#a8896a' },
  { primary: '#6b4d82', secondary: '#9a7eb5' },
  { primary: '#2d6a7a', secondary: '#5a9aaa' },
  { primary: '#4a6b3a', secondary: '#7a9a6a' },
  { primary: '#8a5a5a', secondary: '#b88a8a' },
  { primary: '#5a6a7a', secondary: '#8a9aaa' },
  { primary: '#6a5a4a', secondary: '#9a8a7a' },
  { primary: '#7a4a5a', secondary: '#aa7a8a' },
  { primary: '#4a5a6a', secondary: '#7a8a9a' },
]

// Dark mode color schemes
const DARK_MODE_SCHEMES: ColorScheme[] = [
  { primary: '#f59e0b', secondary: '#fcd34d' },
  { primary: '#ec4899', secondary: '#f9a8d4' },
  { primary: '#22d3ee', secondary: '#a5f3fc' },
  { primary: '#a855f7', secondary: '#d8b4fe' },
  { primary: '#4ade80', secondary: '#bbf7d0' },
  { primary: '#f472b6', secondary: '#fbcfe8' },
  { primary: '#38bdf8', secondary: '#bae6fd' },
  { primary: '#fb923c', secondary: '#fed7aa' },
  { primary: '#a78bfa', secondary: '#ddd6fe' },
  { primary: '#2dd4bf', secondary: '#99f6e4' },
  { primary: '#facc15', secondary: '#fef08a' },
  { primary: '#e879f9', secondary: '#f5d0fe' },
]

// Unique ID for gradient
const gradientId = `aether-gradient-${Math.random().toString(36).slice(2, 9)}`

const viewBox = props.brand === 'arca' ? ARCA_LOGO_VIEWBOX : AETHER_SVG_VIEWBOX
const fullPath = props.brand === 'arca' ? ARCA_LOGO_PATH : AETHER_FULL_PATH

// Path refs and lengths
const pathRefs = ref<(SVGPathElement | null)[]>(new Array(LINE_COUNT).fill(null))
const pathLengths = ref<number[]>(new Array(LINE_COUNT).fill(DEFAULT_PATH_LENGTH))

// Animation states
const lineDrawn = ref<boolean[]>(new Array(LINE_COUNT).fill(false))
const isFilled = ref(false)
const currentPhase = ref<AnimationPhase>('idle')
const isAnimating = ref(false)

// Timer cleanup
let animationAborted = false
let startTimeoutId: ReturnType<typeof setTimeout> | null = null
let hasStartedOnce = false

// Color cycling state
const colorIndex = ref(0)

// Computed
const activeSchemes = computed(() => props.isDark ? DARK_MODE_SCHEMES : LIGHT_MODE_SCHEMES)

const currentColors = computed<ColorScheme>(() => {
  if (props.cycleColors) {
    return activeSchemes.value[colorIndex.value % activeSchemes.value.length]
  }
  return { primary: props.outlineColor, secondary: props.gradientColor }
})

const metallicColors = computed(() => ({
  dark: adjustColor(currentColors.value.primary, -20),
  base: currentColors.value.primary,
  light: currentColors.value.secondary,
  highlight: adjustColor(currentColors.value.secondary, 30)
}))

// Fill style with fade transition
const fillStyle = computed(() => ({
  opacity: isFilled.value ? 0.85 : 0,
  transition: `opacity ${props.fillDuration}ms ease-in-out`
}))

// Helper functions
function adjustColor(hex: string, amount: number): string {
  const num = parseInt(hex.replace('#', ''), 16)
  const r = Math.min(255, Math.max(0, (num >> 16) + amount))
  const g = Math.min(255, Math.max(0, ((num >> 8) & 0x00FF) + amount))
  const b = Math.min(255, Math.max(0, (num & 0x0000FF) + amount))
  return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, '0')}`
}

const setPathRef = (el: SVGPathElement | null, index: number) => {
  pathRefs.value[index] = el
}

const calculatePathLengths = () => {
  pathRefs.value.forEach((path, index) => {
    if (path) {
      try {
        pathLengths.value[index] = path.getTotalLength()
      } catch {
        pathLengths.value[index] = DEFAULT_PATH_LENGTH
      }
    }
  })
}

// Line style with stroke drawing animation
const getLineStyle = (index: number) => {
  const pathLength = pathLengths.value[index]
  const isDrawn = lineDrawn.value[index]
  const phase = currentPhase.value

  // Only enable transition during actual draw/erase phases
  let transition = 'none'
  if (phase === 'drawOutline' || phase === 'eraseOutline') {
    transition = `stroke-dashoffset ${props.strokeDuration}ms cubic-bezier(0.4, 0, 0.2, 1)`
  }

  return {
    strokeDasharray: pathLength,
    strokeDashoffset: isDrawn ? 0 : pathLength,
    transition
  }
}

// Abortable wait
const wait = (ms: number) => new Promise<void>((resolve, reject) => {
  if (animationAborted) {
    reject(new Error('Animation aborted'))
    return
  }
  const timeoutId = setTimeout(() => {
    if (animationAborted) {
      reject(new Error('Animation aborted'))
    } else {
      resolve()
    }
  }, ms)

  if (animationAborted) {
    clearTimeout(timeoutId)
    reject(new Error('Animation aborted'))
  }
})

const nextColor = () => {
  colorIndex.value = (colorIndex.value + 1) % activeSchemes.value.length
  emit('colorChange', currentColors.value)
}

// Animation instance counter to prevent multiple concurrent animations
let animationInstanceId = 0

// Main animation sequence
const startAnimation = async () => {
  if (isAnimating.value) return

  const currentInstanceId = ++animationInstanceId
  isAnimating.value = true
  animationAborted = false

  try {
    // Reset states
    lineDrawn.value = new Array(LINE_COUNT).fill(false)
    isFilled.value = false
    currentPhase.value = 'idle'

    await nextTick()
    calculatePathLengths()
    await nextTick()

    // Phase 1: Draw outlines (line by line)
    currentPhase.value = 'drawOutline'
    emit('phaseChange', 'drawOutline')

    for (const line of orderedLines) {
      lineDrawn.value[line.order] = true
      if (line.order < LINE_COUNT - 1) {
        await wait(Math.round(props.lineDelay * getAetherLineDelayMultiplier(line.order, LINE_COUNT)))
      }
    }
    await wait(props.strokeDuration)

    // Phase 2: Fill fade in
    currentPhase.value = 'fillFadeIn'
    emit('phaseChange', 'fillFadeIn')
    isFilled.value = true
    await wait(props.fillDuration)

    // Hold
    currentPhase.value = 'hold'
    await wait(props.loopPause / 2)

    // Phase 3: Fill fade out
    currentPhase.value = 'fillFadeOut'
    emit('phaseChange', 'fillFadeOut')
    isFilled.value = false
    await wait(props.fillDuration)

    // Phase 4: Erase outlines (line by line)
    currentPhase.value = 'eraseOutline'
    emit('phaseChange', 'eraseOutline')

    for (const line of [...orderedLines].reverse()) {
      lineDrawn.value[line.order] = false
      if (line.order > 0) {
        await wait(Math.round(props.lineDelay * getAetherLineDelayMultiplier(line.order, LINE_COUNT)))
      }
    }
    await wait(props.strokeDuration)

    currentPhase.value = 'idle'
    isAnimating.value = false
    emit('animationComplete')

    // Check if this animation instance is still valid before looping
    if (props.loop && !animationAborted && currentInstanceId === animationInstanceId) {
      if (props.cycleColors) nextColor()
      await wait(props.loopPause / 2)
      // Double check before recursing
      if (!animationAborted && currentInstanceId === animationInstanceId) {
        startAnimation()
      }
    }
  } catch {
    isAnimating.value = false
    currentPhase.value = 'idle'
  }
}

const reset = () => {
  animationAborted = true
  lineDrawn.value = new Array(LINE_COUNT).fill(false)
  isFilled.value = false
  currentPhase.value = 'idle'
  isAnimating.value = false
}

const stop = () => {
  animationAborted = true
}

watch(() => props.isDark, () => {
  colorIndex.value = 0
})

defineExpose({ startAnimation, reset, stop, isAnimating, currentPhase, nextColor, colorIndex })

onMounted(async () => {
  await nextTick()
  calculatePathLengths()
  if (props.autoStart && !hasStartedOnce) {
    hasStartedOnce = true
    startTimeoutId = setTimeout(startAnimation, 300)
  }
})

onUnmounted(() => {
  animationAborted = true
  if (startTimeoutId) {
    clearTimeout(startTimeoutId)
    startTimeoutId = null
  }
})

watch(() => props.autoStart, (newVal) => {
  if (newVal && !isAnimating.value && !hasStartedOnce) {
    hasStartedOnce = true
    startAnimation()
  }
})
</script>

<style scoped>
.line-by-line-logo {
  display: flex;
  align-items: center;
  justify-content: center;
}

.logo-svg {
  width: 100%;
  height: 100%;
  overflow: visible;
  transform: translateZ(0);
  backface-visibility: hidden;
}

.line-path {
  will-change: stroke-dashoffset;
}

.fill-path {
  will-change: opacity;
  pointer-events: none;
}

.ghost-path {
  opacity: 0.06;
}
</style>
