import { describe, expect, it } from 'vitest'
import { keys_missing } from '../../src/lib/key'
import { step_of } from '../../src/lib/step'
import type { KeyPurpose, KeySource, Platform, ServiceStatus } from '../../src/types'


const PLATFORM_READY: Platform = Object.freeze({
    desktop: 'GNOME',
    keystroke: 'portal',
    os: 'linux',
    shortcut: 'keyboard',
    shortcut_bound: true,
    shortcut_error: null,
    typing_allowed: true,
})

const KEY_ABSENT: KeySource = Object.freeze({ builtin: false, length: 0, user: false })
const KEY_BUILTIN: KeySource = Object.freeze({ builtin: true, length: 40, user: false })
const KEY_SAVED: KeySource = Object.freeze({ builtin: false, length: 40, user: true })

const SERVICE_READY: ServiceStatus = Object.freeze({
    host: 'https://api.example.test',
    host_builtin: true,
    keys: { llm: KEY_SAVED, transcription: KEY_SAVED },
})


function service_with(keys: Partial<Record<KeyPurpose, KeySource>>): ServiceStatus {
    return { ...SERVICE_READY, keys: { ...SERVICE_READY.keys, ...keys } }
}


describe('step_of', () => {
    it('shows no step once the key, the shortcut, and typing are all in place', () => {
        const native = { ...PLATFORM_READY, keystroke: 'native' as const, typing_allowed: false }

        expect(step_of(PLATFORM_READY, SERVICE_READY)).toBeNull()
        expect(step_of(native, SERVICE_READY)).toBeNull()
    })

    it('asks for the key first, whatever else is missing', () => {
        const unbound = { ...PLATFORM_READY, shortcut_bound: false, typing_allowed: false }

        const keyless = service_with({ llm: KEY_ABSENT, transcription: KEY_ABSENT })

        expect(step_of(unbound, keyless)).toBe('key')
        expect(step_of(unbound, { ...SERVICE_READY, host: '' })).toBe('key')
        expect(step_of(null, keyless)).toBe('key')
    })

    it('asks for the keys while either one is missing', () => {
        expect(step_of(PLATFORM_READY, service_with({ llm: KEY_ABSENT }))).toBe('key')
        expect(step_of(PLATFORM_READY, service_with({ transcription: KEY_ABSENT }))).toBe('key')
    })

    it('accepts a built-in key in place of a saved one', () => {
        const builtin = service_with({ llm: KEY_BUILTIN, transcription: KEY_BUILTIN })

        expect(step_of(PLATFORM_READY, builtin)).toBeNull()
    })

    it('asks for the shortcut before typing permission', () => {
        const unbound = { ...PLATFORM_READY, shortcut_bound: false, typing_allowed: false }

        expect(step_of(unbound, SERVICE_READY)).toBe('shortcut')
        expect(step_of({ ...PLATFORM_READY, typing_allowed: false }, SERVICE_READY)).toBe('typing')
    })

    it('waits without a step until the platform has been read', () => {
        expect(step_of(null, SERVICE_READY)).toBeNull()
        expect(step_of(null, null)).toBeNull()
    })
})

describe('keys_missing', () => {
    it('lists each missing key once, transcription first, and nothing once both are set', () => {
        const keyless = service_with({ llm: KEY_ABSENT, transcription: KEY_ABSENT })

        expect(keys_missing(keyless)).toEqual(['transcription', 'llm'])
        expect(keys_missing(service_with({ llm: KEY_ABSENT }))).toEqual(['llm'])
        expect(keys_missing(service_with({ llm: KEY_BUILTIN }))).toEqual([])
        expect(keys_missing(SERVICE_READY)).toEqual([])
    })
})
