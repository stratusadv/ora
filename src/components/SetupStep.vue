<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import stratus_mark from '../../assets/stratus-mark.svg'
import ClearableInput from './ClearableInput.vue'
import ShortcutField from './ShortcutField.vue'
import { use_runtime } from '../composables/use_runtime'
import { use_service } from '../composables/use_service'
import { use_settings } from '../composables/use_settings'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'
import { KEY_LABELS, KEY_PURPOSES, keys_missing } from '../lib/key'
import type { Step } from '../lib/step'
import type { KeyPurpose, Platform } from '../types'


const props = defineProps<{
    platform: Platform | null
    step: Step
}>()

const ICON_ARROW = 'M5 12h14M13 6l6 6-6 6'

const { platform_refresh } = use_runtime()
const { service, service_error, service_host_save, service_key_save } = use_service()
const { settings, settings_change, settings_error } = use_settings()

const busy = ref(false)
const error = ref<string | null>(null)
const host_draft = ref(service.value?.host ?? '')
const key_drafts = reactive<Record<KeyPurpose, string>>({ llm: '', transcription: '' })

const desktop = computed(() => props.platform?.desktop ?? 'The desktop')
const host_changed = computed(() => host_draft.value.trim() !== (service.value?.host ?? ''))
const os = computed(() => props.platform?.os ?? 'linux')

const keys_needed = computed<readonly KeyPurpose[]>(() => {
    return service.value === null ? KEY_PURPOSES : keys_missing(service.value)
})

const keys_shown = ref<readonly KeyPurpose[]>(keys_needed.value)
const submitting = ref(false)

const keys_ready = computed(() => {
    if (host_draft.value.trim() === '') return false

    return keys_shown.value.every((purpose) => key_drafts[purpose].trim() !== '')
})

const title = computed(() => {
    switch (props.step) {
        case 'key':
            return 'Welcome to ora'
        case 'shortcut':
            return 'Set a shortcut'
        case 'typing':
            return 'Allow keyboard input'
    }
})

const problem = computed(() => {
    if (props.step === 'key') return service_error.value
    if (props.step === 'typing') return error.value

    return error.value ?? settings_error.value ?? props.platform?.shortcut_error ?? null
})


watch(() => service.value?.host ?? '', (value, previous) => {
    if (host_draft.value === previous) host_draft.value = value
})

watch(keys_needed, (needed) => {
    if (!submitting.value && needed.length > 0) keys_shown.value = needed
})

async function key_submit() {
    submitting.value = true

    try {
        await keys_save()
    } finally {
        submitting.value = false

        if (keys_needed.value.length > 0) keys_shown.value = keys_needed.value
    }
}

async function keys_save() {
    if (host_changed.value) {
        const saved = await service_host_save(host_draft.value)

        if (!saved) return
    }

    for (const purpose of keys_shown.value) {
        if (!await service_key_save(purpose, key_drafts[purpose])) return
    }
}

function shortcut_change(accelerator: string) {
    void settings_change({ shortcut: accelerator })
}

async function typing_allow() {
    busy.value = true
    error.value = null

    try {
        await ipc.typing_allow()
        await platform_refresh()
    } catch (failure) {
        error.value = error_message(failure)
    } finally {
        busy.value = false
    }
}
</script>

