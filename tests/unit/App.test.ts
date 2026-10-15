import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
    HOST,
    PLATFORM_X11,
    SETTINGS_DEFAULT,
    backend_emit,
    backend_install,
    service_keyless,
} from './backend'


beforeEach(() => {
    vi.resetModules()
})


async function app_mount() {
    const App = (await import('../../src/App.vue')).default
    const wrapper = mount(App)

    await flushPromises()

    return wrapper
}

function toasts_of(wrapper: Awaited<ReturnType<typeof app_mount>>): string[] {
    return wrapper.findAll('.toast-text').map((toast) => toast.text())
}


describe('App', () => {
    it('walks a first run from the keys through the shortcut to the home screen', async () => {
        const platform = { ...PLATFORM_X11, shortcut_bound: false }
        const service = service_keyless()
        const backend = backend_install({ platform, service, settings: SETTINGS_DEFAULT })
        const wrapper = await app_mount()

        expect(wrapper.get('.step-title').text()).toBe('Welcome to ora')

        const inputs = wrapper.findAll('input.step-input')

        expect(inputs).toHaveLength(3)

        await inputs[0]?.setValue(HOST)
        await inputs[1]?.setValue('audio-key')
        await inputs[2]?.setValue('notes-key')
        await wrapper.get('form').trigger('submit')
        await flushPromises()

        expect(wrapper.get('.step-title').text()).toBe('Set a shortcut')

        backend.platform.shortcut_bound = true

        await backend_emit('platform', null)
        await flushPromises()

        expect(wrapper.find('.step').exists()).toBe(false)
        expect(wrapper.get('.status-phase[data-current="true"]').text()).toBe('Ready')
        const keys = wrapper.findAll('div.status-phase[data-current="true"] kbd')

        expect(keys.map((key) => key.text())).toEqual(['Ctrl', 'Shift', 'Space'])
    })

    it('opens the settings over a pending step and returns home from the brand', async () => {
        const backend = backend_install({ service: service_keyless(HOST) })
        const wrapper = await app_mount()
        const gear = wrapper.get('button[aria-label="Settings"]')

        expect(wrapper.find('.step').exists()).toBe(true)

        await gear.trigger('click')

        expect(wrapper.find('.settings').exists()).toBe(true)
        expect(gear.attributes('aria-pressed')).toBe('true')

        await wrapper.get('.brand').trigger('click')

        expect(wrapper.find('.step').exists()).toBe(true)
        expect(backend.calls.some((call) => call.command === 'settings_update')).toBe(false)
    })

    it('shows a message once, keeps it closed, and shows it again when it returns', async () => {
        const backend = backend_install()

        backend.failures.set('platform_get', 'config error: the shortcut is taken')

        const wrapper = await app_mount()
        const taken = 'The shortcut is taken'
        const silent = 'No speech was detected.'

        await backend_emit('status', { error: taken, notice: silent, phase: 'idle' })

        expect(toasts_of(wrapper)).toEqual([taken, silent])
        expect(wrapper.findAll('.toast').map((toast) => toast.attributes('data-kind'))).toEqual(
            ['error', 'warning'],
        )

        await wrapper.get('.toast-close').trigger('click')
        await backend_emit('status', { error: taken, notice: silent, phase: 'listening' })

        expect(toasts_of(wrapper)).toEqual([silent])

        backend.failures.clear()

        await backend_emit('platform', null)
        await backend_emit('status', { error: null, notice: silent, phase: 'idle' })
        await flushPromises()

        expect(toasts_of(wrapper)).toEqual([silent])

        await backend_emit('status', { error: taken, notice: silent, phase: 'idle' })

        expect(toasts_of(wrapper)).toEqual([taken, silent])
    })

    it('puts an available update first and installs it from the toast', async () => {
        const update = { currentVersion: '0.1.0', rid: 1, version: '0.2.0' }
        const backend = backend_install({ update })

        vi.stubEnv('DEV', false)

        const wrapper = await app_mount()

        await backend_emit('status', { error: 'Offline', notice: null, phase: 'idle' })

        expect(toasts_of(wrapper)).toEqual(['An update is available.', 'Offline'])

        await wrapper.get('.toast-action').trigger('click')
        await flushPromises()

        expect(backend.calls.filter((call) => call.command === 'app_relaunch')).toHaveLength(1)

        vi.unstubAllEnvs()
    })
})
