import { mount } from 'svelte';
import App from './App.svelte';
import { defaultLanguage, getMessages } from './i18n';
import './app.css';

document.documentElement.lang = defaultLanguage;
document.title = getMessages().appName;

mount(App, {
  target: document.getElementById('app')!,
  props: {
    view: new URLSearchParams(window.location.search).get('window') === 'overlay'
      ? 'overlay'
      : 'settings',
  },
});
