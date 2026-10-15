import { describe, expect, it } from 'vitest'
import { dismissed_prune } from '../../src/lib/toast'


const NOTICE = 'Nothing was heard. Check that the microphone is on and not muted.'


describe('dismissed_prune', () => {
    it('keeps a closed banner closed while its message is still showing', () => {
        const dismissed = dismissed_prune(new Set([NOTICE]), [NOTICE])

        expect(dismissed.has(NOTICE)).toBe(true)
    })

    it('shows a message again once it has cleared and come back', () => {
        const cleared = dismissed_prune(new Set([NOTICE]), [])
        const returned = dismissed_prune(cleared, [NOTICE])

        expect(returned.has(NOTICE)).toBe(false)
    })

    it('never holds more than what is showing', () => {
        const dismissed = dismissed_prune(new Set(['a', 'b', 'c']), ['b', 'd'])

        expect([...dismissed]).toEqual(['b'])
    })
})
