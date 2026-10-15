import { describe, expect, it } from 'vitest'
import {
    accelerator_from_keys,
    accelerator_guarded,
    accelerator_labels,
} from '../../src/lib/accelerator'


const KEYS_NONE = { alt: false, code: '', control: false, meta: false, shift: false }

const GUARD_CASES: readonly (readonly [string, boolean])[] = [
    ['Control+Shift+Space', true],
    ['Control+Alt+KeyD', true],
    ['Super+F9', true],
    ['Alt+Space', true],
    ['F8', true],
    ['Shift+F24', true],
    ['KeyA', false],
    ['Shift+KeyA', false],
    ['Space', false],
    ['Shift+Digit1', false],
    ['ScrollLock', true],
    ['Pause', true],
    ['CapsLock', false],
    ['Insert', false],
    ['PrintScreen', false],
]


describe('accelerator_from_keys', () => {
    it('ignores a press of a modifier on its own', () => {
        expect(accelerator_from_keys({ ...KEYS_NONE, code: 'ControlLeft', control: true })).toBeNull()
        expect(accelerator_from_keys({ ...KEYS_NONE, code: 'ShiftRight', shift: true })).toBeNull()
        expect(accelerator_from_keys({ ...KEYS_NONE, code: 'MetaLeft', meta: true })).toBeNull()
        expect(accelerator_from_keys(KEYS_NONE)).toBeNull()
    })

    it('orders modifiers the way the shortcut service parses them', () => {
        const keys = { alt: true, code: 'KeyD', control: true, meta: true, shift: true }

        expect(accelerator_from_keys(keys)).toBe('Control+Alt+Shift+Super+KeyD')
        expect(accelerator_from_keys({ ...KEYS_NONE, code: 'F8' })).toBe('F8')
    })
})

describe('accelerator_guarded', () => {
    it.each(GUARD_CASES)('%s is guarded: %s', (accelerator, guarded) => {
        expect(accelerator_guarded(accelerator)).toBe(guarded)
    })

    it('treats only F1 through F24 as function keys', () => {
        expect(accelerator_guarded('F25')).toBe(false)
        expect(accelerator_guarded('F0')).toBe(false)
        expect(accelerator_guarded('KeyF')).toBe(false)
    })
})

describe('accelerator_labels', () => {
    it('names keys the way a keyboard prints them', () => {
        expect(accelerator_labels('Control+Shift+Space', 'linux')).toEqual(['Ctrl', 'Shift', 'Space'])
        expect(accelerator_labels('Super+KeyD', 'windows')).toEqual(['Win', 'D'])
        expect(accelerator_labels('Super+Digit1', 'linux')).toEqual(['Super', '1'])
        expect(accelerator_labels('Alt+Backquote', 'linux')).toEqual(['Alt', '`'])
        expect(accelerator_labels('Control+Numpad5', 'linux')).toEqual(['Ctrl', 'Num 5'])
    })

    it('round trips every recorded accelerator into one label per part', () => {
        for (const [accelerator] of GUARD_CASES) {
            expect(accelerator_labels(accelerator, 'linux')).toHaveLength(accelerator.split('+').length)
        }
    })
})
