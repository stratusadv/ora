import { describe, expect, it } from 'vitest'
import { error_message } from '../../src/lib/errors'


describe('error_message', () => {
    it('drops the category the backend puts in front of every error', () => {
        expect(error_message('config error: no API key has been set.')).toBe(
            'No API key has been set.',
        )

        expect(error_message('api error: 401 Transcription failed: bad key')).toBe(
            '401 Transcription failed: bad key',
        )
    })

    it('reads thrown errors and names anything else by its type', () => {
        expect(error_message(new Error('network error: offline'))).toBe('Offline')
        expect(error_message({ code: 7 })).toBe('An unexpected object value')
        expect(error_message(undefined)).toBe('An unexpected undefined value')
        expect(error_message('   ')).toBe('The operation failed without an error message.')
    })
})
