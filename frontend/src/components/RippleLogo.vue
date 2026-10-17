<template>
  <Transition name="logo-fade">
    <!-- Adaptive Aether logo using external SVG with CSS-based dark mode -->
    <!-- Animation sequence: stroke outline -> fill color -> ripple breathing -->
    <div
      v-if="type === 'aether' && useAdaptive"
      :key="`aether-adaptive-${animationKey}`"
      class="aether-adaptive-container"
      :style="{ '--anim-delay': `${animDelay}ms` }"
    >
      <!-- Definitions for gradient and glow -->
      <svg
        style="position: absolute; width: 0; height: 0; overflow: hidden;"
        aria-hidden="true"
      >
        <defs>
          <linearGradient
            id="adaptive-aether-gradient"
            x1="0%"
            y1="0%"
            x2="100%"
            y2="100%"
          >
            <stop
              offset="0%"
              stop-color="#cc785c"
            />
            <stop
              offset="50%"
              stop-color="#d4a27f"
            />
            <stop
              offset="100%"
              stop-color="#cc785c"
            />
          </linearGradient>
          <filter
            id="adaptive-aether-glow"
            x="-50%"
            y="-50%"
            width="200%"
            height="200%"
          >
            <feGaussianBlur
              stdDeviation="3"
              result="coloredBlur"
            />
            <feMerge>
              <feMergeNode in="coloredBlur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
      </svg>

      <!-- Ripple layers - start after fill completes -->
      <div
        class="adaptive-ripple r-1"
        :class="{ active: adaptiveFillComplete }"
      >
        <svg
          :viewBox="adaptiveAetherViewBox"
          class="adaptive-logo-img"
        >
          <path
            :d="adaptiveAetherPath"
            fill="none"
            stroke="url(#adaptive-aether-gradient)"
            stroke-width="2"
            vector-effect="non-scaling-stroke"
          />
        </svg>
      </div>
      <div
        class="adaptive-ripple r-2"
        :class="{ active: adaptiveFillComplete }"
      >
        <svg
          :viewBox="adaptiveAetherViewBox"
          class="adaptive-logo-img"
        >
          <path
            :d="adaptiveAetherPath"
            fill="none"
            stroke="url(#adaptive-aether-gradient)"
            stroke-width="2"
            vector-effect="non-scaling-stroke"
          />
        </svg>
      </div>
      <div
        class="adaptive-ripple r-3"
        :class="{ active: adaptiveFillComplete }"
      >
        <svg
          :viewBox="adaptiveAetherViewBox"
          class="adaptive-logo-img"
        >
          <path
            :d="adaptiveAetherPath"
            fill="none"
            stroke="url(#adaptive-aether-gradient)"
            stroke-width="2"
            vector-effect="non-scaling-stroke"
          />
        </svg>
      </div>

      <!-- Phase 1: Stroke outline drawing (SVG overlay) -->
      <svg
        class="adaptive-stroke-overlay"
        :class="{ 'stroke-complete': adaptiveStrokeComplete }"
        :viewBox="adaptiveAetherViewBox"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path
          class="adaptive-stroke-path"
          :d="adaptiveAetherPath"
          style="stroke: url(#adaptive-aether-gradient); filter: url(#adaptive-aether-glow);"
        />
      </svg>

      <!-- Phase 2: Fill using SVG path -->
      <div
        class="adaptive-fill-layer"
        :class="{ 'fill-active': adaptiveStrokeComplete, 'fill-complete': adaptiveFillComplete, 'breathing': adaptiveFillComplete }"
      >
        <svg
          :viewBox="adaptiveAetherViewBox"
          class="adaptive-fill-img"
        >
          <path
            :d="adaptiveAetherPath"
            fill="url(#adaptive-aether-gradient)"
            fill-rule="evenodd"
          />
        </svg>
      </div>
    </div>

    <!-- Aether logo: single complex path with ripple effect -->
    <svg
      v-else-if="type === 'aether'"
      :key="`aether-${animationKey}`"
      :viewBox="viewBox"
      class="ripple-logo"
      xmlns="http://www.w3.org/2000/svg"
      :style="{ '--anim-delay': `${animDelay}ms` }"
    >
      <defs>
        <path
          id="aether-path"
          ref="aetherPathRef"
          :d="aetherPath"
        />
        <!-- Gradient for breathing glow effect -->
        <linearGradient
          id="aether-gradient"
          x1="0%"
          y1="0%"
          x2="100%"
          y2="100%"
        >
          <stop
            offset="0%"
            stop-color="#cc785c"
          />
          <stop
            offset="50%"
            stop-color="#d4a27f"
          />
          <stop
            offset="100%"
            stop-color="#cc785c"
          />
        </linearGradient>
        <!-- Glow filter for breathing effect -->
        <filter
          id="aether-glow"
          x="-50%"
          y="-50%"
          width="200%"
          height="200%"
        >
          <feGaussianBlur
            stdDeviation="4"
            result="coloredBlur"
          />
          <feMerge>
            <feMergeNode in="coloredBlur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      <!-- Static mode: just show filled logo with fade-in animation -->
      <template v-if="static">
        <use
          href="#aether-path"
          class="static-fill"
          :style="{ fill: strokeColor }"
        />
      </template>

      <!-- Animated mode -->
      <template v-else>
        <!-- Main logo - with stroke drawing animation -->
        <use
          href="#aether-path"
          class="fine-line stroke-draw aether-stroke"
          :class="{ 'draw-complete': drawComplete, 'breathing': drawComplete && !disableRipple }"
          :style="{ stroke: drawComplete ? 'url(#aether-gradient)' : strokeColor, '--path-length': aetherPathLength, transformOrigin: aetherCenter }"
          :filter="drawComplete ? 'url(#aether-glow)' : 'none'"
        />
        <!-- Main logo - fill (fade in after draw) -->
        <use
          href="#aether-path"
          class="aether-fill"
          :class="{ 'fill-active': drawComplete, 'breathing': drawComplete && !disableRipple }"
          :style="{ fill: strokeColor, transformOrigin: aetherCenter }"
        />
        <use
          v-if="!disableRipple"
          href="#aether-path"
          class="fine-line ripple d-1"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: strokeColor, transformOrigin: aetherCenter }"
        />
        <use
          v-if="!disableRipple"
          href="#aether-path"
          class="fine-line ripple d-2"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: strokeColor, transformOrigin: aetherCenter }"
        />
        <use
          v-if="!disableRipple"
          href="#aether-path"
          class="fine-line ripple d-3"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: strokeColor, transformOrigin: aetherCenter }"
        />
      </template>
    </svg>
  
    <!-- Standard single-path logos -->
    <svg
      v-else
      :key="`${type}-${animationKey}`"
      :viewBox="viewBox"
      class="ripple-logo"
      xmlns="http://www.w3.org/2000/svg"
      :style="{ '--anim-delay': `${animDelay}ms` }"
    >
      <defs>
        <path
          :id="pathId"
          :d="pathData"
        />
        <!-- Gemini multi-layer gradients -->
        <template v-if="type === 'gemini'">
          <!-- Fill gradients -->
          <linearGradient
            :id="`${pathId}-fill-0`"
            gradientUnits="userSpaceOnUse"
            x1="7"
            x2="11"
            y1="15.5"
            y2="12"
          >
            <stop stop-color="#08B962" />
            <stop
              offset="1"
              stop-color="#08B962"
              stop-opacity="0"
            />
          </linearGradient>
          <linearGradient
            :id="`${pathId}-fill-1`"
            gradientUnits="userSpaceOnUse"
            x1="8"
            x2="11.5"
            y1="5.5"
            y2="11"
          >
            <stop stop-color="#F94543" />
            <stop
              offset="1"
              stop-color="#F94543"
              stop-opacity="0"
            />
          </linearGradient>
          <linearGradient
            :id="`${pathId}-fill-2`"
            gradientUnits="userSpaceOnUse"
            x1="3.5"
            x2="17.5"
            y1="13.5"
            y2="12"
          >
            <stop stop-color="#FABC12" />
            <stop
              offset=".46"
              stop-color="#FABC12"
              stop-opacity="0"
            />
          </linearGradient>
          <!-- Stroke gradient for outline - 4 directional gradients to match logo colors -->
          <!-- Top point = red, Right point = blue, Bottom point = green, Left point = yellow -->
          <linearGradient
            :id="`${pathId}-stroke-v`"
            gradientUnits="userSpaceOnUse"
            x1="12"
            x2="12"
            y1="1"
            y2="23"
          >
            <stop
              offset="0%"
              stop-color="#F94543"
            />
            <stop
              offset="50%"
              stop-color="#3186FF"
            />
            <stop
              offset="100%"
              stop-color="#08B962"
            />
          </linearGradient>
          <linearGradient
            :id="`${pathId}-stroke-h`"
            gradientUnits="userSpaceOnUse"
            x1="1"
            x2="23"
            y1="12"
            y2="12"
          >
            <stop
              offset="0%"
              stop-color="#FABC12"
            />
            <stop
              offset="50%"
              stop-color="#3186FF"
            />
            <stop
              offset="100%"
              stop-color="#3186FF"
            />
          </linearGradient>
          <!-- Mask for fill-inward animation (controlled by JS) -->
          <mask :id="`${pathId}-fill-mask`">
            <rect
              x="-4"
              y="-4"
              width="32"
              height="32"
              fill="white"
            />
            <circle
              cx="12"
              cy="12"
              :r="geminiFillRadius"
              fill="black"
            />
          </mask>
        </template>
      </defs>

      <!-- OpenAI special rendering: stroke outline -> fill -> rotate + breathe -->
      <template v-if="type === 'openai'">
        <!-- Outer breathing wrapper (scale pulse) -->
        <g
          class="openai-breathe-group"
          :class="{ 'breathing': drawComplete }"
          :style="{ transformOrigin: transformOrigin }"
        >
          <!-- Inner rotation wrapper -->
          <g
            class="openai-rotate-group"
            :class="{ 'rotating': drawComplete }"
            :style="{ transformOrigin: transformOrigin }"
          >
            <!-- Step 1: Stroke outline drawing -->
            <use
              :href="`#${pathId}`"
              class="openai-outline"
              :class="{ 'outline-complete': drawComplete }"
              stroke="currentColor"
            />
            <!-- Step 2: Fill layer (appears after outline) -->
            <use
              :href="`#${pathId}`"
              class="openai-fill"
              :class="{ 'fill-active': drawComplete }"
              fill="currentColor"
              fill-rule="evenodd"
            />
          </g>
        </g>
      </template>

      <!-- Claude special rendering: stroke outline -> fill -> ripple -->
      <template v-else-if="type === 'claude'">
        <!-- Step 1: Stroke outline drawing -->
        <use
          :href="`#${pathId}`"
          class="claude-outline"
          :class="{ 'outline-complete': drawComplete }"
          stroke="#D97757"
        />
        <!-- Step 2: Fill layer (appears after outline) -->
        <use
          :href="`#${pathId}`"
          class="claude-fill"
          :class="{ 'fill-active': drawComplete }"
          fill="#D97757"
        />
        <!-- Step 3: Ripple waves (after fill complete) -->
        <use
          :href="`#${pathId}`"
          class="fine-line claude-ripple d-1"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: '#D97757', transformOrigin: transformOrigin }"
        />
        <use
          :href="`#${pathId}`"
          class="fine-line claude-ripple d-2"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: '#D97757', transformOrigin: transformOrigin }"
        />
        <use
          :href="`#${pathId}`"
          class="fine-line claude-ripple d-3"
          :class="{ 'ripple-active': drawComplete }"
          :style="{ stroke: '#D97757', transformOrigin: transformOrigin }"
        />
      </template>

      <!-- Gemini special rendering: stroke outline -> fill -> breathe -->
      <template v-else-if="type === 'gemini'">
        <!-- Step 1: Stroke outline drawing (multi-layer colorful) -->
        <g
          class="gemini-outline-group"
          :class="{ 'outline-complete': drawComplete }"
        >
          <use
            :href="`#${pathId}`"
            class="gemini-outline"
            stroke="#3186FF"
          />
          <use
            :href="`#${pathId}`"
            class="gemini-outline"
            :style="{ stroke: `url(#${pathId}-fill-0)` }"
          />
          <use
            :href="`#${pathId}`"
            class="gemini-outline"
            :style="{ stroke: `url(#${pathId}-fill-1)` }"
          />
          <use
            :href="`#${pathId}`"
            class="gemini-outline"
            :style="{ stroke: `url(#${pathId}-fill-2)` }"
          />
        </g>
        <!-- Step 2: Fill layer (appears after outline, with inward fill animation) -->
        <g
          class="gemini-fill"
          :class="{ 'fill-complete': fillComplete }"
          :mask="`url(#${pathId}-fill-mask)`"
        >
          <use
            :href="`#${pathId}`"
            fill="#3186FF"
          />
          <use
            :href="`#${pathId}`"
            :fill="`url(#${pathId}-fill-0)`"
          />
          <use
            :href="`#${pathId}`"
            :fill="`url(#${pathId}-fill-1)`"
          />
          <use
            :href="`#${pathId}`"
            :fill="`url(#${pathId}-fill-2)`"
          />
        </g>
        <!-- Step 3: Ripple waves (after fill complete) -->
        <g v-if="!disableRipple">
          <g
            class="gemini-ripple d-1"
            :class="{ 'ripple-active': fillComplete }"
            :style="{ transformOrigin: transformOrigin }"
          >
            <use
              :href="`#${pathId}`"
              fill="#3186FF"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-0)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-1)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-2)`"
            />
          </g>
          <g
            class="gemini-ripple d-2"
            :class="{ 'ripple-active': fillComplete }"
            :style="{ transformOrigin: transformOrigin }"
          >
            <use
              :href="`#${pathId}`"
              fill="#3186FF"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-0)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-1)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-2)`"
            />
          </g>
          <g
            class="gemini-ripple d-3"
            :class="{ 'ripple-active': fillComplete }"
            :style="{ transformOrigin: transformOrigin }"
          >
            <use
              :href="`#${pathId}`"
              fill="#3186FF"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-0)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-1)`"
            />
            <use
              :href="`#${pathId}`"
              :fill="`url(#${pathId}-fill-2)`"
            />
          </g>
        </g>
      </template>

      <!-- Other logos: stroke-based rendering -->
      <template v-else>
        <!-- Static center icon with stroke drawing animation -->
        <use
          :href="`#${pathId}`"
          class="fine-line stroke-draw"
          :class="{ 'draw-complete': drawComplete }"
          :style="{ stroke: strokeColor, '--path-length': pathLength }"
        />

        <!-- Ripple waves - only active after drawing completes -->
        <g>
          <use
            :href="`#${pathId}`"
            class="fine-line ripple d-1"
            :class="{ 'ripple-active': drawComplete }"
            :style="{ stroke: strokeColor, transformOrigin: transformOrigin }"
          />
          <use
            :href="`#${pathId}`"
            class="fine-line ripple d-2"
            :class="{ 'ripple-active': drawComplete }"
            :style="{ stroke: strokeColor, transformOrigin: transformOrigin }"
          />
          <use
            :href="`#${pathId}`"
            class="fine-line ripple d-3"
            :class="{ 'ripple-active': drawComplete }"
            :style="{ stroke: strokeColor, transformOrigin: transformOrigin }"
          />
        </g>
      </template>
    </svg>
  </Transition>
