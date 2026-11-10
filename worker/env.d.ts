// Secrets, set with `wrangler secret put`; `wrangler types` only sees bindings in wrangler.toml.
interface SecretsEnv {
  /** Admin (scrutinizer) username. */
  SCRUTINIZER_USERNAME?: string;
  /** Bcrypt hash of the admin password, from `pnpm hash-password`. */
  SCRUTINIZER_PASSWORD_HASH?: string;
}

interface Env extends SecretsEnv {}

declare namespace Cloudflare {
  interface Env extends SecretsEnv {}
}
