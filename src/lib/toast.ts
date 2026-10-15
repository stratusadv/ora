export function dismissed_prune(
    dismissed: ReadonlySet<string>,
    shown: readonly string[],
): Set<string> {
    return new Set(shown.filter((text) => dismissed.has(text)))
}
