import { assert } from './assert'


export interface KeysPressed {
    alt: boolean
    code: string
    control: boolean
    meta: boolean
    shift: boolean
}

const PARTS_MAX = 5
const FUNCTION_KEY = /^F([1-9]|1\d|2[0-4])$/
const KEYS_SILENT: readonly string[] = Object.freeze(['Pause', 'ScrollLock'])
const MODIFIERS_GUARDING: readonly string[] = Object.freeze(['Alt', 'Control', 'Super'])

const MODIFIER_CODES: readonly string[] = Object.freeze([
    'AltLeft',
    'AltRight',
    'ControlLeft',
    'ControlRight',
    'MetaLeft',
    'MetaRight',
    'OSLeft',
    'OSRight',
    'ShiftLeft',
    'ShiftRight',
])

const KEY_NAMES: Readonly<Record<string, string>> = Object.freeze({
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    ArrowUp: 'Up',
    Backquote: '`',
    Backslash: '\\',
    BracketLeft: '[',
    BracketRight: ']',
    Comma: ',',
    Equal: '=',
    Minus: '-',
    Period: '.',
    Pause: 'Pause',
    Quote: "'",
    ScrollLock: 'Scroll Lock',
    Semicolon: ';',
    Slash: '/',
})


export function accelerator_from_keys(keys: KeysPressed): string | null {
    if (keys.code === '') return null
    if (MODIFIER_CODES.includes(keys.code)) return null

    const parts: string[] = []

    if (keys.control) parts.push('Control')
    if (keys.alt) parts.push('Alt')
    if (keys.shift) parts.push('Shift')
    if (keys.meta) parts.push('Super')

    parts.push(keys.code)

    assert(parts.length <= PARTS_MAX, `an accelerator has at most ${PARTS_MAX} parts`)

    return parts.join('+')
}

export function accelerator_guarded(accelerator: string): boolean {
    const parts = accelerator.split('+')
    const key = parts[parts.length - 1] ?? ''

    if (FUNCTION_KEY.test(key)) return true
    if (KEYS_SILENT.includes(key)) return true

    return parts.slice(0, -1).some((part) => MODIFIERS_GUARDING.includes(part))
}

export function accelerator_labels(accelerator: string, os: string): string[] {
    const parts = accelerator.split('+').filter((part) => part !== '')

    assert(parts.length <= PARTS_MAX, `${accelerator} has more parts than an accelerator holds`)

    return parts.map((part) => part_label(part, os))
}

function part_label(part: string, os: string): string {
    if (part === 'Control') return 'Ctrl'
    if (part === 'Super') return os === 'windows' ? 'Win' : 'Super'
    if (part.startsWith('Key')) return part.slice('Key'.length)
    if (part.startsWith('Digit')) return part.slice('Digit'.length)
    if (part.startsWith('Numpad')) return `Num ${part.slice('Numpad'.length)}`

    return KEY_NAMES[part] ?? part
}