</template>


<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted, nextTick } from 'vue'
import {
  AETHER_FULL_PATH,
  AETHER_LOGO_CENTER,
  AETHER_LOGO_PATH,
  AETHER_LOGO_VIEWBOX,
  AETHER_SVG_VIEWBOX
} from '@/constants/logoPaths'
import { ARCA_LOGO_CENTER, ARCA_LOGO_PATH, ARCA_LOGO_VIEWBOX } from '@/constants/arcaBrand'

type LogoType = 'aether' | 'claude' | 'openai' | 'gemini'

const props = withDefaults(
  defineProps<{
    type: LogoType
    brand?: 'aether' | 'arca'
    size?: number
    /** Use adaptive SVG image instead of inline SVG (for aether type) */
    useAdaptive?: boolean
    /** Disable ripple animation */
    disableRipple?: boolean
    /** Delay before animation starts (ms) */
    animDelay?: number
    /** Static mode - no animations at all, just show the filled logo */
    static?: boolean
  }>(),
  {
    brand: 'aether',
    size: 200,
    useAdaptive: false,
    disableRipple: false,
    animDelay: 0,
    static: false
  }
)

// Drawing animation state
const drawComplete = ref(false)
const fillComplete = ref(false)
const geminiFillRadius = ref(15) // SVG mask circle radius for fill animation
const animationKey = ref(0) // Force re-render to restart CSS animations
const drawDuration = 1000 // ms for stroke drawing animation
const openaiOutlineDuration = 1000 // ms - matches CSS animation duration (1.2s)
const geminiOutlineDuration = 900 // ms - matches CSS animation duration (1.8s)

