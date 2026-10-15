<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import ClearableInput from './ClearableInput.vue'
import ServiceKeyField from './ServiceKeyField.vue'
import { use_service } from '../composables/use_service'
import { use_settings } from '../composables/use_settings'
import { control_restore } from '../lib/control'
import { ipc } from '../lib/ipc'
import { KEY_PURPOSES } from '../lib/key'
import type { ModelInfo, Thinking } from '../types'


interface ThinkingChoice {
    label: string
    value: Thinking | ''
}


const THINKING_CHOICES: readonly ThinkingChoice[] = Object.freeze([
    { label: 'Off', value: '' },
    { label: 'Low', value: 'low' },
    { label: 'Medium', value: 'medium' },
    { label: 'High', value: 'xhigh' },
])

const { service, service_error, service_host_save } = use_service()
const { settings, settings_change } = use_settings()

const host = computed(() => service.value?.host ?? '')

const host_draft = ref(host.value)
const models = ref<ModelInfo[]>([])

const model_choices = computed(() => {
    const chosen = settings.value?.cleanup_model ?? ''
    const listed = models.value.map((model) => model.id)

    if (chosen === '' || listed.includes(chosen)) return listed

    return [...listed, chosen].sort()
})

const thinking_available = computed(() => {
    const chosen = settings.value?.cleanup_model ?? ''

    return models.value.some((model) => model.id === chosen && model.thinking)
})

const thinking_shown = computed(() => {
    return thinking_available.value ? settings.value?.cleanup_thinking ?? '' : ''
})


watch(host, (value, previous) => {
    if (host_draft.value === previous) host_draft.value = value
})

watch(service, () => void models_refresh(), { immediate: true })

async function host_save() {
    if (host_draft.value.trim() !== host.value) await service_host_save(host_draft.value)

    host_draft.value = host.value
}

async function model_change(event: Event) {
    const model = (event.target as HTMLSelectElement).value
    const saved = await settings_change({ cleanup_model: model })

    if (!saved) control_restore(event, settings.value?.cleanup_model)
}

async function thinking_change(event: Event) {
    const value = (event.target as HTMLSelectElement).value
    const choice = THINKING_CHOICES.find((item) => item.value === value)
    const thinking = choice === undefined || choice.value === '' ? null : choice.value
    const saved = await settings_change({ cleanup_thinking: thinking })

    if (!saved) control_restore(event, thinking_shown.value)
}

async function models_refresh() {
    try {
        models.value = await ipc.models_list('llm')
    } catch (error) {
        models.value = []
        console.warn('[ora] the model list could not be loaded:', error)
    }
}
</script>

<template>
    <div>
        <label class="label" for="service-host">API address</label>
        <ClearableInput
            id="service-host"
            v-model="host_draft"
            class="input"
            type="url"
            spellcheck="false"
            autocomplete="off"
            placeholder="https://api.example.com"
            @blur="host_save"
            @keydown.enter="host_save"
        />
    </div>
    <ServiceKeyField v-for="purpose in KEY_PURPOSES" :key="purpose" :purpose="purpose" />
    <div v-if="settings !== null">
        <label class="label" for="cleanup-model">Model</label>
        <select
            id="cleanup-model"
            class="input"
            :value="settings.cleanup_model"
            @change="model_change"
        >
            <option v-for="model in model_choices" :key="model" :value="model">{{ model }}</option>
        </select>
    </div>
    <div v-if="settings !== null">
        <label class="label" for="cleanup-thinking">Thinking</label>
        <select
            id="cleanup-thinking"
            class="input"
            :value="thinking_shown"
            :disabled="!thinking_available"
            @change="thinking_change"
        >
            <option v-for="choice in THINKING_CHOICES" :key="choice.value" :value="choice.value">
                {{ choice.label }}
            </option>
        </select>
    </div>
    <p v-if="service_error !== null" class="message message-error">{{ service_error }}</p>
</template>
