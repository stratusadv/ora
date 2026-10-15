import { describe, expect, it } from 'vitest'
import { terms_fresh, terms_of } from '../../src/lib/vocabulary'


const TERMS: readonly string[] = Object.freeze(['Stratus', 'Quartz Lantern', 'ora', 'café'])


describe('terms_of', () => {
    it('reads back the terms it was given once they are stored a line each', () => {
        expect(terms_of(TERMS.join('\n'))).toEqual(TERMS)
    })

    it('splits on commas and lines and drops blanks and surrounding space', () => {
        expect(terms_of(' Stratus ,\n\n ora,, Quartz Lantern \n')).toEqual([
            'Stratus',
            'ora',
            'Quartz Lantern',
        ])

        expect(terms_of('  \n , ')).toEqual([])
    })
})

describe('terms_fresh', () => {
    it('keeps only terms the list lacks, ignoring case, and each one once', () => {
        expect(terms_fresh(TERMS, 'STRATUS, Kubernetes, kubernetes, Tauri')).toEqual([
            'Kubernetes',
            'Tauri',
        ])
    })

    it('adds nothing for a draft that is blank or already known', () => {
        expect(terms_fresh(TERMS, ' , \n ')).toEqual([])
        expect(terms_fresh(TERMS, 'ora, Café')).toEqual([])
    })

    it('stores to a vocabulary that reads back as the old terms then the new ones', () => {
        const fresh = terms_fresh(TERMS, 'Tauri, Vite')
        const stored = [...TERMS, ...fresh].join('\n')

        expect(terms_of(stored)).toEqual([...TERMS, 'Tauri', 'Vite'])
    })
})
