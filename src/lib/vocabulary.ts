const TERM_SEPARATORS = /[\n,]/


export function terms_fresh(terms: readonly string[], draft: string): string[] {
    const known = new Set(terms.map((term) => term.toLowerCase()))
    const fresh: string[] = []

    for (const term of terms_of(draft)) {
        if (known.has(term.toLowerCase())) continue

        known.add(term.toLowerCase())
        fresh.push(term)
    }

    return fresh
}

export function terms_of(vocabulary: string): string[] {
    return vocabulary
        .split(TERM_SEPARATORS)
        .map((term) => term.trim())
        .filter((term) => term !== '')
}
