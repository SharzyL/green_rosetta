import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { hashSync } from "bcrypt-ts";
import { defineConfig } from "vitest/config";

export const TEST_ADMIN_USERNAME = "admin";
export const TEST_ADMIN_PASSWORD = "correct horse battery staple";

const coverage = {
  provider: "istanbul", // v8 is not supported in the workers runtime
  reporter: ["text", "json-summary", "html", "json"],
  exclude: ["**/test/**"],
};

export default defineConfig({
  test: {
    coverage,
    projects: [
      defineConfig({
        plugins: [
          cloudflareTest({
            wrangler: { configPath: "./wrangler.toml" },
            // Tests seed a local bucket instead of reading the real one.
            remoteBindings: false,
            miniflare: {
              bindings: {
                SCRUTINIZER_USERNAME: TEST_ADMIN_USERNAME,
                SCRUTINIZER_PASSWORD_HASH: hashSync(TEST_ADMIN_PASSWORD, 4),
                TEST_ADMIN_PASSWORD,
                // Admin tests make many calls in quick succession.
                ADMIN_RATE_LIMIT_PER_SECOND: 1000,
              },
            },
          }),
        ],
        test: {
          name: "Workers",
          include: ["worker/test/**/*.spec.ts"],
          coverage,
        },
      }),
    ],
  },
});
