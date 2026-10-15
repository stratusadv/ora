import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Step } from '../../src/lib/step'
import type { Platform } from '../../src/types'
import {
    HOST,
    PLATFORM_X11,
    SETTINGS_DEFAULT,
    backend_install,
    calls_of,
    service_keyless,
    service_ready,
} from './backend'
import type { Backend } from './backend'


beforeEach(() => {
    vi.resetModules()
})


async function step_mount(backend: Backend, step: Step, platform: Platform = PLATFORM_X11) {
    const { use_service } = await import('../../src/composables/use_service')
    const { use_settings } = await import('../../src/composables/use_settings')
    const SetupStep = (await import('../../src/components/SetupStep.vue')).default

    await use_settings().settings_load()
    await use_service().service_refresh()

    expect(backend.calls.length).toBeGreaterThan(0)

    return mount(SetupStep, { props: { platform, step } })
}


describe('SetupStep on the key step', () => {
    it('holds Continue until every field is filled, then saves them in order', async () => {
        const backend = backend_install({ service: service_keyless(), settings: SETTINGS_DEFAULT })
        const wrapper = await step_mount(backend, 'key')
        const inputs = wrapper.findAll('input')
        const submit = wrapper.get('button[type="submit"]')

        expect(wrapper.text()).toContain('API keys')
        const types = inputs.map((input) => input.attributes('type'))

        expect(types).toEqual(['url', 'password', 'password'])
        expect(submit.attributes('disabled')).toBeDefined()

        await inputs[0]?.setValue(` ${HOST} `)
        await inputs[1]?.setValue('audio-key')

        expect(submit.attributes('disabled')).toBeDefined()

        await inputs[2]?.setValue('notes-key')

        expect(submit.attributes('disabled')).toBeUndefined()

        await wrapper.get('form').trigger('submit')
        await flushPromises()

        const order = backend.calls
            .map((call) => call.command)
            .filter((command) => command === 'settings_update' || command === 'service_key_set')

        expect(order).toEqual(['settings_update', 'service_key_set', 'service_key_set'])
        expect(backend.settings.api_host).toBe(HOST)
        expect(calls_of(backend, 'service_key_set')).toEqual([
            { api_key: 'audio-key', purpose: 'transcription' },
            { api_key: 'notes-key', purpose: 'llm' },
        ])
    })

    it('asks only for the key that is missing and leaves a known address alone', async () => {
        const service = service_ready()

        service.keys.llm = { builtin: false, length: 0, user: false }

        const backend = backend_install({ service })
        const wrapper = await step_mount(backend, 'key')

        expect(wrapper.text()).toContain('the API key for')
        expect(wrapper.findAll('input[type="password"]')).toHaveLength(1)

        await wrapper.get('input[type="password"]').setValue('notes-key')
        await wrapper.get('form').trigger('submit')
        await flushPromises()

        expect(calls_of(backend, 'settings_update')).toEqual([])
        expect(calls_of(backend, 'service_key_set')).toEqual([
            { api_key: 'notes-key', purpose: 'llm' },
        ])
    })

    it('stops at the first key that fails, shows why, and keeps the fields', async () => {
        const backend = backend_install({ service: service_keyless(HOST) })
        const wrapper = await step_mount(backend, 'key')
        const keys = wrapper.findAll('input[type="password"]')

        backend.failures.set('service_key_set', 'config error: the key is too long')

        await keys[0]?.setValue('audio-key')
        await keys[1]?.setValue('notes-key')
        await wrapper.get('form').trigger('submit')
        await flushPromises()

        expect(calls_of(backend, 'service_key_set')).toHaveLength(1)
        expect(wrapper.get('.step-problem').text()).toBe(
            'The transcription key could not be saved. The key is too long',
        )
        expect(wrapper.findAll('input[type="password"]')).toHaveLength(2)
        expect(wrapper.get('button[type="submit"]').text()).toBe('Continue')
    })
})

describe('SetupStep on the shortcut and typing steps', () => {
    it('saves a recorded shortcut and shows why the old one did not bind', async () => {
        const shortcut_error = 'Taken by GNOME'
        const platform = { ...PLATFORM_X11, shortcut_bound: false, shortcut_error }
        const backend = backend_install()
        const wrapper = await step_mount(backend, 'shortcut', platform)
        const field = wrapper.get('.shortcut')

        expect(wrapper.get('.step-problem').text()).toBe('Taken by GNOME')

        await field.trigger('click')
        await field.trigger('keydown', { code: 'F8' })
        await flushPromises()

        expect(backend.settings.shortcut).toBe('F8')
    })

    it('asks the desktop for typing access and reports a refusal', async () => {
        const platform = { ...PLATFORM_X11, desktop: 'KDE', keystroke: 'portal' as const }
        const backend = backend_install({ platform })
        const wrapper = await step_mount(backend, 'typing', platform)
        const grant = wrapper.get('button')

        expect(wrapper.text()).toContain('KDE asks once')

        backend.failures.set('typing_allow', 'keystroke error: the request was cancelled')

        await grant.trigger('click')
        await flushPromises()

        expect(wrapper.get('.step-problem').text()).toBe('The request was cancelled')
        expect(grant.attributes('disabled')).toBeUndefined()

        backend.failures.clear()

        await grant.trigger('click')
        await flushPromises()

        expect(wrapper.find('.step-problem').exists()).toBe(false)
        expect(calls_of(backend, 'platform_get')).toHaveLength(1)
        expect(backend.platform.typing_allowed).toBe(true)
    })
})
