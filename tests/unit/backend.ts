import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type {
    KeyPurpose,
    Microphone,
    ModelInfo,
    Platform,
    ServiceStatus,
    Settings,
    Status,
} from '../../src/types'


export interface Backend {
    autostart: boolean
    calls: Call[]
    failures: Map<string, string>
    microphones: Microphone[]
    models: ModelInfo[]
    platform: Platform
    service: ServiceStatus
    settings: Settings
    status: Status
    update: UpdateMetadata | null
}

export interface Call {
    args: Record<string, unknown>
    command: string
}

interface UpdateMetadata {
    currentVersion: string
    rid: number
    version: string
}

export const HOST = 'https://api.example.test'
export const VERSION = '0.1.0'

export const PLATFORM_X11: Platform = Object.freeze({
    desktop: 'GNOME',
    keystroke: 'native',
    os: 'linux',
    shortcut: 'hotkey',
    shortcut_bound: true,
    shortcut_error: null,
    typing_allowed: false,
})

export const SETTINGS_DEFAULT: Settings = Object.freeze({
    api_host: null,
    cleanup_enabled: false,
    cleanup_model: 'stratus.turbo',
    cleanup_thinking: null,
    cues_enabled: true,
    microphone: null,
    paste_keys: 'control_v',
    shortcut: 'Control+Shift+Space',
    shortcut_mode: 'hold_or_tap',
    vocabulary: '',
})


export function backend_emit(event: string, payload: unknown): Promise<void> {
    return emit(event, payload)
}

export function backend_install(overrides: Partial<Backend> = {}): Backend {
    const backend: Backend = {
        autostart: false,
        calls: [],
        failures: new Map(),
        microphones: [],
        models: [],
        platform: { ...PLATFORM_X11 },
        service: service_ready(),
        settings: { ...SETTINGS_DEFAULT, api_host: HOST },
        status: { error: null, notice: null, phase: 'idle' },
        update: null,
        ...overrides,
    }

    clearMocks()

    mockIPC(
        (command, payload) => backend_handle(backend, command, payload),
        { shouldMockEvents: true },
    )

    return backend
}

export function calls_of(backend: Backend, command: string): Record<string, unknown>[] {
    return backend.calls.filter((call) => call.command === command).map((call) => call.args)
}

export function service_keyless(host = ''): ServiceStatus {
    const absent = { builtin: false, length: 0, user: false }

    return { host, host_builtin: false, keys: { llm: absent, transcription: absent } }
}

export function service_ready(): ServiceStatus {
    const saved = { builtin: false, length: 8, user: true }

    return { host: HOST, host_builtin: false, keys: { llm: saved, transcription: saved } }
}

function backend_handle(backend: Backend, command: string, payload?: InvokeArgs): unknown {
    const args = (payload ?? {}) as Record<string, unknown>

    backend.calls.push({ args, command })

    const failure = backend.failures.get(command)

    if (failure !== undefined) throw new Error(failure)

    return backend_reply(backend, command, args)
}

function backend_reply(backend: Backend, command: string, args: Record<string, unknown>): unknown {
    switch (command) {
        case 'autostart_get':
            return backend.autostart
        case 'autostart_set':
            backend.autostart = args['enabled'] as boolean

            return backend.autostart
        case 'microphones_list':
            return backend.microphones
        case 'models_list':
            return backend.models
        case 'platform_get':
            return { ...backend.platform }
        case 'plugin:app|version':
            return VERSION
        case 'plugin:updater|check':
            return backend.update
        case 'service_get':
            return structuredClone(backend.service)
        case 'service_key_set':
            return service_key_set(
                backend,
                args['purpose'] as KeyPurpose,
                args['api_key'] as string,
            )
        case 'settings_get':
            return { ...backend.settings }
        case 'settings_update':
            return settings_update(backend, args['settings'] as Settings)
        case 'status_get':
            return { ...backend.status }
        case 'typing_allow':
            backend.platform.typing_allowed = true

            return { ...backend.platform }
        default:
            return null
    }
}

function service_key_set(backend: Backend, purpose: KeyPurpose, api_key: string): ServiceStatus {
    const trimmed = api_key.trim()
    const builtin = backend.service.keys[purpose].builtin

    backend.service.keys[purpose] = { builtin, length: trimmed.length, user: trimmed !== '' }

    return structuredClone(backend.service)
}

function settings_update(backend: Backend, settings: Settings): Settings {
    backend.settings = { ...settings }
    backend.service.host = settings.api_host ?? ''

    return { ...backend.settings }
}
