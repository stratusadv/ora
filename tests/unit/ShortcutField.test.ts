import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import ShortcutField from '../../src/components/ShortcutField.vue'


function field_mount() {
    return mount(ShortcutField, {
        props: { accelerator: 'Control+Shift+Space', os: 'linux' },
    })
}


describe('ShortcutField', () => {
    it('shows the current shortcut as one key per part', () => {
        const wrapper = field_mount()
        const keys = wrapper.findAll('kbd').map((key) => key.text())

        expect(keys).toEqual(['Ctrl', 'Shift', 'Space'])
    })

    it('records a guarded combination and reports it once', async () => {
        const wrapper = field_mount()
        const button = wrapper.get('button')

        await button.trigger('click')
        await button.trigger('keydown', { code: 'ControlLeft', ctrlKey: true })
        await button.trigger('keydown', { code: 'KeyD', ctrlKey: true, altKey: true })

        expect(wrapper.emitted('change')).toEqual([['Control+Alt+KeyD']])
        expect(wrapper.find('.shortcut-recording').exists()).toBe(false)
    })

    it('refuses a combination that would take over typing and keeps listening', async () => {
        const wrapper = field_mount()
        const button = wrapper.get('button')

        await button.trigger('click')
        await button.trigger('keydown', { code: 'KeyA', shiftKey: true })

        expect(wrapper.emitted('change')).toBeUndefined()
        expect(wrapper.find('.message-warning').exists()).toBe(true)
        expect(wrapper.find('.shortcut-recording').exists()).toBe(true)
    })

    it('keeps the current shortcut when Escape is pressed', async () => {
        const wrapper = field_mount()
        const button = wrapper.get('button')

        await button.trigger('click')
        await button.trigger('keydown', { code: 'Escape' })

        expect(wrapper.emitted('change')).toBeUndefined()
        expect(wrapper.find('.shortcut-recording').exists()).toBe(false)
    })

    it('ignores keys while it is not recording', async () => {
        const wrapper = field_mount()

        await wrapper.get('button').trigger('keydown', { code: 'KeyD', ctrlKey: true })

        expect(wrapper.emitted('change')).toBeUndefined()
    })
})