// Timer refs for cleanup on type switch
const animationTimers = ref<number[]>([])
const fillAnimationId = ref<number | null>(null)

// Adaptive aether animation states (3-phase: stroke -> fill -> ripple)
const adaptiveStrokeComplete = ref(false)
const adaptiveFillComplete = ref(false)
const adaptiveStrokeDuration = 1500 // ms for stroke drawing
const adaptiveFillDuration = 600 // ms for fill fade-in (shorter for smoother transition)
const aetherPath = props.brand === 'arca' ? ARCA_LOGO_PATH : AETHER_LOGO_PATH
const logoCenter = props.brand === 'arca' ? ARCA_LOGO_CENTER : AETHER_LOGO_CENTER
const aetherCenter = `${logoCenter.x}px ${logoCenter.y}px`
const adaptiveAetherPath = props.brand === 'arca' ? ARCA_LOGO_PATH : AETHER_FULL_PATH
const adaptiveAetherViewBox = props.brand === 'arca' ? ARCA_LOGO_VIEWBOX : AETHER_SVG_VIEWBOX
const aetherPathRef = ref<SVGPathElement | null>(null)
const aetherPathLength = ref(12000)

const updateAetherPathLength = () => {
  if (props.type !== 'aether') return
  if (!aetherPathRef.value) return

  try {
    aetherPathLength.value = aetherPathRef.value.getTotalLength()
  } catch {
    // keep the fallback length to avoid animation jitter
  }
}

