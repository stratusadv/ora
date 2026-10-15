import { flushPromises } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { backend_install } from './backend'
import {
    preference_of,
    theme_apply,
    theme_of,
    theme_resolve,
    theme_store,
    theme_stored,
} from '../../src/lib/theme'
import type { ThemePreference } from '../../src/lib/theme'


beforeEach(() => {
    vi.resetModules()
    vi.restoreAllMocks()
    localStorage.clear()
    delete document.documentElement.dataset['theme']
})


function system_set(light: boolean): void {
    vi.spyOn(window, 'matchMedia').mockImplementation((query: string) => {
        const list = { addEventListener: vi.fn(), matches: light, media: query }

        return list as unknown as MediaQueryList
    })
}

describe('theme_resolve', () => {
    it('takes a chosen theme as it is and follows the system only when none is chosen', () => {
        expect(theme_resolve('light', false)).toBe('light')
        expect(theme_resolve('dark', true)).toBe('dark')
        expect(theme_resolve(null, true)).toBe('light')
        expect(theme_resolve(null, false)).toBe('dark')
    })
})

describe('preference_of and theme_of', () => {
    it('turn each preference into a stored theme and back unchanged', () => {
        const preferences: ThemePreference[] = ['system', 'light', 'dark']

        for (const preference of preferences) {
            expect(preference_of(theme_of(preference))).toBe(preference)
        }

        expect(theme_of('system')).toBeNull()
    })
})

describe('theme_store', () => {
    it('remembers a chosen theme, forgets it for the system, and ignores anything else', () => {
        theme_store('light')

        expect(theme_stored()).toBe('light')

        theme_store(null)

        expect(theme_stored()).toBeNull()

        localStorage.setItem('ora.theme', 'sepia')

        expect(theme_stored()).toBeNull()
    })
})

describe('theme_apply', () => {
    it('marks the page with the theme and its colour scheme', () => {
        theme_apply('light')

        expect(document.documentElement.dataset['theme']).toBe('light')
        expect(document.documentElement.style.colorScheme).toBe('light')

        theme_apply('dark')

        expect(document.documentElement.dataset['theme']).toBe('dark')
        expect(document.documentElement.style.colorScheme).toBe('dark')
    })
})

describe('use_theme', () => {
    it('paints the remembered theme first, then follows the stored setting', async () => {
        const backend = backend_install()

        system_set(false)
        localStorage.setItem('ora.theme', 'light')
        backend.settings = { ...backend.settings, theme: null }

        const { use_settings } = await import('../../src/composables/use_settings')
        const { use_theme } = await import('../../src/composables/use_theme')

        use_theme().theme_start()

        expect(document.documentElement.dataset['theme']).toBe('light')

        await use_settings().settings_load()
        await flushPromises()

        expect(document.documentElement.dataset['theme']).toBe('dark')
        expect(localStorage.getItem('ora.theme')).toBeNull()

        await use_settings().settings_change({ theme: 'light' })
        await flushPromises()

        expect(document.documentElement.dataset['theme']).toBe('light')
        expect(localStorage.getItem('ora.theme')).toBe('light')
        expect(() => {
            use_theme().theme_start()
        }).toThrow()
    })
})
