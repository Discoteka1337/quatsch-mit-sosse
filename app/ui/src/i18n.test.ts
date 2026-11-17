import { describe, expect, it } from 'vitest';
import de from './locales/de.json';
import en from './locales/en.json';
import { getMessages } from './i18n';

describe('translations', () => {
  it('uses German by default and for unsupported languages', () => {
    expect(getMessages()).toEqual(de);
    expect(getMessages('fr')).toEqual(de);
    expect(getMessages('constructor')).toEqual(de);
    expect(getMessages().settings).toBe('Einstellungen');
  });

  it('provides English with the same nonempty translation keys', () => {
    expect(getMessages('en')).toEqual(en);
    expect(Object.keys(en).sort()).toEqual(Object.keys(de).sort());
    for (const messages of [de, en]) {
      expect(Object.values(messages).every((text) => text.trim().length > 0)).toBe(true);
      expect(messages.appName).toBe('Quatsch mit Soße');
    }
  });
});
