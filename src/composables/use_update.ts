import { ref, shallowRef } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { check } from '@tauri-apps/plugin-updater'
import type { Update } from '@tauri-apps/plugin-updater'
import { error_message } from '../lib/errors'
import { ipc } from '../lib/ipc'


export type UpdatePhase = 'available' | 'checking' | 'current' | 'failed' | 'idle' | 'installing'

const CHECK_TIMEOUT_MS = 15_000
const DOWNLOAD_TIMEOUT_MS = 600_000

const phase = ref<UpdatePhase>('idle')
const update_error = ref<string | null>(null)
const update_pending = shallowRef<Update | null>(null)
const version_app = ref<string | null>(null)
const version_update = ref<string | null>(null)


async function update_check(): Promise<void> {
    if (phase.value === 'checking') return
    if (phase.value === 'installing') return

    phase.value = 'checking'
    update_error.value = null

    try {
        const update = await check({ timeout: CHECK_TIMEOUT_MS })

        update_pending.value = update
        version_update.value = update?.version ?? null
        phase.value = update === null ? 'current' : 'available'
    } catch (error) {
        phase.value = 'failed'
        update_error.value = error_message(error)
    }
}

async function update_check_startup(): Promise<void> {
    version_app.value = await getVersion()

    if (import.meta.env.DEV) return
    if (!navigator.onLine) return

    await update_check()

    if (phase.value === 'failed') {
        console.warn('[ora] the update check failed:', update_error.value)
        phase.value = 'idle'
        update_error.value = null
    }
}

async function update_install(): Promise<void> {
    const update = update_pending.value

    if (update === null) return

    phase.value = 'installing'
    update_error.value = null

    try {
        await update.downloadAndInstall(undefined, { timeout: DOWNLOAD_TIMEOUT_MS })
        await ipc.app_relaunch()
    } catch (error) {
        phase.value = 'failed'
        update_error.value = error_message(error)
    }
}

export function use_update() {
    return {
        phase,
        update_check,
        update_check_startup,
        update_error,
        update_install,
        version_app,
        version_update,
    }
}
