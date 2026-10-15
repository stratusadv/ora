<script setup lang="ts">
import { computed, ref } from 'vue'
import {
    accelerator_from_keys,
    accelerator_guarded,
    accelerator_labels,
} from '../lib/accelerator'


const props = defineProps<{
    accelerator: string
    os: string
}>()

const emit = defineEmits<{
    change: [accelerator: string]
}>()

const HINT_UNGUARDED = 'A shortcut needs a Ctrl, Alt, or Super modifier, or a non-typing key '
    + 'such as F8 or Scroll Lock, so it does not intercept normal typing.'

const hint = ref<string | null>(null)
const recording = ref(false)
const labels = computed(() => accelerator_labels(props.accelerator, props.os))

const label_accessible = computed(() => {
    if (recording.value) return 'Press the new shortcut, or Escape to keep the current one'

    return `Change the dictation shortcut (currently ${labels.value.join(' ')})`
})


function key_down(event: KeyboardEvent) {
    if (!recording.value) return

    event.preventDefault()
    event.stopPropagation()

    if (event.code === 'Escape') {
        recording_stop()

        return
    }

    const accelerator = accelerator_from_keys({
        alt: event.altKey,
        code: event.code,
        control: event.ctrlKey,
        meta: event.metaKey,
        shift: event.shiftKey,
    })

    if (accelerator === null) return

    if (!accelerator_guarded(accelerator)) {
        hint.value = HINT_UNGUARDED

        return
    }

    recording_stop()
    emit('change', accelerator)
}

function key_up(event: KeyboardEvent) {
    if (recording.value) event.preventDefault()
}

function recording_start() {
    hint.value = null
    recording.value = true
}

function recording_stop() {
    recording.value = false
}
</script>

<template>
    <div class="shortcut-field">
        <button
            type="button"
            class="shortcut"
            :class="{ 'shortcut-recording': recording }"
            :aria-label="label_accessible"
            @click="recording_start"
            @keydown="key_down"
            @keyup="key_up"
            @blur="recording_stop"
        >
            <span v-if="recording" class="shortcut-prompt">Press a key combination…</span>
            <template v-else>
                <kbd v-for="label in labels" :key="label">{{ label }}</kbd>
            </template>
        </button>
        <p v-if="hint !== null" class="message message-warning">
            {{ hint }}
        </p>
    </div>
</template>

<style scoped>
.shortcut-field {
    display: grid;
    justify-items: start;
    gap: 0.5rem;
}

.shortcut {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    width: 100%;
    height: var(--control-height);
    padding: 0 0.3125rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-well);
    color: var(--color-content-strong);
    font: inherit;
    transition: border-color 120ms ease, box-shadow 120ms ease;
}

.shortcut:hover {
    border-color: var(--color-border-strong);
}

.shortcut-recording,
.shortcut-recording:hover {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-accent-soft);
}

.shortcut kbd {
    height: 1.5rem;
    margin: 0;
}

.shortcut-prompt {
    padding: 0 0.4375rem;
    color: var(--color-content-meta);
    font-size: 0.875rem;
}
</style>
