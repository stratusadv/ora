import { flushPromises } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import {
    HOST,
    SETTINGS_DEFAULT,
    VERSION,
    backend_emit,
    backend_install,
    calls_of,
} from './backend'


beforeEach(() => {
    vi.resetModules()
})

afterEach(() => {
    vi.unstubAllEnvs()
    vi.restoreAllMocks()
})


describe('use_settings', () => {
    it('sends nothing before the settings have been read', async () => {
        const backend = backend_install()
        const { use_settings } = await import('../../src/composables/use_settings')
        const { settings_change } = use_settings()

        expect(await settings_change({ cues_enabled: false })).toBe(false)
        expect(calls_of(backend, 'settings_update')).toEqual([])
    })

    it('sends a change as the whole settings with the patch laid over them', async () => {
        const backend = backend_install()
        const { use_settings } = await import('../../src/composables/use_settings')
        const { settings, settings_change, settings_load } = use_settings()

        await settings_load()

        const patch = { cues_enabled: false, shortcut: 'F8' }
        const expected = { ...SETTINGS_DEFAULT, api_host: HOST, ...patch }

        expect(await settings_change(patch)).toBe(true)

        expect(calls_of(backend, 'settings_update')).toEqual([{ settings: expected }])
        expect(settings.value).toEqual(expected)
        expect(backend.settings).toEqual(expected)
    })

    it('keeps the last good settings through a refused change, then clears the error', async () => {
        const backend = backend_install()
        const { use_settings } = await import('../../src/composables/use_settings')
        const { settings, settings_change, settings_error, settings_load } = use_settings()

        await settings_load()

        const before = settings.value

        backend.failures.set('settings_update', 'shortcut error: F13 is not a shortcut')

        expect(await settings_change({ shortcut: 'F13' })).toBe(false)
        expect(settings.value).toEqual(before)
        expect(settings_error.value).toBe('F13 is not a shortcut')

        backend.failures.clear()

        expect(await settings_change({ shortcut: 'F8' })).toBe(true)
        expect(settings_error.value).toBeNull()
    })
})

describe('use_service', () => {
    it('stores a trimmed address and clears a blank one back to the built-in', async () => {
        const backend = backend_install()
        const { use_service } = await import('../../src/composables/use_service')
        const { use_settings } = await import('../../src/composables/use_settings')
        const { service, service_host_save } = use_service()

        await use_settings().settings_load()

        expect(await service_host_save('  https://other.test  ')).toBe(true)
        expect(backend.settings.api_host).toBe('https://other.test')
        expect(service.value?.host).toBe('https://other.test')

        expect(await service_host_save('   ')).toBe(true)
        expect(backend.settings.api_host).toBeNull()
    })

    it('names the key that failed to save and keeps the status it had', async () => {
        const backend = backend_install()
        const { use_service } = await import('../../src/composables/use_service')
        const { service, service_error, service_key_save, service_refresh } = use_service()

        await service_refresh()

        const before = service.value

        backend.failures.set('service_key_set', 'config error: the key is too long')

        expect(await service_key_save('llm', 'k')).toBe(false)
        expect(service_error.value).toBe('The LLM key could not be saved. The key is too long')
        expect(service.value).toEqual(before)

        backend.failures.clear()

        expect(await service_key_save('transcription', ' fresh ')).toBe(true)
        expect(service_error.value).toBeNull()
        expect(service.value?.keys.transcription).toEqual({ builtin: false, length: 5, user: true })
    })
})

