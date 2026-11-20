<script module lang="ts">
  export type OverlayState =
    | { kind: 'ready' | 'processing' | 'inserting' | 'administrator' }
    | { kind: 'listening'; level: number }
    | { kind: 'error'; message?: string; text: string };
</script>

<script lang="ts">
  import { getMessages } from './i18n';

  let {
    state: status = { kind: 'ready' },
    showReady = false,
    language,
    onCopy = (text) => navigator.clipboard.writeText(text),
  }: {
    state?: OverlayState;
    showReady?: boolean;
    language?: string;
    onCopy?: (text: string) => void | Promise<void>;
  } = $props();

  const messages = $derived(getMessages(language));
  let copying = $state(false);
  let copyFailed = $state(false);

  $effect(() => {
    // Feedback belongs to the current failure only.
    void status;
    copying = false;
    copyFailed = false;
  });

  async function copyText() {
    const failure = status;
    if (failure.kind !== 'error' || copying) return;
    copying = true;
    copyFailed = false;
    try {
      await onCopy(failure.text || failure.message || messages.overlayError);
      if (status === failure) status = { kind: 'ready' };
    } catch {
      if (status === failure) copyFailed = true;
    } finally {
      if (status === failure) copying = false;
    }
  }
</script>

{#if status.kind === 'ready'}
  {#if showReady}
    <div class="dot" role="status" aria-label={messages.overlayReady}></div>
  {/if}
{:else}
  <div class="bar" class:notice={status.kind === 'error' || status.kind === 'administrator'} role="status">
    {#if status.kind === 'listening'}
      <span>{messages.overlayListening}</span>
      <meter
        aria-label={messages.overlayLevel}
        min="0"
        max="100"
        value={Number.isFinite(status.level) ? Math.trunc(status.level) : 0}
      ></meter>
    {:else if status.kind === 'processing'}
      <span>{messages.overlayProcessing}</span>
    {:else if status.kind === 'inserting'}
      <span>{messages.overlayInserting}</span>
    {:else if status.kind === 'administrator'}
      <strong>{messages.overlayAdministrator}</strong>
      <small>{messages.overlayAdministratorHelp}</small>
    {:else if status.kind === 'error'}
      <div class="actions">
        <strong>{copyFailed ? messages.overlayCopyError : status.message || messages.overlayError}</strong>
        <button
          type="button"
          disabled={copying}
          onmousedown={(event) => event.preventDefault()}
          onclick={copyText}
        >{messages.overlayCopy}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .dot,
  .bar {
    color-scheme: light dark;
    font: 14px/1.3 'Segoe UI Variable', 'Segoe UI', sans-serif;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: light-dark(#517567, #9bc2b0);
  }

  .bar {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    width: 160px;
    min-height: 36px;
    padding: 4px 12px;
    border: 1px solid light-dark(#c3c9c9, #596362);
    border-radius: 14px;
    background: light-dark(#f8faf9, #262d2c);
    color: light-dark(#1f2926, #eef3f0);
  }

  meter {
    width: 48px;
  }

  .notice {
    align-items: stretch;
    gap: 4px;
    width: max-content;
    max-width: min(300px, 100%);
    flex-direction: column;
    padding-block: 6px;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
  }

  button {
    font: inherit;
  }
</style>
