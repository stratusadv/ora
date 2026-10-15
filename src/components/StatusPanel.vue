<script setup lang="ts">
import { computed } from 'vue'
import OraOrb from './OraOrb.vue'
import { use_settings } from '../composables/use_settings'
import { ipc } from '../lib/ipc'
import type { Phase, ShortcutMode, Status } from '../types'


interface Hint {
    after: string
    before: string
}

type HintTable = Record<Exclude<Phase, 'transcribing'>, Record<ShortcutMode, Hint>>

const props = defineProps<{
    paste_manual: boolean
    shortcut: string[]
    status: Status
}>()

const PHASES: readonly Phase[] = Object.freeze(['idle', 'listening', 'transcribing'])

const HINTS: Readonly<HintTable> = Object.freeze({
    idle: {
        hold: { after: 'to dictate', before: 'Hold' },
        hold_or_tap: { after: 'to dictate', before: 'Hold or press' },
        tap: { after: 'to start dictating', before: 'Press' },
    },
    listening: {
        hold: { after: 'to stop', before: 'Release' },
        hold_or_tap: { after: 'to stop', before: 'Release or press' },
        tap: { after: 'again to stop', before: 'Press' },
    },
})

const TITLES: Readonly<Record<Phase, string>> = Object.freeze({
    idle: 'Ready',
    listening: 'Recording',
    transcribing: 'Transcribing',
})

const { settings } = use_settings()

const mode = computed<ShortcutMode>(() => settings.value?.shortcut_mode ?? 'hold_or_tap')

const orb_label = computed(() => {
    return props.status.phase === 'listening' ? 'Stop recording' : 'Start recording'
})


async function orb_click() {
    try {
        await ipc.dictation_toggle()
    } catch (error) {
        console.warn('[ora] the orb click did not reach dictation:', error)
    }
}
</script>

<template>
    <section class="status" :data-phase="status.phase" aria-live="polite">
        <div class="status-hero">
            <button
                type="button"
                class="status-orb"
                :aria-label="orb_label"
                :title="orb_label"
                @click="orb_click"
            >
                <OraOrb :phase="status.phase" />
            </button>
        </div>

        <footer class="status-footer">
            <div class="status-stage">
                <h1
                    v-for="phase in PHASES"
                    :key="phase"
                    class="status-phase status-title"
                    :data-current="phase === status.phase"
                    :data-phase="phase"
                >
                    {{ TITLES[phase] }}
                </h1>
            </div>

            <div class="status-stage">
                <div
                    v-for="phase in PHASES"
                    :key="phase"
                    class="status-phase"
                    :data-current="phase === status.phase"
                >
                    <p v-if="phase === 'transcribing'" class="status-hint">
                        Converting the recording to text.
                    </p>
                    <p v-else class="status-hint">
                        {{ HINTS[phase][mode].before }}
                        <span class="status-keys">
                            <kbd v-for="key in shortcut" :key="key">{{ key }}</kbd>
                        </span>
                        {{ HINTS[phase][mode].after }}
                    </p>
                    <p v-if="phase === 'idle' && paste_manual" class="status-note">
                        Paste the result with <kbd>Ctrl</kbd> <kbd>V</kbd>.
                    </p>
                </div>
            </div>
        </footer>
    </section>
</template>

<style scoped>
.status {
    flex: 1;
    display: grid;
    grid-template-rows: 1fr auto;
    gap: 1.5rem;
}

.status-hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-content: center;
    justify-items: center;
    gap: 0;
}

.status-orb {
    justify-self: start;
    width: 100vw;
    margin: 0 0 0 calc(50% - 50vw);
    padding: 0;
    border: 0;
    background: none;
}

.status-orb:focus-visible {
    outline-offset: -2px;
}

.status-orb :deep(.orb) {
    margin-bottom: 0;
}

.status-stage {
    display: grid;
    width: 100%;
}

.status-phase {
    grid-area: 1 / 1;
    display: grid;
    align-content: end;
    justify-items: start;
    gap: 0.25rem;
    opacity: 0;
    visibility: hidden;
    transition:
        opacity 140ms ease-out,
        visibility 0s linear 140ms;
}

.status-phase[data-current="true"] {
    opacity: 1;
    visibility: visible;
    transition:
        opacity 260ms ease-in-out 120ms,
        visibility 0s linear 0s;
}

.status-title {
    --sweep-base: var(--color-content);
    --sweep-light: var(--color-sweep);
    --sweep-seconds: 4.5s;
    padding-bottom: 0.08em;
    background-image: linear-gradient(
        90deg,
        var(--sweep-base) 0%,
        var(--sweep-base) 35%,
        var(--sweep-light) 50%,
        var(--sweep-base) 65%,
        var(--sweep-base) 100%
    );
    background-repeat: repeat-x;
    background-size: 200% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    font-size: 2.75rem;
    font-weight: 500;
    letter-spacing: -0.045em;
    line-height: 1.05;
    animation: title-sweep var(--sweep-seconds) ease-in-out infinite paused;
}

.status-title[data-current="true"] {
    animation-play-state: running;
}

.status-title[data-phase="listening"] {
    --sweep-base: var(--color-listening);
    --sweep-light: var(--color-sweep-listening);
    --sweep-seconds: 3.2s;
}

.status-title[data-phase="transcribing"] {
    --sweep-base: var(--color-transcribing);
    --sweep-light: var(--color-sweep-transcribing);
    --sweep-seconds: 2.6s;
}

.status-hint {
    max-width: 28rem;
    color: var(--color-content-muted);
    font-size: 0.875rem;
    line-height: 2rem;
}

.status-keys {
    display: inline-flex;
    gap: 0.1875rem;
    margin: 0 0.25rem;
    vertical-align: middle;
}

.status-keys kbd {
    min-width: 1.5rem;
    height: 1.5rem;
    margin: 0;
    padding: 0 0.4375rem;
    font-size: 0.75rem;
}

.status-note {
    color: var(--color-content-meta);
    font-size: 0.8125rem;
    line-height: 1.75rem;
}

.status-footer {
    display: grid;
    gap: 0.375rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--color-border);
}

@keyframes title-sweep {
    from {
        background-position: 200% 0;
    }

    to {
        background-position: 0% 0;
    }
}

@media (prefers-reduced-motion: reduce) {
    .status-phase,
    .status-phase[data-current="true"] {
        transition: none;
    }

    .status-title {
        animation: none;
    }
}
</style>
