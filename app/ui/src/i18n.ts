import de from './locales/de.json';
import en from './locales/en.json';

export const defaultLanguage = 'de';
export type Messages = typeof de;

export function getMessages(language: string = defaultLanguage): Messages {
  return language === 'en' ? en : de;
}
