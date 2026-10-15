<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import ClearableInput from './ClearableInput.vue'
import InfoTip from './InfoTip.vue'
import { use_service } from '../composables/use_service'
import { KEY_LABELS, KEY_NAMES } from '../lib/key'
import type { KeyPurpose } from '../types'


const props = defineProps<{
    purpose: KeyPurpose
}>()

const MASK_CHAR = '•'
const MASK_LENGTH_MAX = 1024

const { service, service_key_save } = use_service()

const id = computed(() => `service-key-${props.purpose}`)
const label = computed(() => KEY_LABELS[props.purpose])
const source = computed(() => service.value?.keys[props.purpose] ?? null)
const present = computed(() => source.value?.builtin === true || source.value?.user === true)
const mask = computed(() => {
    const length = Math.min(Math.max(source.value?.length ?? 0, 1), MASK_LENGTH_MAX)

    return MASK_CHAR.repeat(length)
})

const resting = computed(() => present.value ? mask.value : '')

const draft = ref(resting.value)

const state = computed(() => {
    if (source.value === null) return ''
    if (source.value.user) return 'A key is stored on this computer.'
    if (source.value.builtin) return 'This build includes the key.'

    return 'No key is configured.'
})


watch(resting, (value, previous) => {
    if (draft.value === previous) draft.value = value
})

function focus(event: FocusEvent) {
    if (present.value && draft.value === mask.value) (event.target as HTMLInputElement).select()
}

function leave(event: KeyboardEvent) {
    (event.target as HTMLInputElement).blur()
}

async function save() {
    if (draft.value === resting.value) return

    await service_key_save(props.purpose, draft.value)

    draft.value = resting.value
}
</script>

<template>
    <div>
        <div class="label-row">
            <label class="label" :for="id">{{ label }}</label>
            <InfoTip v-if="state !== ''" :label="`About the ${KEY_NAMES[purpose]}`" :text="state" />
        </div>
        <ClearableInput
            :id="id"
            v-model="draft"
            class="input"
            type="password"
            autocomplete="off"
            placeholder="API key"
            @blur="save"
            @focus="focus"
            @keydown.enter.prevent="leave"
        />
    </div>
</template>
