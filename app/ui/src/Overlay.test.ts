import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Overlay from './Overlay.svelte';
import de from './locales/de.json';
import en from './locales/en.json';

afterEach(cleanup);

describe('dictation overlay', () => {
  it('moves from hidden to listening with a live level, then to processing, inserting and ready', async () => {
    const { rerender } = render(Overlay);
    expect(screen.queryByRole('status')).toBeNull();

    await rerender({ state: { kind: 'listening', level: 25 } });
    expect(screen.getByText(de.overlayListening)).toBeTruthy();
    const meter = screen.getByRole('meter', { name: de.overlayLevel }) as HTMLMeterElement;
    expect(meter.value).toBe(25);
    await rerender({ state: { kind: 'listening', level: 80 } });
    expect(meter.value).toBe(80);
    await rerender({ state: { kind: 'listening', level: 120 } });
    expect(meter.value).toBe(100);

    await rerender({ state: { kind: 'processing' } });
    expect(screen.getByText(de.overlayProcessing)).toBeTruthy();
    expect(screen.queryByRole('meter')).toBeNull();
    await rerender({ state: { kind: 'inserting' } });
    expect(screen.getByText(de.overlayInserting)).toBeTruthy();
    expect(screen.queryByText(de.overlayProcessing)).toBeNull();
    await rerender({ state: { kind: 'ready' }, showReady: true });
    expect(screen.getByRole('status', { name: de.overlayReady }).textContent).toBe('');
    await rerender({ showReady: false });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('keeps the last dictation available for an explicit copy and a retry after failure', async () => {
    const copy = vi.fn().mockRejectedValueOnce(new Error()).mockResolvedValueOnce(undefined);
    const text = 'Grüße aus Überlingen';
    const reason = 'Das Eingabefeld ist schreibgeschützt.';
    const { rerender } = render(Overlay, {
      state: { kind: 'error', message: reason, text },
      onCopy: copy,
    });
    expect(screen.getByText(reason)).toBeTruthy();
    expect(screen.queryByText(text)).toBeNull();
    const button = screen.getByRole('button', { name: de.overlayCopy });
    expect(copy).not.toHaveBeenCalled();
    await fireEvent.click(button);
    expect(screen.getByText(de.overlayCopyError)).toBeTruthy();
    await fireEvent.click(button);
    expect(copy.mock.calls).toEqual([[text], [text]]);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.queryByRole('status')).toBeNull();
    expect(screen.queryByText(de.overlayCopyError)).toBeNull();
    await rerender({ state: { kind: 'processing' } });
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.queryByText(reason)).toBeNull();
  });

  it('copies the explanation or the localized error label if no dictation is available', async () => {
    const copy = vi.fn();
    const message = 'Kein Mikrofon gefunden.';
    const { rerender } = render(Overlay, {
      state: { kind: 'error', message, text: '' },
      showReady: true,
      onCopy: copy,
    });
    await fireEvent.click(screen.getByRole('button', { name: de.overlayCopy }));
    expect(copy).toHaveBeenLastCalledWith(message);
    expect(screen.getByRole('status', { name: de.overlayReady })).toBeTruthy();
    await rerender({ language: 'en', state: { kind: 'error', text: '' } });
    expect(screen.getByText(en.overlayError)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: en.overlayCopy }));
    expect(copy).toHaveBeenLastCalledWith(en.overlayError);
  });

  it('shows the administrator notice and explanation in the selected language', async () => {
    const { rerender } = render(Overlay, { state: { kind: 'administrator' } });
    expect(screen.getByText(de.overlayAdministrator)).toBeTruthy();
    expect(screen.getByText(de.overlayAdministratorHelp)).toBeTruthy();
    await rerender({ language: 'en' });
    expect(screen.getByText(en.overlayAdministrator)).toBeTruthy();
    expect(screen.getByText(en.overlayAdministratorHelp)).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
    await rerender({ state: { kind: 'ready' } });
    expect(screen.queryByText(en.overlayAdministrator)).toBeNull();
  });
});
