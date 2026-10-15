<script setup lang="ts">
import { computed, ref, useTemplateRef } from 'vue'
import InfoTip from './InfoTip.vue'
import { use_settings } from '../composables/use_settings'
import { terms_fresh, terms_of } from '../lib/vocabulary'


const ICON_REMOVE = 'M6 6l12 12M18 6L6 18'

const { settings, settings_change } = use_settings()

const term_draft = ref('')
const term_input = useTemplateRef<HTMLInputElement>('term_input')

const terms = computed(() => terms_of(settings.value?.vocabulary ?? ''))


async function term_add() {
    const fresh = terms_fresh(terms.value, term_draft.value)

    if (fresh.length === 0) {
        term_draft.value = ''

        return
    }

    if (await settings_change({ vocabulary: [...terms.value, ...fresh].join('\n') })) {
        term_draft.value = ''
    }
}

function term_input_focus() {
    term_input.value?.focus()
}

function term_remove(index: number) {
    const next = terms.value.filter((_, position) => position !== index)

    void settings_change({ vocabulary: next.join('\n') })
}
</script>

<template>
    <div>
        <div class="label-row">
            <label class="label" for="vocabulary-term">Custom vocabulary</label>
            <InfoTip
                label="About custom vocabulary"
                text="Sent to the transcription model and the LLM as spelling hints."
            />
        </div>
        <div class="terms" @click.self="term_input_focus">
            <span v-for="(term, index) in terms" :key="`${index}:${term}`" class="term">
                {{ term }}
                <button
                    type="button"
                    class="term-remove"
                    :aria-label="`Remove ${term}`"
                    @click="term_remove(index)"
                >
                    <svg
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        aria-hidden="true"
                    >
                        <path :d="ICON_REMOVE" />
                    </svg>
                </button>
            </span>
            <input
                id="vocabulary-term"
                ref="term_input"
                v-model="term_draft"
                class="terms-input"
                type="text"
                spellcheck="false"
                autocomplete="off"
                placeholder="Add a term"
                @keydown.enter.prevent="term_add"
                @blur="term_add"
            />
        </div>
    </div>
</template>

<style scoped>
.terms {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3125rem;
    min-height: var(--control-height);
    padding: 0.3125rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-well);
    cursor: text;
    transition: border-color 120ms ease, box-shadow 120ms ease;
}

.terms:hover:not(:focus-within) {
    border-color: var(--color-border-strong);
}

.terms:focus-within {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-accent-soft);
}

.term {
    display: inline-flex;
    align-items: center;
    gap: 0.125rem;
    height: 1.5rem;
    padding: 0 0.125rem 0 0.5rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-raised);
    color: var(--color-content-strong);
    font-size: 0.8125rem;
    cursor: default;
}

.term-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.125rem;
    height: 1.125rem;
    padding: 0;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--color-content-meta);
    transition: background 120ms ease, color 120ms ease;
}

.term-remove:hover {
    background: var(--color-raised-hover);
    color: var(--color-content-strong);
}

.term-remove svg {
    width: 0.625rem;
    height: 0.625rem;
}

.terms-input {
    flex: 1 1 8rem;
    min-width: 0;
    height: 1.5rem;
    padding: 0 0.4375rem;
    border: 0;
    background: transparent;
    color: var(--color-content-strong);
    font: inherit;
    font-size: 0.875rem;
}

.terms-input::placeholder {
    color: var(--color-content-meta);
}

.terms-input:focus {
    outline: none;
}
</style>
