import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { HOST, backend_install, calls_of, service_ready } from './backend'
import type { Backend } from './backend'


beforeEach(() => {
    vi.resetModules()
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)
})


async function component_mount(
    backend: Backend,
    name: string,
    props: Record<string, unknown> = {},
) {
    const { use_runtime } = await import('../../src/composables/use_runtime')
    const { use_service } = await import('../../src/composables/use_service')
    const { use_settings } = await import('../../src/composables/use_settings')
    const component = (await import(`../../src/components/${name}.vue`)) as { default: object }

    await use_settings().settings_load()
    await use_service().service_refresh()
    await use_runtime().platform_refresh()

    expect(backend.calls.length).toBe(3)

    const wrapper = mount(component.default, { props })

    await flushPromises()

    return wrapper
}


describe('SettingsPage', () => {
    it('sends each choice as the value the backend stores', async () => {
        const backend = backend_install({ microphones: [{ id: 'usb', name: 'USB mic' }] })
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: false })

        expect(wrapper.findAll('#microphone option').map((option) => option.text())).toEqual([
            'System default',
            'USB mic',
        ])

        await wrapper.get('#microphone').setValue('usb')
        await wrapper.get('#shortcut-mode').setValue('tap')
        await wrapper.get('#paste-keys').setValue('shift_insert')
        await flushPromises()

        expect(backend.settings.microphone).toBe('usb')
        expect(backend.settings.shortcut_mode).toBe('tap')
        expect(backend.settings.paste_keys).toBe('shift_insert')

        await wrapper.get('#microphone').setValue('')
        await flushPromises()

        expect(backend.settings.microphone).toBeNull()
    })

    it('flips each switch through the backend', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: false })
        const [cues, cleanup, autostart] = wrapper.findAll('input[role="switch"]')

        await cues?.setValue(false)
        await cleanup?.setValue(false)
        await autostart?.setValue(true)
        await flushPromises()

        expect(backend.settings.cues_enabled).toBe(false)
        expect(backend.settings.cleanup_enabled).toBe(false)
        expect(backend.autostart).toBe(true)
    })

    it('puts the startup switch back and says why when the system refuses it', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: false })
        const autostart = wrapper.findAll('input[role="switch"]')[2]

        backend.failures.set('autostart_set', 'config error: the system did not apply it')

        await autostart?.setValue(true)
        await flushPromises()

        expect((autostart?.element as HTMLInputElement).checked).toBe(false)
        expect(wrapper.get('.message-error').text()).toBe('The system did not apply it')
    })

    it('puts a refused switch or choice back to what is stored', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: false })
        const cues = wrapper.findAll('input[role="switch"]')[0]
        const mode = wrapper.get('#shortcut-mode')

        backend.failures.set('settings_update', 'io error: the disk is full')

        await cues?.setValue(false)
        await mode.setValue('hold')
        await flushPromises()

        expect((cues?.element as HTMLInputElement).checked).toBe(true)
        expect((mode.element as HTMLSelectElement).value).toBe('hold_or_tap')
        expect(wrapper.get('.message-error').text()).toBe('The disk is full')
        expect(backend.settings.cues_enabled).toBe(true)
    })

    it('locks the paste keys when the desktop cannot type', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: true })

        expect(wrapper.get('#paste-keys').attributes('disabled')).toBeDefined()
    })

    it('labels the update button by where the check stands', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'SettingsPage', { paste_manual: false })
        const button = wrapper.findAll('.actions-row button')[1]

        expect(button?.text()).toBe('Check for updates')

        await button?.trigger('click')
        await flushPromises()

        expect(button?.text()).toBe('Up to date')

        backend.update = { currentVersion: '0.1.0', rid: 1, version: '0.2.0' }

        await button?.trigger('click')
        await flushPromises()

        expect(button?.text()).toBe('Install update')

        await button?.trigger('click')
        await flushPromises()

        expect(calls_of(backend, 'plugin:updater|download_and_install')).toHaveLength(1)
        expect(calls_of(backend, 'app_relaunch')).toHaveLength(1)
    })
})

