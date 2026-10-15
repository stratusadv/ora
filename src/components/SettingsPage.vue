<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import ServiceFields from './ServiceFields.vue'
import ShortcutField from './ShortcutField.vue'
import VocabularyFields from './VocabularyFields.vue'
import { use_runtime } from '../composables/use_runtime'
import { use_settings } from '../composables/use_settings'
import { use_update } from '../composables/use_update'
import { control_restore } from '../lib/control'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'
import type { Microphone, PasteKeys, ShortcutMode } from '../types'


interface ModeChoice {
    label: string
    value: ShortcutMode
}

interface PasteChoice {
    label: string
    value: PasteKeys
}

defineProps<{
    paste_manual: boolean
}>()

const MODE_CHOICES: readonly ModeChoice[] = Object.freeze([
    { label: 'Hold or toggle', value: 'hold_or_tap' },
    { label: 'Push to talk', value: 'hold' },
    { label: 'Toggle', value: 'tap' },
])

const PASTE_CHOICES: readonly PasteChoice[] = Object.freeze([
    { label: 'Ctrl + V', value: 'control_v' },
    { label: 'Ctrl + Shift + V (terminals)', value: 'control_shift_v' },
    { label: 'Shift + Insert', value: 'shift_insert' },
])

const { platform } = use_runtime()
const { settings, settings_change, settings_error } = use_settings()

const {
    phase: update_phase,
    update_check,
    update_install,
    version_app,
} = use_update()

const autostart = ref(false)
const microphones = ref<Microphone[]>([])
const page_error = ref<string | null>(null)

const os = computed(() => platform.value?.os ?? 'linux')

const update_busy = computed(() => {
    return update_phase.value === 'checking' || update_phase.value === 'installing'
})

const update_label = computed(() => {
    switch (update_phase.value) {
        case 'available':
            return 'Install update'
        case 'checking':
            return 'Checking…'
        case 'current':
            return 'Up to date'
        case 'installing':
            return 'Installing…'
        case 'failed':
        case 'idle':
            return 'Check for updates'
    }
})


async function autostart_change(event: Event) {
    const enabled = checked_of(event)

    try {
        autostart.value = await ipc.autostart_set(enabled)
    } catch (error) {
        page_error.value = error_message(error)
        control_restore(event, autostart.value)
    }
}

async function autostart_refresh() {
    try {
        autostart.value = await ipc.autostart_get()
    } catch (error) {
        page_error.value = error_message(error)
    }
}

function checked_of(event: Event): boolean {
    return (event.target as HTMLInputElement).checked
}

async function cleanup_change(event: Event) {
    const saved = await settings_change({ cleanup_enabled: checked_of(event) })

    if (!saved) control_restore(event, settings.value?.cleanup_enabled)
}

async function cues_change(event: Event) {
    const saved = await settings_change({ cues_enabled: checked_of(event) })

    if (!saved) control_restore(event, settings.value?.cues_enabled)
}

async function microphone_change(event: Event) {
    const microphone = value_of(event)
    const saved = await settings_change({ microphone: microphone === '' ? null : microphone })

    if (!saved) control_restore(event, settings.value?.microphone)
}

async function microphones_refresh() {
    try {
        microphones.value = await ipc.microphones_list()
    } catch (error) {
        page_error.value = error_message(error)
    }
}

async function mode_change(event: Event) {
    const value = value_of(event)
    const choice = MODE_CHOICES.find((candidate) => candidate.value === value)

    if (choice === undefined) return

    const saved = await settings_change({ shortcut_mode: choice.value })

    if (!saved) control_restore(event, settings.value?.shortcut_mode)
}

async function paste_keys_change(event: Event) {
    const value = value_of(event)
    const choice = PASTE_CHOICES.find((candidate) => candidate.value === value)

    if (choice === undefined) return

    const saved = await settings_change({ paste_keys: choice.value })

    if (!saved) control_restore(event, settings.value?.paste_keys)
}

function quit() {
    void ipc.app_quit()
}

function shortcut_change(accelerator: string) {
    void settings_change({ shortcut: accelerator })
}

function update_action() {
    if (update_phase.value === 'available') {
        void update_install()

        return
    }

    void update_check()
}

function value_of(event: Event): string {
    return (event.target as HTMLSelectElement).value
}

onMounted(async () => {
    await Promise.all([
        microphones_refresh(),
        autostart_refresh(),
    ])
})
</script>