// Animate fill from edges to center (Gemini)
const geminiFillDuration = 600 // ms - shorter fill animation
const animateFill = () => {
  const startRadius = 15
  const endRadius = 0
  const startTime = performance.now()

  const animate = (currentTime: number) => {
    const elapsed = currentTime - startTime
    const progress = Math.min(elapsed / geminiFillDuration, 1)
    // Ease-out curve: fast start, slow end - fill appears quickly
    const easedProgress = 1 - Math.pow(1 - progress, 2)
    geminiFillRadius.value = startRadius - (startRadius - endRadius) * easedProgress

    if (progress < 1) {
      fillAnimationId.value = requestAnimationFrame(animate)
    } else {
      fillAnimationId.value = null
      fillComplete.value = true
      // Outline fades out after fill completes
      drawComplete.value = true
    }
  }

  fillAnimationId.value = requestAnimationFrame(animate)
}

// Clear all pending animation timers
const clearAnimationTimers = () => {
  animationTimers.value.forEach((timerId) => {
    clearTimeout(timerId)
  })
  animationTimers.value = []
  
  if (fillAnimationId.value !== null) {
    cancelAnimationFrame(fillAnimationId.value)
    fillAnimationId.value = null
  }
}

// Helper to add a tracked timer
const addTimer = (callback: () => void, delay: number) => {
  const timerId = window.setTimeout(callback, delay)
  animationTimers.value.push(timerId)
  return timerId
}