describe('use_update', () => {
    it('reports up to date, then an available version, then a failed check', async () => {
        const backend = backend_install()
        const { use_update } = await import('../../src/composables/use_update')
        const { phase, update_check, update_error, version_update } = use_update()

        await update_check()

        expect(phase.value).toBe('current')
        expect(version_update.value).toBeNull()

        backend.update = { currentVersion: VERSION, rid: 1, version: '0.2.0' }

        await update_check()

        expect(phase.value).toBe('available')
        expect(version_update.value).toBe('0.2.0')

        backend.failures.set('plugin:updater|check', 'network error: offline')

        await update_check()

        expect(phase.value).toBe('failed')
        expect(update_error.value).toBe('Offline')
    })

    it('relaunches after an install and reports an install that failed', async () => {
        const update = { currentVersion: VERSION, rid: 1, version: '0.2.0' }
        const backend = backend_install({ update })
        const { use_update } = await import('../../src/composables/use_update')
        const { phase, update_check, update_error, update_install } = use_update()

        await update_check()
        await update_install()

        expect(calls_of(backend, 'plugin:updater|download_and_install')).toHaveLength(1)
        expect(calls_of(backend, 'app_relaunch')).toHaveLength(1)

        backend.failures.set('plugin:updater|download_and_install', 'signature mismatch')

        await update_install()

        expect(phase.value).toBe('failed')
        expect(update_error.value).toBe('Signature mismatch')
        expect(calls_of(backend, 'app_relaunch')).toHaveLength(1)
    })

    it('skips the startup check in development and keeps a failed one quiet', async () => {
        const backend = backend_install()
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)
        const { use_update } = await import('../../src/composables/use_update')
        const { phase, update_check_startup, update_error, version_app } = use_update()

        await update_check_startup()

        expect(version_app.value).toBe(VERSION)
        expect(calls_of(backend, 'plugin:updater|check')).toEqual([])

        vi.stubEnv('DEV', false)
        backend.failures.set('plugin:updater|check', 'network error: offline')

        await update_check_startup()

        expect(calls_of(backend, 'plugin:updater|check')).toHaveLength(1)
        expect(phase.value).toBe('idle')
        expect(update_error.value).toBeNull()
        expect(warn).toHaveBeenCalledOnce()
    })
})

describe('use_runtime', () => {
    it('reads the state once and then follows the events the backend sends', async () => {
        const backend = backend_install({ status: { error: null, notice: 'n', phase: 'idle' } })
        const { use_runtime } = await import('../../src/composables/use_runtime')
        const { platform, runtime_start, status } = use_runtime()

        await runtime_start()
        await runtime_start()

        expect(status.value.notice).toBe('n')
        expect(platform.value?.shortcut_bound).toBe(true)
        expect(calls_of(backend, 'status_get')).toHaveLength(1)

        await backend_emit('status', { error: null, notice: null, phase: 'listening' })

        expect(status.value.phase).toBe('listening')

        backend.platform.shortcut_bound = false

        await backend_emit('platform', null)
        await flushPromises()

        expect(platform.value?.shortcut_bound).toBe(false)
    })

    it('reports a platform that could not be read and clears it once it can', async () => {
        const backend = backend_install()
        const { use_runtime } = await import('../../src/composables/use_runtime')
        const { platform_error, platform_refresh } = use_runtime()

        backend.failures.set('platform_get', 'io error: no session bus')

        await platform_refresh()

        expect(platform_error.value).toBe('No session bus')

        backend.failures.clear()

        await platform_refresh()

        expect(platform_error.value).toBeNull()
    })
})

describe('use_waveform', () => {
    it('opens the monitor for the first holder and closes it after the last', async () => {
        const backend = backend_install()
        const { use_settings } = await import('../../src/composables/use_settings')
        const { use_waveform } = await import('../../src/composables/use_waveform')
        const { points, waveform_acquire, waveform_release } = use_waveform()

        await use_settings().settings_load()
        await waveform_acquire()
        await waveform_acquire()

        expect(calls_of(backend, 'monitor_open')).toHaveLength(1)

        await backend_emit('waveform', [0, 0.5, -0.5])

        expect(points.value).toEqual([0, 0.5, -0.5])

        await use_settings().settings_change({ microphone: 'usb' })
        await nextTick()

        expect(calls_of(backend, 'monitor_open')).toHaveLength(2)

        await waveform_release()

        expect(calls_of(backend, 'monitor_close')).toEqual([])

        await waveform_release()

        expect(calls_of(backend, 'monitor_close')).toHaveLength(1)
        expect(points.value).toEqual([])

        await use_settings().settings_change({ microphone: null })
        await nextTick()

        expect(calls_of(backend, 'monitor_open')).toHaveLength(2)
    })
})
