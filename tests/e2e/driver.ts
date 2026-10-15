import { spawn } from 'node:child_process'
import type { ChildProcess } from 'node:child_process'


export interface Session {
    id: string
}

const DRIVER_PORT = 4444
const DRIVER_URL = `http://127.0.0.1:${DRIVER_PORT}`
const ELEMENT_KEY = 'element-6066-11e4-a52e-4f735466cecf'
const POLL_MS = 100
const WAIT_MS_MAX = 15_000

export const KEY_ENTER = ''


export async function driver_start(environment: NodeJS.ProcessEnv): Promise<ChildProcess> {
    const driver = spawn('tauri-driver', ['--port', String(DRIVER_PORT)], {
        env: environment,
        stdio: 'inherit',
    })

    await wait_until('tauri-driver to listen', async () => {
        try {
            return (await fetch(`${DRIVER_URL}/status`)).ok
        } catch {
            return false
        }
    })

    return driver
}

export async function session_start(application: string): Promise<Session> {
    const capabilities = { alwaysMatch: { 'tauri:options': { application } } }
    const reply = await command<{ sessionId: string }>('POST', '/session', { capabilities })

    return { id: reply.sessionId }
}

export async function session_end(session: Session): Promise<void> {
    await command('DELETE', `/session/${session.id}`)
}

export async function click(session: Session, selector: string): Promise<void> {
    const element = await element_find(session, selector)
    // An option has no box of its own in WebKit, so its select is what has to come into view.
    const script = '(arguments[0].closest("select") ?? arguments[0]).scrollIntoView({ block: "center" })'
    const args = [{ [ELEMENT_KEY]: element }]

    await command('POST', `/session/${session.id}/execute/sync`, { args, script })
    await command('POST', `/session/${session.id}/element/${element}/click`, {})
}

export async function count(session: Session, selector: string): Promise<number> {
    const body = { using: 'css selector', value: selector }
    const found = await command<unknown[]>('POST', `/session/${session.id}/elements`, body)

    return found.length
}

export async function property(session: Session, selector: string, name: string): Promise<unknown> {
    const element = await element_find(session, selector)

    return command('GET', `/session/${session.id}/element/${element}/property/${name}`)
}

export async function text(session: Session, selector: string): Promise<string> {
    const element = await element_find(session, selector)

    return command<string>('GET', `/session/${session.id}/element/${element}/text`)
}

export async function type(session: Session, selector: string, keys: string): Promise<void> {
    const element = await element_find(session, selector)
    const path = `/session/${session.id}/element/${element}`

    await command('POST', `${path}/clear`, {})
    await command('POST', `${path}/value`, { text: keys })
}

export async function wait_until(what: string, check: () => Promise<boolean>): Promise<void> {
    const deadline = Date.now() + WAIT_MS_MAX

    while (Date.now() < deadline) {
        if (await check()) return

        await new Promise((resolve) => setTimeout(resolve, POLL_MS))
    }

    throw new Error(`gave up after ${WAIT_MS_MAX} ms waiting for ${what}`)
}

export async function wait_text(
    session: Session,
    selector: string,
    expected: string,
): Promise<void> {
    await wait_until(`${selector} to read "${expected}"`, async () => {
        try {
            return (await text(session, selector)) === expected
        } catch {
            return false
        }
    })
}

async function command<Value>(method: string, path: string, body?: unknown): Promise<Value> {
    const response = await fetch(`${DRIVER_URL}${path}`, {
        body: body === undefined ? null : JSON.stringify(body),
        headers: { 'content-type': 'application/json' },
        method,
    })

    const reply = (await response.json()) as { value: Value }

    if (!response.ok) throw new Error(`${method} ${path} failed: ${JSON.stringify(reply.value)}`)

    return reply.value
}

async function element_find(session: Session, selector: string): Promise<string> {
    const body = { using: 'css selector', value: selector }
    const path = `/session/${session.id}/element`
    const found = await command<Record<string, string>>('POST', path, body)
    const element = found[ELEMENT_KEY]

    if (element === undefined) throw new Error(`${selector} matched no element`)

    return element
}