// Start animation sequence
const startAnimation = () => {
  // Static mode: skip all animations, show filled logo immediately
  if (props.static) {
    drawComplete.value = true
    fillComplete.value = true
    adaptiveStrokeComplete.value = true
    adaptiveFillComplete.value = true
    return
  }

  // Global delay before starting any animation logic
  addTimer(() => {
    if (props.type === 'aether' && props.useAdaptive) {
      // For adaptive Aether: 3-phase animation
      // Phase 1: Stroke drawing (CSS animation handles this)
      addTimer(() => {
        // Phase 2: Fill fades in after stroke completes
        adaptiveStrokeComplete.value = true

        // Phase 3: Ripples start slightly before fill completes for smoother transition
        // Start ripple at 70% of fill duration to overlap and avoid the "pause" feeling
        addTimer(() => {
          adaptiveFillComplete.value = true
        }, adaptiveFillDuration * 0.7)
      }, adaptiveStrokeDuration)
    } else if (props.type === 'gemini') {
      // For Gemini: start fill right when outline completes (ease-in curve ends fast)
      addTimer(() => {
        animateFill()
      }, geminiOutlineDuration)
    } else if (props.type === 'openai') {
      // For OpenAI: start fill right when outline completes
      addTimer(() => {
        drawComplete.value = true
      }, openaiOutlineDuration)
    } else if (props.type === 'claude') {
      // For Claude: start fill right when outline completes
      addTimer(() => {
        drawComplete.value = true
      }, drawDuration)
    } else {
      // For other logos: set drawComplete after stroke animation
      addTimer(() => {
        drawComplete.value = true
      }, drawDuration)
    }
  }, props.animDelay)
}

// Reset and restart animation when type changes
watch(
  () => props.type,
  async (_newType, oldType) => {
    // Clear any pending timers from previous logo type
    clearAnimationTimers()

    // Reset all animation states immediately
    drawComplete.value = false
    fillComplete.value = false
    geminiFillRadius.value = 15
    adaptiveStrokeComplete.value = false
    adaptiveFillComplete.value = false

    // Only increment key if type actually changed (not on initial render)
    // Increment synchronously to avoid frame delay
    if (oldType !== undefined) {
      animationKey.value++
    }

    // Wait for DOM update
    await nextTick()
    // Small delay to allow CSS transition to start
    await new Promise(resolve => setTimeout(resolve, 30))
    updateAetherPathLength()
    startAnimation()
  }
)

// Start animation on mount
onMounted(() => {
  updateAetherPathLength()
  startAnimation()
})

// Clean up timers on unmount
onUnmounted(() => {
  clearAnimationTimers()
})

// Expose animation states for parent component
defineExpose({
  fillComplete,
  drawComplete
})

const pathId = computed(() => `${props.type}-ripple-path`)

// Different viewBox for each logo to center them properly
// Aether logo viewBox: original is "419 249 954 933", center around (896, 715)
// Smaller viewBox = larger icon appearance
const viewBox = computed(() => {
  const viewBoxes: Record<LogoType, string> = {
    aether: props.brand === 'arca' ? ARCA_LOGO_VIEWBOX : AETHER_LOGO_VIEWBOX,
    claude: '0 0 24 24', // Original Claude viewBox
    openai: '0 0 24 24', // Original OpenAI viewBox
    gemini: '-4 -4 32 32'
  }
  return viewBoxes[props.type]
})

// Path lengths for different logos (approximate values for stroke animation)
const pathLength = computed(() => {
  if (props.type === 'aether') {
    return aetherPathLength.value
  }

  const lengths: Record<Exclude<LogoType, 'aether'>, number> = {
    claude: 300,
    openai: 200,
    gemini: 150
  }
  return lengths[props.type]
})

