import type { KeyPurpose, ServiceStatus } from '../types'


export const KEY_PURPOSES: readonly KeyPurpose[] = Object.freeze(['transcription', 'llm'])

export const KEY_LABELS: Readonly<Record<KeyPurpose, string>> = Object.freeze({
    llm: 'LLM key',
    transcription: 'Transcription key',
})

export const KEY_NAMES: Readonly<Record<KeyPurpose, string>> = Object.freeze({
    llm: 'LLM key',
    transcription: 'transcription key',
})


export function key_present(service: ServiceStatus, purpose: KeyPurpose): boolean {
    const source = service.keys[purpose]

    return source.builtin || source.user
}

export function keys_missing(service: ServiceStatus): KeyPurpose[] {
    return KEY_PURPOSES.filter((purpose) => !key_present(service, purpose))
}
