<script lang="ts">
  import { getMessages } from './i18n';

  let { view = 'settings' }: { view?: 'settings' | 'overlay' } = $props();
  const messages = getMessages();
</script>

{#if view === 'overlay'}
  <main class="overlay">
    <p>{messages.overlayPlaceholder}</p>
  </main>
{:else}
  <main class="settings">
    <p class="app-name">{messages.appName}</p>
    <h1>{messages.settings}</h1>
    <p class="placeholder">{messages.settingsPlaceholder}</p>
  </main>
{/if}

<style>
  .settings {
    min-height: 100vh;
    padding: 40px;
    background: #f7f8fa;
    color: #202632;
  }

  .app-name {
    margin: 0 0 24px;
    color: #4d5b70;
    font-weight: 600;
  }

  h1 {
    margin: 0 0 16px;
    font-size: 1.75rem;
    font-weight: 600;
  }

  .placeholder {
    max-width: 40rem;
    margin: 0;
    line-height: 1.6;
  }

  .overlay {
    display: grid;
    min-height: 100vh;
    place-items: center;
    background: transparent;
  }

  .overlay p {
    margin: 8px;
    padding: 10px 20px;
    border: 1px solid #ffffff40;
    border-radius: 24px;
    background: #202632ee;
    color: #fff;
  }
</style>
