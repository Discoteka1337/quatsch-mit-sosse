import { fileURLToPath } from 'node:url';
import { logError, run } from '@tauri-apps/cli';

process.chdir(fileURLToPath(new URL('../../', import.meta.url)));

try {
  await run(process.argv.slice(2), 'npm run tauri --');
} catch (error) {
  logError(error.message);
  process.exitCode = 1;
}
