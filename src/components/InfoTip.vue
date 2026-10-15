<script setup lang="ts">
import { computed, ref, useId } from 'vue'


defineProps<{
    label: string
    text: string
}>()

const ICON_INFO = 'M8 7.25v4M8 4.75v.01'

const id = useId()
const hovered = ref(false)
const pinned = ref(false)

const open = computed(() => hovered.value || pinned.value)


function pinned_toggle() {
    pinned.value = !pinned.value
}

function unpin() {
    pinned.value = false
}
</script>

<template>
    <span class="tip" @mouseenter="hovered = true" @mouseleave="hovered = false">
        <button
            type="button"
            class="tip-button"
            :aria-label="label"
            :aria-describedby="id"
            :aria-expanded="open"
            @click="pinned_toggle"
            @blur="unpin"
            @keydown.esc="unpin"
        >
            <svg
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
                aria-hidden="true"
            >
                <circle cx="8" cy="8" r="6.3" />
                <path :d="ICON_INFO" />
            </svg>
        </button>
        <span :id="id" role="tooltip" class="tip-text" :data-open="open">{{ text }}</span>
    </span>
</template>

<style scoped>
.tip {
    position: relative;
    display: inline-flex;
}

.tip-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--color-content-meta);
    transition: color 120ms ease;
}

.tip-button:hover,
.tip-button[aria-expanded="true"] {
    color: var(--color-content-strong);
}

.tip-button svg {
    width: 0.875rem;
    height: 0.875rem;
}

.tip-text {
    position: absolute;
    bottom: calc(100% + 0.375rem);
    left: -0.5rem;
    z-index: 30;
    width: max-content;
    max-width: 16rem;
    padding: 0.4375rem 0.625rem;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-float);
    box-shadow: var(--shadow-float);
    color: var(--color-content);
    font-size: 0.8125rem;
    font-weight: 400;
    line-height: 1.45;
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition:
        opacity 120ms ease,
        visibility 0s linear 120ms;
}

.tip-text[data-open="true"] {
    opacity: 1;
    visibility: visible;
    transition-delay: 0s;
}

@media (prefers-reduced-motion: reduce) {
    .tip-text {
        transition: none;
    }
}
</style>
