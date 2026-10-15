import { ref } from 'vue'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'
import type { Settings } from '../types'


const settings = ref<Settings | null>(null)
const settings_error = ref<string | null>(null)


async function settings_change(patch: Partial<Settings>): Promise<boolean> {
    const current = settings.value

    if (current === null) return false

    try {
        settings.value = await ipc.settings_update({ ...current, ...patch })
        settings_error.value = null

        return true
    } catch (error) {
        settings_error.value = error_message(error)

        return false
    }
}

async function settings_load(): Promise<void> {
    try {
        settings.value = await ipc.settings_get()
        settings_error.value = null
    } catch (error) {
        settings_error.value = error_message(error)
    }
}

export function use_settings() {
    return { settings, settings_change, settings_error, settings_load }
}
