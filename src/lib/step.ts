import { keys_missing } from './key'
import type { Platform, ServiceStatus } from '../types'


export type Step = 'key' | 'shortcut' | 'typing'


export function step_of(platform: Platform | null, service: ServiceStatus | null): Step | null {
    if (service !== null) {
        if (service.host === '') return 'key'
        if (keys_missing(service).length > 0) return 'key'
    }

    if (platform === null) return null
    if (!platform.shortcut_bound) return 'shortcut'
    if (platform.keystroke !== 'portal') return null
    if (platform.typing_allowed) return null

    return 'typing'
}
