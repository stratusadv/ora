<script setup lang="ts">
import { useTemplateRef } from 'vue'


defineOptions({ inheritAttrs: false })

const ICON_CLEAR = 'M6 6l12 12M18 6L6 18'

const model = defineModel<string>({ required: true })

const input = useTemplateRef<HTMLInputElement>('input')


function clear() {
    model.value = ''
    input.value?.focus()
}
</script>

<template>
    <span class="clearable">
        <input ref="input" v-model="model" v-bind="$attrs" />
        <button
            v-if="model !== ''"
            type="button"
            class="clearable-button"
            aria-label="Clear"
            title="Clear"
            @mousedown.prevent
            @click="clear"
        >
            <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                aria-hidden="true"
            >
                <path :d="ICON_CLEAR" />
            </svg>
        </button>
    </span>
</template>

<style scoped>
.clearable {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
}

.clearable > input {
    flex: 1 1 auto;
    width: 100%;
    padding-right: 2rem;
}

.clearable-button {
    position: absolute;
    top: 50%;
    right: 0.25rem;
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--color-content-meta);
    cursor: pointer;
    transform: translateY(-50%);
    transition: background-color 120ms ease, color 120ms ease;
}

.clearable-button:hover {
    background: var(--color-raised-hover);
    color: var(--color-content-strong);
}

.clearable-button svg {
    width: 0.875rem;
    height: 0.875rem;
}
</style>