const pathData = computed(() => {
  if (props.type === 'aether') {
    return aetherPath
  }

  const paths: Record<Exclude<LogoType, 'aether'>, string> = {
    claude:
      'M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z',
    openai:
      'M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z',
    gemini:
      'M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z'
  }
  return paths[props.type]
})

const strokeColor = computed(() => {
  if (props.type === 'aether' || props.type === 'openai') {
    return 'currentColor'
  }

  if (props.type === 'claude') {
    return '#D97757'
  }

  if (props.type === 'gemini') {
    return `url(#${pathId.value}-gradient)`
  }

  return 'currentColor'
})

// Each logo has different center point based on their path coordinates
const transformOrigin = computed(() => {
  if (props.type === 'aether') {
    return aetherCenter
  }

  // Claude logo center - the sunburst visual center is around (11, 10)
  if (props.type === 'claude') {
    return '12.6px 12.7px'
  }

  return '12px 12px'
})
</script>

<style scoped>
.ripple-logo-container {
  display: flex;
  align-items: center;
  justify-content: center;
}

.ripple-logo {
  width: 100%;
  height: 100%;
  overflow: visible;
}

.fine-line {
  fill: none;
  stroke-width: 0.6px;
  vector-effect: non-scaling-stroke;
}

/* Stroke drawing animation - handwriting effect */
@keyframes stroke-draw {
  0% {
    stroke-dashoffset: var(--path-length);
    opacity: 0.3;
  }
  10% {
    opacity: 1;
  }
  100% {
    stroke-dashoffset: 0;
    opacity: 1;
  }
}

.stroke-draw {
  stroke-dasharray: var(--path-length);
  stroke-dashoffset: var(--path-length);
  animation: stroke-draw 1.2s cubic-bezier(0.4, 0, 0.2, 1) forwards;
  animation-delay: var(--anim-delay, 0s);
}

.stroke-draw.delay-1 {
  animation-delay: 0.3s;
}

.stroke-draw.draw-complete {
  stroke-dasharray: none;
  stroke-dashoffset: 0;
  animation: none;
}

/* Ripple breathing animation - multiple directions for variety */
@keyframes ripple-expand {
  0% {
    transform: scale(1);
    opacity: 0.5;
  }
  100% {
    transform: scale(2.5);
    opacity: 0;
  }
}

@keyframes ripple-expand-up {
  0% {
    transform: scale(1) translateY(0);
    opacity: 0.5;
  }
  100% {
    transform: scale(2) translateY(-30%);
    opacity: 0;
  }
}

@keyframes ripple-expand-diagonal {
  0% {
    transform: scale(1) translate(0, 0);
    opacity: 0.5;
  }
  100% {
    transform: scale(2.2) translate(15%, -15%);
    opacity: 0;
  }
}

@keyframes ripple-pulse {
  0% {
    transform: scale(1);
    opacity: 0.4;
  }
  50% {
    transform: scale(1.8);
    opacity: 0.2;
  }
  100% {
    transform: scale(2.5);
    opacity: 0;
  }
}

/* Ripple waves - hidden by default, only show after drawing completes */
.ripple {
  opacity: 0;
  pointer-events: none;
  will-change: transform, opacity;
  transform: translateZ(0); /* GPU acceleration */
}

.ripple.ripple-active {
  animation: ripple-expand 4s cubic-bezier(0, 0, 0.2, 1) infinite;
}

.ripple.ripple-active.d-1 {
  animation-name: ripple-expand;
  animation-delay: 0s;
}
.ripple.ripple-active.d-2 {
  animation-name: ripple-expand-up;
  animation-delay: 1.3s;
}
.ripple.ripple-active.d-3 {
  animation-name: ripple-expand-diagonal;
  animation-delay: 2.6s;
}

/* OpenAI specific styles - 3 phase animation with smooth transitions */
/* Phase 1: Stroke outline drawing -> Phase 2: Fill fade in -> Phase 3: Rotate + Breathe */

/* Phase 1: Stroke outline drawing - clear and visible */
.openai-outline {
  fill: none;
  stroke-width: 0.5px;
  vector-effect: non-scaling-stroke;
  stroke-dasharray: 200;
  stroke-dashoffset: 200;
  animation: openai-outline-draw 0.9s ease-in forwards;
  animation-delay: var(--anim-delay, 0s);
}

@keyframes openai-outline-draw {
  0% {
    stroke-dashoffset: 200;
    opacity: 0.3;
  }
  10% {
    opacity: 1;
  }
  100% {
    stroke-dashoffset: 0;
    opacity: 1;
  }
}

/* Outline fades out immediately when fill starts */
.openai-outline.outline-complete {
  opacity: 0;
  transition: opacity 0.3s ease-out;
}

