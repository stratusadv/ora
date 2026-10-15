import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'


export interface Request {
    authorization: string
    method: string
    path: string
}

export interface Server {
    close(): Promise<void>
    host: string
    requests: Request[]
}

const REPLIES: Readonly<Record<string, unknown>> = Object.freeze({
    'GET /v1/models': {
        data: [
            { id: 'stratus.turbo' },
            { id: 'stratus.listen' },
            { id: 'stratus.thinking' },
        ],
    },
    'HEAD /v1/models': {},
})


export async function server_start(): Promise<Server> {
    const requests: Request[] = []

    const server = createServer((request, response) => {
        const method = request.method ?? ''
        const path = request.url ?? ''
        const reply = REPLIES[`${method} ${path}`]

        requests.push({ authorization: request.headers.authorization ?? '', method, path })
        request.resume()

        if (reply === undefined) {
            response.writeHead(404).end()

            return
        }

        response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(reply))
    })

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))

    const { port } = server.address() as AddressInfo

    return {
        close: () => new Promise((resolve) => server.close(() => { resolve() })),
        host: `http://127.0.0.1:${port}`,
        requests,
    }
}