describe('ServiceFields', () => {
    it('offers the listed models plus a chosen one the server no longer lists', async () => {
        const backend = backend_install({
            models: [
                { id: 'stratus.thinking', thinking: true },
                { id: 'stratus.turbo', thinking: false },
            ],
        })

        backend.settings.cleanup_model = 'stratus.legacy'

        const wrapper = await component_mount(backend, 'ServiceFields')
        const models = wrapper.findAll('#cleanup-model option').map((option) => option.text())

        expect(models).toEqual(['stratus.legacy', 'stratus.thinking', 'stratus.turbo'])
        expect(calls_of(backend, 'models_list')).toEqual([{ purpose: 'llm' }])
    })

    it('enables thinking only on a model that thinks and sends Off as no level', async () => {
        const backend = backend_install({
            models: [
                { id: 'stratus.thinking', thinking: true },
                { id: 'stratus.turbo', thinking: false },
            ],
        })

        backend.settings.cleanup_thinking = 'medium'

        const wrapper = await component_mount(backend, 'ServiceFields')
        const thinking = wrapper.get('#cleanup-thinking')

        expect(thinking.attributes('disabled')).toBeDefined()
        expect((thinking.element as HTMLSelectElement).value).toBe('')

        await wrapper.get('#cleanup-model').setValue('stratus.thinking')
        await flushPromises()

        expect(thinking.attributes('disabled')).toBeUndefined()
        expect((thinking.element as HTMLSelectElement).value).toBe('medium')

        await thinking.setValue('xhigh')
        await flushPromises()

        expect(backend.settings.cleanup_thinking).toBe('xhigh')

        await thinking.setValue('')
        await flushPromises()

        expect(backend.settings.cleanup_thinking).toBeNull()

        backend.failures.set('settings_update', 'io error: the disk is full')

        await thinking.setValue('low')
        await wrapper.get('#cleanup-model').setValue('stratus.turbo')
        await flushPromises()

        expect((thinking.element as HTMLSelectElement).value).toBe('')
        expect((wrapper.get('#cleanup-model').element as HTMLSelectElement).value).toBe(
            'stratus.thinking',
        )
    })

    it('saves the address only when it changed', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'ServiceFields')
        const host = wrapper.get('#service-host')

        await host.trigger('blur')
        await flushPromises()

        expect(calls_of(backend, 'settings_update')).toEqual([])

        await host.setValue('https://other.test/')
        await host.trigger('keydown', { key: 'Enter' })
        await flushPromises()

        expect(backend.settings.api_host).toBe('https://other.test/')
        expect((host.element as HTMLInputElement).value).toBe('https://other.test/')
    })

    it('masks a stored key, keeps it on an untouched blur, and saves a new one', async () => {
        const backend = backend_install()
        const wrapper = await component_mount(backend, 'ServiceFields')
        const key = wrapper.get('#service-key-llm')

        expect((key.element as HTMLInputElement).value).toBe('•'.repeat(8))
        expect(wrapper.text()).not.toContain('This build includes the key.')

        await key.trigger('blur')
        await flushPromises()

        expect(calls_of(backend, 'service_key_set')).toEqual([])

        await key.setValue('replacement')
        await key.trigger('blur')
        await flushPromises()

        expect(calls_of(backend, 'service_key_set')).toEqual([
            { api_key: 'replacement', purpose: 'llm' },
        ])
        expect((key.element as HTMLInputElement).value).toBe('•'.repeat('replacement'.length))
    })

    it('reports a failed model list as no models rather than an error', async () => {
        const backend = backend_install()

        backend.failures.set('models_list', 'network error: offline')

        const wrapper = await component_mount(backend, 'ServiceFields')
        const models = wrapper.findAll('#cleanup-model option').map((option) => option.text())

        expect(models).toEqual(['stratus.turbo'])
        expect(wrapper.find('.message-error').exists()).toBe(false)
        expect(backend.service).toEqual(service_ready())
        expect(backend.settings.api_host).toBe(HOST)
    })
})

describe('VocabularyFields', () => {
    it('adds new terms once each and removes the one asked for', async () => {
        const backend = backend_install()

        backend.settings.vocabulary = 'Stratus\nora'

        const wrapper = await component_mount(backend, 'VocabularyFields')
        const input = wrapper.get('#vocabulary-term')

        await input.setValue('ORA, Tauri, tauri, Vite')
        await input.trigger('keydown', { key: 'Enter' })
        await flushPromises()

        expect(backend.settings.vocabulary).toBe('Stratus\nora\nTauri\nVite')
        expect((input.element as HTMLInputElement).value).toBe('')

        await wrapper.get('button[aria-label="Remove ora"]').trigger('click')
        await flushPromises()

        expect(backend.settings.vocabulary).toBe('Stratus\nTauri\nVite')
        const terms = wrapper.findAll('.term').map((term) => term.text())

        expect(terms).toEqual(['Stratus', 'Tauri', 'Vite'])
    })

    it('keeps a refused draft and sends nothing for a known term', async () => {
        const backend = backend_install()

        backend.settings.vocabulary = 'Stratus'

        const wrapper = await component_mount(backend, 'VocabularyFields')
        const input = wrapper.get('#vocabulary-term')

        await input.setValue('stratus')
        await input.trigger('blur')
        await flushPromises()

        expect(calls_of(backend, 'settings_update')).toEqual([])

        backend.failures.set('settings_update', 'config error: Vocabulary is too long')

        await input.setValue('Tauri')
        await input.trigger('blur')
        await flushPromises()

        expect((input.element as HTMLInputElement).value).toBe('Tauri')
    })
})

describe('StatusPanel', () => {
    it('tells the user what to press for the current mode and phase', async () => {
        const backend = backend_install()

        backend.settings.shortcut_mode = 'tap'

        const status = { error: null, notice: null, phase: 'listening' as const }
        const props = { paste_manual: true, shortcut: ['Ctrl', 'Space'], status }
        const wrapper = await component_mount(backend, 'StatusPanel', props)
        const phases = wrapper.findAll('.status-phase[data-current="true"]')
        const current = phases.map((node) => node.text())

        expect(current[0]).toBe('Recording')
        expect(current[1]).toMatch(/^Press\s+CtrlSpace\s+again to stop$/)
        expect(wrapper.get('.status-orb').attributes('aria-label')).toBe('Stop recording')
        expect(wrapper.text()).toContain('Paste the result with')
    })

    it('toggles dictation from the orb and opens the live signal while shown', async () => {
        const backend = backend_install()
        const status = { error: null, notice: null, phase: 'idle' as const }
        const props = { paste_manual: false, shortcut: ['F8'], status }
        const wrapper = await component_mount(backend, 'StatusPanel', props)

        await wrapper.get('.status-orb').trigger('click')
        await flushPromises()

        expect(calls_of(backend, 'dictation_toggle')).toHaveLength(1)
        expect(calls_of(backend, 'monitor_open')).toHaveLength(1)

        wrapper.unmount()
        await flushPromises()

        expect(calls_of(backend, 'monitor_close')).toHaveLength(1)
    })
})
