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
    expect(screen.getByText(de.settingsPlaceholder)).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('renders only the overlay placeholder in the overlay window', () => {
    render(App, { view: 'overlay' });
    expect(screen.getByText(de.overlayPlaceholder)).toBeTruthy();
    expect(screen.queryByRole('heading', { name: de.settings })).toBeNull();
    expect(screen.queryByText(de.settingsPlaceholder)).toBeNull();
  });
});
