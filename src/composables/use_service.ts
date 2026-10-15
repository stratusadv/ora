import { ref } from 'vue'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'
import { KEY_NAMES } from '../lib/key'
import type { KeyPurpose, ServiceStatus } from '../types'
import { use_settings } from './use_settings'


const service = ref<ServiceStatus | null>(null)
const service_error = ref<string | null>(null)


async function service_host_save(host: string): Promise<boolean> {
    const { settings_change, settings_error } = use_settings()
    const trimmed = host.trim()
    const saved = await settings_change({ api_host: trimmed === '' ? null : trimmed })

    service_error.value = saved ? null : settings_error.value

    await service_refresh()

    return saved
}

async function service_key_save(purpose: KeyPurpose, api_key: string): Promise<boolean> {
    service_error.value = null

    try {
        service.value = await ipc.service_key_set(purpose, api_key)

        return true
    } catch (error) {
        const name = KEY_NAMES[purpose]
        service_error.value = `The ${name} could not be saved. ${error_message(error)}`

        return false
    }
}

async function service_refresh(): Promise<void> {
    try {
        service.value = await ipc.service_get()
    } catch (error) {
        service_error.value = error_message(error)
    }
}

export function use_service() {
    return {
        service,
        service_error,
        service_host_save,
        service_key_save,
        service_refresh,
    }
}
