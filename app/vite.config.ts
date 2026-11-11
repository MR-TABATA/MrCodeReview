import { defineConfig } from 'vite';

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const inTest = Boolean(process.env.VITEST);

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1431 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/.dev/**'] },
    // テストだけ、同じリポジトリの mrcr-core のソース（識別子の書式の突き合わせ。findingtag.test.ts）を読めるようにする。開発サーバーの読み取り範囲は広げない
    fs: inTest ? { allow: ['.', '../crates/mrcr-core/src'] } : undefined,
  },
});
