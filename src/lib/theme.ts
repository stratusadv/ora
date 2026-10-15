import type { Theme } from '../types'


export type ThemePreference = Theme | 'system'

const QUERY_LIGHT = '(prefers-color-scheme: light)'
const STORAGE_KEY = 'ora.theme'


export function preference_of(theme: Theme | null): ThemePreference {
    return theme ?? 'system'
}

export function system_query(): MediaQueryList | null {
    if (typeof window.matchMedia !== 'function') return null

    return window.matchMedia(QUERY_LIGHT)
}

export function theme_apply(theme: Theme): void {
    const root = document.documentElement

    root.dataset['theme'] = theme
    root.style.colorScheme = theme
}

export function theme_of(preference: ThemePreference): Theme | null {
    return preference === 'system' ? null : preference
}

export function theme_resolve(theme: Theme | null, system_light: boolean): Theme {
    if (theme !== null) return theme

    return system_light ? 'light' : 'dark'
}

// The stored copy only spares the first paint a flash of the wrong theme before the settings
// arrive, so a storage that refuses access costs that one frame and nothing else.
export function theme_store(theme: Theme | null): void {
    try {
        if (theme === null) {
            localStorage.removeItem(STORAGE_KEY)

            return
        }

        localStorage.setItem(STORAGE_KEY, theme)
    } catch (error) {
        console.warn('the theme could not be remembered for the next launch', error)
    }
}

export function theme_stored(): Theme | null {
    try {
        const stored = localStorage.getItem(STORAGE_KEY)

        if (stored === 'dark') return 'dark'
        if (stored === 'light') return 'light'

        return null
    } catch (error) {
        console.warn('the remembered theme could not be read', error)

        return null
    }
}
