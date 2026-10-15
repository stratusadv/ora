<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import goat_mark from '../src-tauri/icons/128x128@2x.png'
import SettingsPage from './components/SettingsPage.vue'
import SetupStep from './components/SetupStep.vue'
import StatusPanel from './components/StatusPanel.vue'
import { use_runtime } from './composables/use_runtime'
import { use_service } from './composables/use_service'
import { use_settings } from './composables/use_settings'
import { use_update } from './composables/use_update'
import { accelerator_labels } from './lib/accelerator'
import { step_of } from './lib/step'
import { dismissed_prune } from './lib/toast'


type View = 'home' | 'settings'

interface Toast {
    kind: 'error' | 'update' | 'warning'
    text: string
}

const ICON_SETTINGS = 'M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 '
    + '1.281c.063.374.313.686.65.83a6.748 6.748 0 0 1 1.124.65c.317.234.715.298 1.067.156l1.213-'
    + '.485a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241'
    + '-.438.613-.43.992.014.418.014.838 0 1.255-.008.378.137.75.43.991l1.004.828a1.125 1.125 0 0 '
    + '1 .26 1.431l-1.298 2.247a1.125 1.125 0 0 1-1.37.491l-1.213-.486c-.352-.141-.75-.077-1.066'
    + '.158a6.81 6.81 0 0 1-1.125.65c-.337.144-.587.456-.65.83l-.213 1.281c-.09.543-.56.94-1.11.94'
    + 'h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.65-.83a6.749 6.749 0 0 '
    + '1-1.125-.65c-.317-.235-.715-.299-1.067-.158l-1.213.486a1.125 1.125 0 0 1-1.369-.49l-1.297-'
    + '2.247a1.125 1.125 0 0 1 .26-1.432l1.004-.827c.292-.241.437-.613.43-.992a7.07 7.07 0 0 1 0-'
    + '1.255c.007-.378-.138-.75-.43-.991l-1.004-.828a1.125 1.125 0 0 1-.26-1.431l1.297-2.247a1.125 '
    + '1.125 0 0 1 1.37-.491l1.212.486c.352.142.75.078 1.067-.157.347-.26.722-.479 1.125-.65.337-'
    + '.144.587-.456.65-.83l.214-1.28Z'

const ICON_CLOSE = 'M6 6l12 12M18 6L6 18'
const ICON_SETTINGS_HUB = 'M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z'
const UPDATE_TEXT = 'An update is available.'

const { platform, platform_error, runtime_start, status } = use_runtime()
const { service, service_refresh } = use_service()
const { settings, settings_error, settings_load } = use_settings()

const {
    phase: update_phase,
    update_check_startup,
    update_error,
    update_install,
} = use_update()

const dismissed = ref<ReadonlySet<string>>(new Set())
const entrance = ref(true)
const ready = ref(false)
const view = ref<View>('home')

const toasts_all = computed<Toast[]>(() => {
    const errors = [
        platform_error.value,
        settings_error.value,
        update_error.value,
        status.value.error,
    ]

    const unique = [...new Set(errors.filter((error) => error !== null))]
    const shown: Toast[] = unique.map((text) => ({ kind: 'error', text }))

    if (status.value.notice !== null) shown.push({ kind: 'warning', text: status.value.notice })
    if (update_phase.value === 'available') shown.unshift({ kind: 'update', text: UPDATE_TEXT })

    return shown
})

const toasts = computed(() => toasts_all.value.filter((toast) => !dismissed.value.has(toast.text)))

const os = computed(() => platform.value?.os ?? 'linux')
const paste_manual = computed(() => platform.value?.keystroke === 'unavailable')
const step = computed(() => step_of(platform.value, service.value))
const screen = computed(() => view.value === 'settings' ? 'settings' : step.value ?? 'home')

const shortcut_labels = computed(() => {
    if (settings.value === null) return []

    return accelerator_labels(settings.value.shortcut, os.value)
})


function toast_dismiss(text: string) {
    dismissed.value = new Set([...dismissed.value, text])
}

function view_toggle() {
    view.value = view.value === 'settings' ? 'home' : 'settings'
}

watch(screen, () => {
    if (ready.value) entrance.value = false
})

watch(toasts_all, (all) => {
    dismissed.value = dismissed_prune(dismissed.value, all.map((toast) => toast.text))
})

onMounted(async () => {
    try {
        await Promise.all([
            settings_load(),
            runtime_start(),
            service_refresh(),
        ])
    } finally {
        ready.value = true
    }

    await update_check_startup()
})
</script>

<template>
    <div class="shell">
        <header class="bar">
            <button type="button" class="brand" title="Home" @click="view = 'home'">
                <img :src="goat_mark" alt="" />
                <span>ora</span>
            </button>
            <button
                type="button"
                class="icon-btn"
                title="Settings"
                aria-label="Settings"
                :aria-pressed="view === 'settings'"
                @click="view_toggle"
            >
                <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                >
                    <path :d="ICON_SETTINGS" />
                    <path :d="ICON_SETTINGS_HUB" />
                </svg>
            </button>
        </header>

        <TransitionGroup tag="div" name="toast" class="toasts" aria-live="polite">
            <div
                v-for="toast in toasts"
                :key="toast.text"
                class="toast"
                :data-kind="toast.kind"
                role="status"
            >
                <span class="toast-dot" />
                <p class="toast-text">{{ toast.text }}</p>
                <button
                    v-if="toast.kind === 'update'"
                    type="button"
                    class="btn-primary toast-action"
                    @click="update_install"
                >
                    Install
                </button>
                <button
                    type="button"
                    class="toast-close"
                    title="Close"
                    aria-label="Close"
                    @click="toast_dismiss(toast.text)"
                >
                    <svg
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        aria-hidden="true"
                    >
                        <path :d="ICON_CLOSE" />
                    </svg>
                </button>
            </div>
        </TransitionGroup>

        <main class="main">
            <Transition name="screen" mode="out-in">
                <div
                    v-if="ready"
                    :key="screen"
                    class="content"
                    :data-entrance="entrance"
                    :data-setup="view === 'home' && step !== null"
                    :data-view="view"
                >
                    <SettingsPage
                        v-if="view === 'settings'"
                        :paste_manual="paste_manual"
                    />
                    <SetupStep
                        v-else-if="step !== null"
                        :platform="platform"
                        :step="step"
                    />
                    <StatusPanel
                        v-else
                        :paste_manual="paste_manual"
                        :shortcut="shortcut_labels"
                        :status="status"
                    />
                </div>
            </Transition>
        </main>
    </div>
