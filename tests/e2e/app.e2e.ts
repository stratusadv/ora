import type { ChildProcess } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import {
    KEY_ENTER,
    click,
    count,
    driver_start,
    property,
    session_end,
    session_start,
    text,
    type,
    wait_text,
    wait_until,
} from './driver'
import type { Session } from './driver'
import { server_start } from './server'
import type { Server } from './server'


const APPLICATION = process.env['ORA_E2E_APPLICATION']
    ?? resolve(import.meta.dirname, '../../src-tauri/target/e2e/debug/ora')

const FILE_MODE_PRIVATE = 0o600
const KEY_LLM = 'notes-key'
const KEY_TRANSCRIPTION = 'audio-key'
const PHASE_CURRENT = 'h1.status-phase[data-current="true"]'

const scratch = mkdtempSync(join(tmpdir(), 'ora-e2e-'))
const root = join(scratch, 'data', 'com.stratusadv.ora')

let driver: ChildProcess
let server: Server
let session: Session


function file_mode(name: string): number {
    return statSync(join(root, name)).mode & 0o777
}

function file_text(name: string): string {
    return readFileSync(join(root, name), 'utf8')
}

function settings_stored(): Record<string, unknown> {
    return JSON.parse(file_text('settings.json')) as Record<string, unknown>
}

async function settings_wait(key: string, expected: unknown): Promise<void> {
    await wait_until(`settings.json to hold ${key} = ${JSON.stringify(expected)}`, () => {
        try {
            return Promise.resolve(settings_stored()[key] === expected)
        } catch {
            return Promise.resolve(false)
        }
    })
}

beforeAll(async () => {
    const environment: NodeJS.ProcessEnv = {
        ...process.env,
        GDK_BACKEND: 'x11',
        XDG_CONFIG_HOME: join(scratch, 'config'),
        XDG_DATA_HOME: join(scratch, 'data'),
    }

    delete environment['WAYLAND_DISPLAY']

    expect(environment['DISPLAY']).toBeTruthy()

    server = await server_start()
    driver = await driver_start(environment)
    session = await session_start(APPLICATION)
})

afterAll(async () => {
    await session_end(session)
    driver.kill()
    await server.close()
    rmSync(scratch, { force: true, recursive: true })
})


describe('ora on a first run', () => {
    it('stores the address and both keys, then shows the home screen', async () => {
        await wait_text(session, '.step-title', 'Welcome to ora')

        expect(await count(session, 'input.step-input[type="password"]')).toBe(2)

        await type(session, 'input.step-input[type="url"]', server.host)
        await type(session, 'label:nth-of-type(2) input.step-input', KEY_TRANSCRIPTION)
        await type(session, 'label:nth-of-type(3) input.step-input', KEY_LLM)
        await click(session, 'button[type="submit"]')
        await wait_text(session, PHASE_CURRENT, 'Ready')

        expect(settings_stored()['api_host']).toBe(server.host)
        expect(file_text('api_key.bin')).toBe(KEY_TRANSCRIPTION)
        expect(file_text('api_key_llm.bin')).toBe(KEY_LLM)
        expect(file_mode('api_key.bin')).toBe(FILE_MODE_PRIVATE)
        expect(file_mode('api_key_llm.bin')).toBe(FILE_MODE_PRIVATE)
    })
})

describe('ora settings', () => {
    it('lists the models the server offers, asking with the LLM key', async () => {
        await click(session, 'button[aria-label="Settings"]')
        await wait_until('the model list', async () => {
            return (await count(session, '#cleanup-model option')) === 2
        })

        const listed = server.requests.filter((request) => request.path === '/v1/models')

        expect(await count(session, '#cleanup-model option[value="stratus.thinking"]')).toBe(1)
        expect(await count(session, '#cleanup-model option[value="stratus.turbo"]')).toBe(1)
        expect(listed.map((request) => request.authorization)).toContain(`Bearer ${KEY_LLM}`)
        expect(await property(session, '#cleanup-thinking', 'disabled')).toBe(true)
    })

    it('stores a thinking level once a thinking model is chosen', async () => {
        await click(session, '#cleanup-model option[value="stratus.thinking"]')
        await settings_wait('cleanup_model', 'stratus.thinking')
        await wait_until('the thinking level to unlock', async () => {
            return (await property(session, '#cleanup-thinking', 'disabled')) === false
        })

        await click(session, '#cleanup-thinking option[value="xhigh"]')
        await settings_wait('cleanup_thinking', 'xhigh')
    })

    it('stores the shortcut mode and the vocabulary', async () => {
        await click(session, '#shortcut-mode option[value="tap"]')
        await settings_wait('shortcut_mode', 'tap')

        await type(session, '#vocabulary-term', `Kubernetes, Tauri, kubernetes${KEY_ENTER}`)
        await settings_wait('vocabulary', 'Kubernetes\nTauri')

        expect(await count(session, '.term')).toBe(2)
    })

    it('refuses an address that is not a web address and keeps the stored one', async () => {
        await type(session, '#service-host', `example.test${KEY_ENTER}`)
        await wait_until('the address to be refused', async () => {
            const messages = await text(session, '.settings').catch(() => '')

            return messages.includes('is not a web address')
        })

        expect(settings_stored()['api_host']).toBe(server.host)
        expect(await property(session, '#service-host', 'value')).toBe(server.host)
    })
})

describe('ora after a relaunch', () => {
    it('opens on the home screen with the keys and settings it stored', async () => {
        await session_end(session)

        session = await session_start(APPLICATION)

        await wait_text(session, PHASE_CURRENT, 'Ready')

        expect(await text(session, 'div.status-phase[data-current="true"]')).toContain(
            'to start dictating',
        )

        await click(session, 'button[aria-label="Settings"]')
        await wait_until('the settings page', async () => (await count(session, '.term')) === 2)

        expect(await property(session, '#shortcut-mode', 'value')).toBe('tap')
        expect(await property(session, '#cleanup-thinking', 'value')).toBe('xhigh')
    })
})
