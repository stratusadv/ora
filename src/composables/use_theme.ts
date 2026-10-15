import { ref, watch } from 'vue'
import { assert } from '../lib/assert'
import { system_query, theme_apply, theme_resolve, theme_store, theme_stored } from '../lib/theme'
import { use_settings } from './use_settings'
import type { Theme } from '../types'


const { settings } = use_settings()

const started = ref(false)


function theme_chosen(): Theme | null {
    if (settings.value === null) return theme_stored()

    return settings.value.theme
}

function theme_refresh(): void {
    const system_light = system_query()?.matches ?? false

    theme_apply(theme_resolve(theme_chosen(), system_light))
}

function theme_start(): void {
    assert(!started.value, 'the theme starts once, before the app mounts')

    started.value = true
    theme_refresh()
    system_query()?.addEventListener('change', theme_refresh)

    watch(
        () => settings.value?.theme,
        (theme) => {
            if (theme === undefined) return

            theme_store(theme)
            theme_refresh()
        },
    )
}

export function use_theme() {
    return { theme_start }
}
