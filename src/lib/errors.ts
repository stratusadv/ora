const CATEGORY_PREFIX = /^[a-z]+ error: /


export function error_message(error: unknown): string {
    const raw = error_text(error).trim()
    const message = raw.replace(CATEGORY_PREFIX, '')

    if (message === '') return 'The operation failed without an error message.'

    return message.charAt(0).toUpperCase() + message.slice(1)
}

function error_text(error: unknown): string {
    if (typeof error === 'string') return error
    if (error instanceof Error) return error.message

    return `an unexpected ${typeof error} value`
}
