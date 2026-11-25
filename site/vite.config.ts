import { cloudflare } from "@cloudflare/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
import { defineConfig, loadEnv } from "vite";
import { syncAgentDocs } from "./scripts/sync-agent-docs";

export default defineConfig(({ mode }) => {
  syncAgentDocs();
  const pages = loadEnv(mode, ".", "GITHUB_PAGES").GITHUB_PAGES === "1";
  // `vercel build` sets VERCEL=1; every other server build targets Cloudflare Workers.
  const vercel = !pages && Boolean(process.env.VERCEL);
  const workers = !pages && !vercel && !process.env.VITEST;
  return {
    base: pages ? "/omadesign/" : "/",
    // Edge-cached HTML is keyed per build so a deploy never serves pages that point at removed assets.
    define: { __BUILD_ID__: JSON.stringify(Date.now().toString(36)) },
    server: {
      // The website imports the shared manuals without exposing the rest of the repo.
      fs: { allow: [".", "../docs"] },
    },
    build: pages ? { outDir: ".pages-build" } : undefined,
    plugins: [
      ...(workers ? [cloudflare({ viteEnvironment: { name: "ssr" } })] : []),
      tailwindcss(),
      tanstackStart({
        router: { basepath: pages ? "/omadesign" : "/" },
        prerender: pages
          ? {
              enabled: true,
              autoSubfolderIndex: true,
              autoStaticPathsDiscovery: true,
              crawlLinks: true,
              failOnError: true,
            }
          : undefined,
      }),
      viteReact(),
      // Start prerenders its own static client output; Pages has no server runtime.
      ...(!vercel
        ? []
        : [
            nitro({
              preset: "vercel",
              routeRules: {
                "/install": {
                  headers: {
                    "content-type": "text/plain; charset=utf-8",
                    "cache-control": "public, max-age=300",
                  },
                },
                "/install/": { redirect: "/install" },
              },
            }),
          ]),
    ],
  };
});
