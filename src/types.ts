export type KeyPurpose = 'llm' | 'transcription'

export type KeystrokeBackend = 'native' | 'portal' | 'uinput' | 'unavailable' | 'virtual_keyboard'

export type PasteKeys = 'control_shift_v' | 'control_v' | 'shift_insert'

export type Phase = 'idle' | 'listening' | 'transcribing'

export type ShortcutBackend = 'hotkey' | 'keyboard'

export type ShortcutMode = 'hold' | 'hold_or_tap' | 'tap'

export type Theme = 'dark' | 'light'

export type Thinking = 'low' | 'medium' | 'xhigh'

export interface KeySource {
    builtin: boolean
    length: number
    user: boolean
}

export interface Microphone {
    id: string
    name: string
}

export interface ModelInfo {
    id: string
    thinking: boolean
}

export interface Platform {
    desktop: string
    keystroke: KeystrokeBackend
    os: string
    shortcut: ShortcutBackend
    shortcut_bound: boolean
    shortcut_error: string | null
    typing_allowed: boolean
}

export interface ServiceStatus {
    host: string
    host_builtin: boolean
    keys: Record<KeyPurpose, KeySource>
}

export interface Settings {
    api_host: string | null
    cleanup_enabled: boolean
    cleanup_model: string
    cleanup_thinking: Thinking | null
    cues_enabled: boolean
    microphone: string | null
    paste_keys: PasteKeys
    shortcut: string
    shortcut_mode: ShortcutMode
    theme: Theme | null
    vocabulary: string
}

export interface Status {
    error: string | null
    notice: string | null
    phase: Phase
}