/* Phase 2: Fill appears immediately */
.openai-fill {
  opacity: 0;
  visibility: hidden;
}

.openai-fill.fill-active {
  visibility: visible;
  animation: openai-fill-reveal 0.4s ease-out forwards;
}

@keyframes openai-fill-reveal {
  0% {
    opacity: 0;
  }
  100% {
    opacity: 1;
  }
}

/* Phase 3: Rotation + Breathing effects */

@keyframes openai-rotate {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.openai-rotate-group.rotating {
  animation: openai-rotate 25s linear infinite;
}

@keyframes openai-breathe {
  0%, 100% {
    transform: scale(1);
    filter: brightness(1);
  }
  50% {
    transform: scale(1.03);
    filter: brightness(1.05);
  }
}

.openai-breathe-group.breathing {
  animation: openai-breathe 3.5s ease-in-out infinite;
}

/* Claude specific styles - 2 phase animation: outline -> fill -> breathe */

/* Phase 1: Stroke outline drawing */
.claude-outline {
  fill: none;
  stroke-width: 0.15px;
  vector-effect: non-scaling-stroke;
  stroke-dasharray: 300;
  stroke-dashoffset: 300;
  animation: claude-outline-draw 1.2s ease-in forwards;
}

@keyframes claude-outline-draw {
  0% {
    stroke-dashoffset: 300;
    opacity: 0.3;
  }
  10% {
    opacity: 1;
  }
  100% {
    stroke-dashoffset: 0;
    opacity: 1;
  }
}

/* Outline fades out after fill appears */
.claude-outline.outline-complete {
  opacity: 0;
  transition: opacity 0.3s ease-out;
}

/* Phase 2: Fill appears after outline */
.claude-fill {
  opacity: 0;
  visibility: hidden;
}

.claude-fill.fill-active {
  visibility: visible;
  opacity: 1;
  transition: opacity 0.5s ease-in;
}

/* Phase 3: Ripple waves */
.claude-ripple {
  opacity: 0;
  pointer-events: none;
  stroke-width: 0.3px;
}

.claude-ripple.ripple-active {
  animation: claude-ripple-expand 4s cubic-bezier(0, 0, 0.2, 1) infinite;
}

/* Claude ripple - expand from center only */
@keyframes claude-ripple-expand {
  0% {
    transform: scale(1);
    opacity: 0.5;
  }
  100% {
    transform: scale(2.5);
    opacity: 0;
  }
}

.claude-ripple.ripple-active.d-1 {
  animation-delay: 0s;
}
.claude-ripple.ripple-active.d-2 {
  animation-delay: 1.3s;
}
.claude-ripple.ripple-active.d-3 {
  animation-delay: 2.6s;
}

/* Gemini specific styles - 3 phase animation: outline -> fill -> breathe */

/* Phase 1: Stroke outline drawing (multi-layer colorful) */
.gemini-outline-group {
  opacity: 1;
}

/* Outline stays visible, fades out only after fill completes */
.gemini-outline-group.outline-complete {
  opacity: 0;
  transition: opacity 0.3s ease-out;
}

.gemini-outline {
  fill: none;
  stroke-width: 1px;
  vector-effect: non-scaling-stroke;
  stroke-dasharray: 100;
  stroke-dashoffset: 100;
  animation: gemini-outline-draw 1.8s cubic-bezier(0.4, 0, 0.2, 1) forwards;
}

@keyframes gemini-outline-draw {
  0% {
    stroke-dashoffset: 100;
    opacity: 0;
  }
  5% {
    opacity: 0.5;
  }
  15% {
    opacity: 1;
  }
  100% {
    stroke-dashoffset: 0;
    opacity: 1;
  }
}

/* Phase 2: Fill from edges inward to center (uses SVG mask animation) */
.gemini-fill {
  opacity: 1;
}

.gemini-fill.fill-complete {
  mask: none;
}

/* Phase 3: Breathing ripples */
.gemini-ripple {
  opacity: 0;
  pointer-events: none;
}

.gemini-ripple.ripple-active {
  animation: ripple-expand 4s cubic-bezier(0, 0, 0.2, 1) infinite;
}

.gemini-ripple.ripple-active.d-1 {
  animation-name: ripple-expand;
  animation-delay: 0s;
}
.gemini-ripple.ripple-active.d-2 {
  animation-name: ripple-expand-up;
  animation-delay: 1.3s;
}
.gemini-ripple.ripple-active.d-3 {
  animation-name: ripple-expand-diagonal;
  animation-delay: 2.6s;
}

/* Aether Fill Animation */
.aether-fill {
  opacity: 0;
  transition: opacity 1.5s ease;
  pointer-events: none;
}

.aether-fill.fill-active {
  opacity: 0.6; /* Solid fill to show logo shape clearly */
}

/* Aether Breathing Animation */
@keyframes aether-breathe {
  0%, 100% {
    stroke-width: 0.6px;
    opacity: 0.85;
    transform: scale(1);
  }
  50% {
    stroke-width: 1.5px;
    opacity: 1;
    transform: scale(1.05);
  }
}

@keyframes aether-glow-pulse {
  0%, 100% {
    filter: url(#aether-glow) brightness(1) drop-shadow(0 0 2px rgba(204, 120, 92, 0.3));
  }
  50% {
    filter: url(#aether-glow) brightness(1.3) drop-shadow(0 0 8px rgba(204, 120, 92, 0.6));
  }
}

.aether-stroke.breathing {
  animation: aether-breathe 4s ease-in-out infinite;
  transition: stroke 0.5s ease;
}

.aether-fill.breathing {
  animation: aether-fill-breathe 4s ease-in-out infinite;
}

@keyframes aether-fill-breathe {
  0%, 100% {
    opacity: 0.6;
    transform: scale(1);
  }
  50% {
    opacity: 0.75;
    transform: scale(1.05);
  }
}

/* Logo transition styles - simple fade only, no transform to avoid animation interference */
.logo-fade-enter-active,
.logo-fade-leave-active {
  transition: opacity 0.3s ease;
}

.logo-fade-enter-from,
.logo-fade-leave-to {
  opacity: 0;
}

/* Adaptive Aether Logo Styles - 3 Phase Animation */
/* Phase 1: Stroke drawing -> Phase 2: Fill reveal -> Phase 3: Ripple breathing */
.aether-adaptive-container {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* Stroke drawing animation keyframes */
@keyframes adaptive-stroke-draw {
  0% {
    stroke-dashoffset: -12000;
    opacity: 0.3;
  }
  10% {
    opacity: 1;
  }
  100% {
    stroke-dashoffset: 0;
    opacity: 1;
  }
}

/* Ripple layers - hidden until fill completes (Phase 3) */
.adaptive-ripple {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  pointer-events: none;
  visibility: hidden;
}

.adaptive-ripple.active {
  visibility: visible;
  animation: adaptive-ripple-expand 4s ease-out infinite;
}

.adaptive-logo-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

@keyframes adaptive-ripple-expand {
  0% {
    transform: scale(1);
    opacity: 0.35;
  }
  100% {
    transform: scale(2);
    opacity: 0;
  }
}

@keyframes adaptive-ripple-expand-up {
  0% {
    transform: scale(1) translateY(0);
    opacity: 0.35;
  }
  100% {
    transform: scale(1.8) translateY(-25%);
    opacity: 0;
  }
}

@keyframes adaptive-ripple-expand-diagonal {
  0% {
    transform: scale(1) translate(0, 0);
    opacity: 0.35;
  }
  100% {
    transform: scale(2) translate(12%, -12%);
    opacity: 0;
  }
}

/* Stagger the ripples with different directions */
.adaptive-ripple.active.r-1 {
  animation-name: adaptive-ripple-expand;
  animation-delay: 0s;
}
.adaptive-ripple.active.r-2 {
  animation-name: adaptive-ripple-expand-up;
  animation-delay: 1.33s;
}
.adaptive-ripple.active.r-3 {
  animation-name: adaptive-ripple-expand-diagonal;
  animation-delay: 2.66s;
}

/* Phase 1: Stroke overlay - positioned above fill layer */
.adaptive-stroke-overlay {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 3;
  pointer-events: none;
  overflow: visible;
}

/* Fade out stroke overlay when fill starts */
.adaptive-stroke-overlay.stroke-complete {
  opacity: 0;
  transition: opacity 0.5s ease;
}

/* Stroke path animation */
.adaptive-stroke-path {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-dasharray: 12000;
  stroke-dashoffset: -12000;
  animation: adaptive-stroke-draw 1.5s cubic-bezier(0.4, 0, 0.2, 1) forwards;
}

/* Phase 2: Fill layer using original SVG image */
.adaptive-fill-layer {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 2;
  overflow: visible;
  opacity: 0;
}

.adaptive-fill-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

/* Fade in the fill - use ease-in-out for smoother transition to breathing */
.adaptive-fill-layer.fill-active {
  animation: adaptive-fill-fadein 0.6s ease-in-out forwards;
}

.adaptive-fill-layer.fill-complete {
  opacity: 1;
}

@keyframes adaptive-fill-fadein {
  0% {
    opacity: 0;
    transform: scale(0.98);
  }
  100% {
    opacity: 1;
    transform: scale(1);
  }
}

/* Static mode fill with fade-in animation */
.static-fill {
  animation: static-fill-fadein 2s ease-in forwards;
}

@keyframes static-fill-fadein {
  0% {
    opacity: 0;
  }
  100% {
    opacity: 1;
  }
}

</style>
