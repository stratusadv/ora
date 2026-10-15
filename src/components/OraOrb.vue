<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { use_waveform } from '../composables/use_waveform'
import type { Phase } from '../types'


type Layer = 'scan' | 'trace'

defineProps<{
    phase: Phase | 'waiting'
}>()

const AMPLITUDE = 85
const BASELINE = 100
const LAYERS: readonly Layer[] = Object.freeze(['trace', 'scan'])
const WIDTH = 1440

const { points, waveform_acquire, waveform_release } = use_waveform()

const trace = computed(() => {
    const values = points.value

    if (values.length < 2) return `M0 ${BASELINE}L${WIDTH} ${BASELINE}`

    const step = WIDTH / (values.length - 1)

    return values
        .map((value, index) => {
            const x = (index * step).toFixed(1)
            const y = (BASELINE - value * AMPLITUDE).toFixed(1)

            return `${index === 0 ? 'M' : 'L'}${x} ${y}`
        })
        .join('')
})


onMounted(() => {
    void waveform_acquire()
})

onBeforeUnmount(() => {
    void waveform_release()
})
</script>

<template>
    <div class="orb" :data-phase="phase" aria-hidden="true">
        <div v-for="layer in LAYERS" :key="layer" class="orb-layer" :data-layer="layer">
            <svg class="orb-strip" :viewBox="`0 0 ${WIDTH} 200`" preserveAspectRatio="none">
                <path class="orb-trace" :d="trace" />
            </svg>
        </div>
    </div>
</template>

<style scoped>
@property --tone {
    syntax: "<color>";
    inherits: true;
    initial-value: #8a8f98;
}

.orb {
    --tone: var(--color-content-muted);
    position: relative;
    width: var(--orb-width, 100%);
    height: 15rem;
    margin-bottom: 1rem;
    overflow: hidden;
    background: radial-gradient(var(--color-dot) 1px, transparent 1.5px) center / 16px 16px;
    transition: --tone 300ms ease;
    /* The edges fade by masking the content rather than painting the surface over it, so the
       8-bit steps of the two crossed gradients land on the sparse dots and stay invisible
       instead of drawing concentric rectangles across a light surface. */
    -webkit-mask:
        linear-gradient(90deg, transparent 4%, #000 38%, #000 62%, transparent 96%),
        linear-gradient(180deg, transparent, #000 25%, #000 75%, transparent);
    -webkit-mask-composite: source-in;
    mask:
        linear-gradient(90deg, transparent 4%, #000 38%, #000 62%, transparent 96%),
        linear-gradient(180deg, transparent, #000 25%, #000 75%, transparent);
    mask-composite: intersect;
}

.orb-layer {
    position: absolute;
    inset: 0;
}

.orb-strip {
    position: absolute;
    top: 0;
    left: 50%;
    display: block;
    width: 1440px;
    height: 100%;
    margin-left: -720px;
    overflow: visible;
}

.orb-trace {
    fill: none;
    stroke: var(--tone);
    stroke-linejoin: round;
    stroke-opacity: 0.6;
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
}

.orb-layer[data-layer="scan"] {
    opacity: 0;
    filter: drop-shadow(0 0 4px var(--color-accent));
    -webkit-mask: linear-gradient(90deg, transparent, #000 92%, transparent) -45% 0 / 30% 100% no-repeat;
    mask: linear-gradient(90deg, transparent, #000 92%, transparent) -45% 0 / 30% 100% no-repeat;
    transition: opacity 200ms ease;
    animation: orb-scan 1.6s linear infinite paused;
}

.orb-layer[data-layer="scan"] .orb-trace {
    stroke: var(--color-accent-top);
    stroke-opacity: 1;
    stroke-width: 2.5;
}

.orb[data-phase="listening"] {
    --tone: var(--color-listening);
}

.orb[data-phase="listening"] .orb-layer[data-layer="trace"] {
    filter: drop-shadow(0 0 5px color-mix(in oklab, var(--tone) 55%, transparent));
}

.orb[data-phase="listening"] .orb-layer[data-layer="trace"] .orb-trace {
    stroke-opacity: 1;
    stroke-width: 2;
}

.orb[data-phase="transcribing"] .orb-layer[data-layer="trace"] .orb-trace {
    stroke-opacity: 0.35;
}

.orb[data-phase="transcribing"] .orb-layer[data-layer="scan"] {
    opacity: 1;
    animation-play-state: running;
}

@keyframes orb-scan {
    to {
        -webkit-mask-position: 145% 0;
        mask-position: 145% 0;
    }
}

@media (prefers-reduced-motion: reduce) {
    .orb-layer[data-layer="scan"] {
        animation: none;
        -webkit-mask-position: 60% 0;
        mask-position: 60% 0;
    }

    .orb-layer {
        transition: none;
    }
}
</style>
