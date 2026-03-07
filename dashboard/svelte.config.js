import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  kit: {
    adapter: adapter({
      pages: 'build',
      assets: 'build',
      fallback: 'index.html',
    }),
    alias: {
      '@ghost/sdk': '../packages/sdk/src/index.ts',
    },
    serviceWorker: {
      register: false, // We register manually in +layout.svelte for more control.
    },
  },
};

export default config;