<template>
    <section class="step">
        <aside class="step-art">
            <img :src="stratus_mark" alt="" />
            <span class="step-company">Stratus Advanced Technologies</span>
        </aside>

        <div class="step-side">
            <div class="step-body">
                <h1 v-if="step === 'key'" class="step-title">
                    Welcome to <span class="step-brand">ora</span>
                </h1>
                <h1 v-else class="step-title">{{ title }}</h1>

                <p v-if="step === 'key'" class="step-text">
                    Please enter the {{ keys_shown.length > 1 ? 'API keys' : 'API key' }} for this
                    app.
                </p>
                <p v-else-if="step === 'shortcut'" class="step-text">
                    Select the field, then press the key combination to use.
                </p>
                <p v-else class="step-text">
                    {{ desktop }} asks once for permission to control input. Allow it in the
                    dialog that opens.
                </p>

                <form v-if="step === 'key'" class="step-form" @submit.prevent="key_submit">
                    <label class="step-field">
                        <span class="step-label">API address</span>
                        <ClearableInput
                            v-model="host_draft"
                            class="step-input"
                            type="url"
                            placeholder="https://api.example.com"
                            spellcheck="false"
                            autocomplete="off"
                        />
                    </label>
                    <label v-for="purpose in keys_shown" :key="purpose" class="step-field">
                        <span class="step-label">{{ KEY_LABELS[purpose] }}</span>
                        <ClearableInput
                            v-model="key_drafts[purpose]"
                            class="step-input"
                            type="password"
                            placeholder="Paste the key"
                            autocomplete="off"
                        />
                    </label>
                    <button
                        type="submit"
                        class="btn-primary step-action"
                        :disabled="submitting || !keys_ready"
                    >
                        {{ submitting ? 'Saving…' : 'Continue' }}
                        <svg
                            v-if="!submitting"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path :d="ICON_ARROW" />
                        </svg>
                    </button>
                </form>

                <div v-else-if="step === 'shortcut'" class="step-form">
                    <ShortcutField
                        v-if="settings !== null"
                        :accelerator="settings.shortcut"
                        :os="os"
                        @change="shortcut_change"
                    />
                </div>

                <div v-else class="step-form">
                    <button
                        type="button"
                        class="btn-primary step-action"
                        :disabled="busy"
                        @click="typing_allow"
                    >
                        Grant access
                    </button>
                </div>

                <p v-if="problem !== null" class="message message-error step-problem">
                    {{ problem }}
                </p>
            </div>
        </div>
    </section>
</template>

<style scoped>
.step {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    flex: 1;
}

.step-art {
    display: flex;
    grid-row: 2;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    height: 3rem;
    border-top: 1px solid var(--color-border);
    background: var(--color-footer);
}

.step-art img {
    width: 1.25rem;
    opacity: 0.5;
    user-select: none;
}

.step-company {
    color: var(--color-content-meta);
    font-size: 0.625rem;
    font-weight: 500;
    letter-spacing: 0.24em;
    opacity: 0.5;
    text-transform: uppercase;
    white-space: nowrap;
}

.step-side {
    display: grid;
    place-items: center;
    padding: 4.5rem 1.75rem 1.5rem;
}

.step-body {
    display: grid;
    justify-items: start;
    width: 100%;
    max-width: 22rem;
}

.step-title {
    color: var(--color-content-strong);
    font-size: 2rem;
    font-weight: 600;
    letter-spacing: -0.045em;
    line-height: 1.1;
}

.step-brand {
    color: var(--color-accent);
}

.step-text {
    margin-top: 0.625rem;
    color: var(--color-content-muted);
    font-size: 0.9375rem;
}

.step-form {
    display: grid;
    gap: 1.25rem;
    width: 100%;
    margin-top: 1.75rem;
}

.step-field {
    display: grid;
    gap: 0.25rem;
}

.step-label {
    color: var(--color-content-meta);
    font-size: 0.75rem;
    font-weight: 500;
}

.step-form :deep(.step-input) {
    height: 2.25rem;
    padding: 0 2rem 0 0;
    border: 0;
    border-bottom: 1px solid var(--color-border-strong);
    border-radius: 0;
    background: transparent;
    color: var(--color-content-strong);
    font: inherit;
    font-size: 0.9375rem;
    transition: border-color 140ms ease;
}

.step-form :deep(.step-input::placeholder) {
    color: var(--color-content-meta);
}

.step-form :deep(.step-input:hover:not(:focus)) {
    border-bottom-color: var(--color-underline-hover);
}

.step-form :deep(.step-input:focus) {
    outline: none;
    border-bottom-color: var(--color-accent);
}

.step-action {
    justify-self: start;
    gap: 0.5rem;
    height: 2.5rem;
    margin-top: 0.5rem;
    padding: 0 1.125rem 0 1.25rem;
    border-radius: 999px;
}

.step-action svg {
    width: 1rem;
    height: 1rem;
}

.step-problem {
    width: 100%;
    margin-top: 1.25rem;
}

.step :deep(.shortcut-field) {
    width: 100%;
}
</style>