<template>
    <section class="settings">
        <h2>Settings</h2>

        <p v-if="page_error !== null" class="message message-error">{{ page_error }}</p>
        <p v-if="settings_error !== null" class="message message-error">{{ settings_error }}</p>

        <template v-if="settings !== null">
            <div class="settings-section">
                <h3>Dictation</h3>
                <div class="card group">
                    <div>
                        <label class="label" for="microphone">Microphone</label>
                        <select
                            id="microphone"
                            class="input"
                            :value="settings.microphone ?? ''"
                            @focus="microphones_refresh"
                            @change="microphone_change"
                        >
                            <option value="">System default</option>
                            <option
                                v-for="microphone in microphones"
                                :key="microphone.id"
                                :value="microphone.id"
                            >
                                {{ microphone.name }}
                            </option>
                        </select>
                    </div>
                    <div>
                        <span class="label">Shortcut</span>
                        <ShortcutField
                            :accelerator="settings.shortcut"
                            :os="os"
                            @change="shortcut_change"
                        />
                        <p v-if="platform?.shortcut_error" class="message message-error">
                            {{ platform.shortcut_error }}
                        </p>
                    </div>
                    <div>
                        <label class="label" for="shortcut-mode">Shortcut mode</label>
                        <select
                            id="shortcut-mode"
                            class="input"
                            :value="settings.shortcut_mode"
                            @change="mode_change"
                        >
                            <option
                                v-for="choice in MODE_CHOICES"
                                :key="choice.value"
                                :value="choice.value"
                            >
                                {{ choice.label }}
                            </option>
                        </select>
                    </div>
                    <div>
                        <label class="label" for="paste-keys">Paste keystroke</label>
                        <select
                            id="paste-keys"
                            class="input"
                            :value="settings.paste_keys"
                            :disabled="paste_manual"
                            @change="paste_keys_change"
                        >
                            <option
                                v-for="choice in PASTE_CHOICES"
                                :key="choice.value"
                                :value="choice.value"
                            >
                                {{ choice.label }}
                            </option>
                        </select>
                    </div>
                </div>
            </div>

            <div class="settings-section">
                <h3>Vocabulary</h3>
                <div class="card group">
                    <VocabularyFields />
                </div>
            </div>

            <div class="settings-section">
                <h3>General</h3>
                <div class="card toggles">
                    <label class="toggle">
                        <span>Audio cues</span>
                        <input
                            class="switch"
                            type="checkbox"
                            role="switch"
                            :checked="settings.cues_enabled"
                            @change="cues_change"
                        />
                    </label>
                    <label class="toggle">
                        <span>Spelling &amp; grammar cleanup</span>
                        <input
                            class="switch"
                            type="checkbox"
                            role="switch"
                            :checked="settings.cleanup_enabled"
                            @change="cleanup_change"
                        />
                    </label>
                    <label class="toggle">
                        <span>Launch at startup</span>
                        <input
                            class="switch"
                            type="checkbox"
                            role="switch"
                            :checked="autostart"
                            @change="autostart_change"
                        />
                    </label>
                </div>
            </div>

            <div class="settings-section">
                <h3>Server</h3>
                <div class="card group">
                    <ServiceFields />
                </div>
            </div>
        </template>

        <div class="settings-section">
            <h3>About</h3>
            <div class="card group about">
                <p class="meta">ora {{ version_app ?? '' }}</p>
                <div class="actions-row">
                    <button type="button" class="btn-default" @click="quit">Quit ora</button>
                    <button
                        type="button"
                        :class="update_phase === 'available' ? 'btn-primary' : 'btn-default'"
                        :disabled="update_busy"
                        @click="update_action"
                    >
                        {{ update_label }}
                    </button>
                </div>
            </div>
        </div>
    </section>
</template>

<style scoped>
.settings {
    display: grid;
    gap: 1.75rem;
}

.settings-section {
    display: grid;
    gap: 0.625rem;
}

.group {
    display: grid;
    gap: 1.125rem;
    padding: 1.125rem;
}

.toggles {
    overflow: hidden;
}

.toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    min-height: 3.25rem;
    padding: 0.5rem 1.125rem;
    color: var(--color-content-strong);
    cursor: pointer;
    transition: background-color 120ms ease;
}

.toggle:hover {
    background-color: var(--color-row-hover);
}

.toggle + .toggle {
    border-top: 1px solid var(--color-border);
}

.about {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
}
</style>
