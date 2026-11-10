declare namespace Cloudflare {
  interface Env {
    /** Plain-text admin password matching SCRUTINIZER_PASSWORD_HASH, injected by vitest.config.js. */
    TEST_ADMIN_PASSWORD: string;
  }
}