</template>

<style scoped>
.shell {
    position: relative;
    height: 100%;
}

.bar {
    position: absolute;
    inset: 0 0 auto;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    height: 3.5rem;
    padding: 0 1.125rem 0 1.75rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-header);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    user-select: none;
    animation: entrance-fade 520ms ease backwards;
}

.brand {
    display: inline-flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-content-strong);
    font-size: 1.375rem;
    font-weight: 600;
    letter-spacing: -0.03em;
    line-height: 1;
}

.brand img {
    width: 2.25rem;
    height: 2.25rem;
}

.toasts {
    position: absolute;
    inset: 4.25rem 1.25rem auto;
    z-index: 20;
    max-width: 31.5rem;
    margin: 0 auto;
    pointer-events: none;
}

.toasts > * {
    pointer-events: auto;
}

.toasts > * + * {
    margin-top: 0.5rem;
}

.toast {
    --tone: var(--color-accent);
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-height: 3.25rem;
    padding: 0.625rem 1rem;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-lg);
    background: var(--color-float);
    box-shadow: var(--edge-lit), var(--shadow-float);
    color: var(--color-content-strong);
}

.toast[data-kind="error"] {
    --tone: var(--color-listening);
}

.toast[data-kind="warning"] {
    --tone: var(--color-warning-content);
}

.toast-dot {
    flex: none;
    width: 0.4375rem;
    height: 0.4375rem;
    border-radius: 50%;
    background: var(--tone);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--tone) 20%, transparent);
}

.toast-text {
    flex: 1;
    min-width: 0;
    font-size: 0.875rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
}

.toast-action {
    flex: none;
    height: 2rem;
    padding: 0 0.875rem;
    font-size: 0.875rem;
}

.toast-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 1.75rem;
    height: 1.75rem;
    margin-right: -0.375rem;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-content-meta);
    transition: background 120ms ease, color 120ms ease;
}

.toast-close:hover {
    background: var(--color-raised);
    color: var(--color-content-strong);
}

.toast-close svg {
    width: 0.875rem;
    height: 0.875rem;
}

.toast-enter-active,
.toast-leave-active,
.toast-move {
    transition:
        opacity 220ms ease,
        transform 280ms cubic-bezier(0.2, 0.9, 0.3, 1);
}

.toast-enter-from,
.toast-leave-to {
    opacity: 0;
    transform: translateY(-1rem);
}

.toast-leave-active {
    position: absolute;
    inset: auto 0;
}

@media (prefers-reduced-motion: reduce) {
    .toast-enter-from,
    .toast-leave-to {
        transform: none;
    }
}

.screen-enter-active {
    transition: opacity 260ms ease;
}

.screen-leave-active {
    transition: opacity 160ms ease;
}

.screen-enter-from,
.screen-leave-to {
    opacity: 0;
}

.content[data-entrance="true"] :deep(.step-art),
.content[data-entrance="true"] :deep(.status-hero) {
    animation: entrance-fade 900ms ease 80ms backwards;
}

.content[data-entrance="true"] :deep(.step-body > *),
.content[data-entrance="true"] :deep(.status-footer) {
    --entrance-delay: 200ms;
    will-change: transform;
    animation: entrance-rise 760ms cubic-bezier(0.16, 1, 0.3, 1) var(--entrance-delay) backwards;
}

.content[data-entrance="true"] :deep(.step-body > :nth-child(2)) {
    --entrance-delay: 280ms;
}

.content[data-entrance="true"] :deep(.step-body > :nth-child(3)) {
    --entrance-delay: 360ms;
    --entrance-rise: 0;
}

.content[data-entrance="true"] :deep(.step-body > :nth-child(n + 4)) {
    --entrance-delay: 440ms;
    --entrance-rise: 0;
}

@keyframes entrance-fade {
    from {
        opacity: 0;
    }
}

@keyframes entrance-rise {
    from {
        opacity: 0;
        transform: translateY(calc(var(--entrance-rise, 1) * 12px));
    }
}

@media (prefers-reduced-motion: reduce) {
    .bar,
    .content[data-entrance="true"] :deep(.step-art),
    .content[data-entrance="true"] :deep(.status-hero),
    .content[data-entrance="true"] :deep(.step-body > *),
    .content[data-entrance="true"] :deep(.status-footer) {
        animation: none;
    }

    .screen-enter-active,
    .screen-leave-active {
        transition: none;
    }
}

.main {
    height: 100%;
    overflow-x: hidden;
    overflow-y: auto;
}

.content {
    display: grid;
    align-content: start;
    gap: 1.25rem;
    max-width: 34rem;
    min-height: 100%;
    margin: 0 auto;
    padding: 5rem 1.25rem 2.5rem;
}

.content[data-view="home"] {
    display: flex;
    flex-direction: column;
    justify-content: center;
    max-width: none;
    padding: 4.5rem 1.75rem 1.75rem;
}

.content[data-setup="true"] {
    padding: 0;
}
</style>
