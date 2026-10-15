import { assert } from './assert'


export function control_restore(event: Event, value: boolean | string | null | undefined): void {
    const control = event.target

    if (control instanceof HTMLInputElement) {
        assert(control.type === 'checkbox', 'only a switch among the inputs is restored')

        control.checked = value === true

        return
    }

    assert(control instanceof HTMLSelectElement, 'only a switch or a select is restored')
    assert(typeof value !== 'boolean', 'a select holds text, never a boolean')

    control.value = value ?? ''
}
