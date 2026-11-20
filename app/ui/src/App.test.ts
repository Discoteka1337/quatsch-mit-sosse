import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import App from './App.svelte';
import de from './locales/de.json';

afterEach(cleanup);

describe('app scaffold', () => {
  it('renders the German settings placeholder without functional controls', () => {
    render(App);
    expect(screen.getByRole('heading', { name: de.settings })).toBeTruthy();
    expect(screen.getByText(de.appName)).toBeTruthy();
    expect(screen.getByText(de.generalTitle)).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('opens the overlay in its configured ready state', async () => {
    const { rerender } = render(App, { view: 'overlay' });
    expect(screen.queryByRole('status')).toBeNull();
    await rerender({ overlay: { showReady: true } });
    expect(screen.getByRole('status', { name: de.overlayReady }).textContent).toBe('');
    expect(screen.queryByRole('heading', { name: de.settings })).toBeNull();
    expect(screen.queryByText(de.generalTitle)).toBeNull();
  });
});
