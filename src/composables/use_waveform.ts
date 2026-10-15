import { listen } from '@tauri-apps/api/event'
import { ref, watch } from 'vue'
import { ipc } from '../lib/ipc'
import { use_settings } from './use_settings'


const EVENT_WAVEFORM = 'waveform'

const points = ref<readonly number[]>([])
const holders = ref(0)
const listening = ref(false)


async function monitor_open(): Promise<void> {
    try {
        await ipc.monitor_open()
    } catch (error) {
        console.warn('[ora] the live signal could not start:', error)
    }
}

async function waveform_acquire(): Promise<void> {
    holders.value += 1

    if (!listening.value) {
        listening.value = true

        await listen<number[]>(EVENT_WAVEFORM, (event) => {
            points.value = event.payload
        })

        watch(() => use_settings().settings.value?.microphone, () => {
            if (holders.value > 0) void monitor_open()
        })
    }

    if (holders.value === 1) await monitor_open()
}

async function waveform_release(): Promise<void> {
    holders.value = Math.max(holders.value - 1, 0)

    if (holders.value > 0) return

    points.value = []

    try {
        await ipc.monitor_close()
    } catch (error) {
        console.warn('[ora] the live signal could not stop:', error)
    }
}

export function use_waveform() {
    return { points, waveform_acquire, waveform_release }
}
