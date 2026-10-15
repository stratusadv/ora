import { listen } from '@tauri-apps/api/event'
import { ref } from 'vue'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'
import type { Platform, Status } from '../types'


const EVENT_PLATFORM = 'platform'
const EVENT_STATUS = 'status'

const platform = ref<Platform | null>(null)
const platform_error = ref<string | null>(null)
const started = ref(false)

const status = ref<Status>({
    error: null,
    notice: null,
    phase: 'idle',
})


async function platform_refresh(): Promise<void> {
    try {
        platform.value = await ipc.platform_get()
        platform_error.value = null
    } catch (error) {
        platform_error.value = error_message(error)
    }
}

async function runtime_start(): Promise<void> {
    if (started.value) return

    started.value = true

    await listen<Status>(EVENT_STATUS, (event) => {
        status.value = event.payload
    })

    await listen(EVENT_PLATFORM, () => {
        void platform_refresh()
    })

    status.value = await ipc.status_get()

    await platform_refresh()
}

export function use_runtime() {
    return { platform, platform_error, platform_refresh, runtime_start, status }
}
