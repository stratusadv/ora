import { invoke } from '@tauri-apps/api/core'
import type {
    KeyPurpose,
    Microphone,
    ModelInfo,
    Platform,
    ServiceStatus,
    Settings,
    Status,
} from '../types'


export const ipc = Object.freeze({
    app_quit(): Promise<void> {
        return invoke('app_quit')
    },
    app_relaunch(): Promise<void> {
        return invoke('app_relaunch')
    },
    autostart_get(): Promise<boolean> {
        return invoke<boolean>('autostart_get')
    },
    autostart_set(enabled: boolean): Promise<boolean> {
        return invoke<boolean>('autostart_set', { enabled })
    },
    dictation_toggle(): Promise<void> {
        return invoke('dictation_toggle')
    },
    microphones_list(): Promise<Microphone[]> {
        return invoke<Microphone[]>('microphones_list')
    },
    models_list(purpose: KeyPurpose): Promise<ModelInfo[]> {
        return invoke<ModelInfo[]>('models_list', { purpose })
    },
    monitor_close(): Promise<void> {
        return invoke('monitor_close')
    },
    monitor_open(): Promise<void> {
        return invoke('monitor_open')
    },
    platform_get(): Promise<Platform> {
        return invoke<Platform>('platform_get')
    },
    service_get(): Promise<ServiceStatus> {
        return invoke<ServiceStatus>('service_get')
    },
    service_key_set(purpose: KeyPurpose, api_key: string): Promise<ServiceStatus> {
        return invoke<ServiceStatus>('service_key_set', { api_key, purpose })
    },
    settings_get(): Promise<Settings> {
        return invoke<Settings>('settings_get')
    },
    settings_update(settings: Settings): Promise<Settings> {
        return invoke<Settings>('settings_update', { settings })
    },
    status_get(): Promise<Status> {
        return invoke<Status>('status_get')
    },
    typing_allow(): Promise<Platform> {
        return invoke<Platform>('typing_allow')
    },
})
