import handler from "@tanstack/react-start/server-entry";
import { AUTH_PATH } from "./start";

/** Public HTML is identical for every visitor, so the edge keeps it for five minutes. */
declare const __BUILD_ID__: string;

const PUBLIC_HTML = "public, max-age=0, s-maxage=300, stale-while-revalidate=86400";

/** Keep public pages and public API answers (such as /api/stats) in the colo cache. */
async function cached(request: Request, url: URL, ctx: { waitUntil(promise: Promise<unknown>): void }) {
  const cache = (caches as unknown as { default: Cache }).default;
  const keyUrl = new URL(url);
  keyUrl.searchParams.set("__build", __BUILD_ID__);
  const key = new Request(keyUrl.toString(), { method: "GET" });
  const hit = await cache.match(key);
  if (hit) return hit;

  const response = await handler.fetch(request);
  if (response.status !== 200 || response.headers.has("set-cookie")) return response;
  const html = response.headers.get("content-type")?.includes("text/html");
  if (!html && !response.headers.get("cache-control")?.includes("public")) return response;
  const fresh = new Response(response.body, response);
  if (html) fresh.headers.set("cache-control", PUBLIC_HTML);
  ctx.waitUntil(cache.put(key, fresh.clone()));
  return fresh;
}

export default {
  async fetch(request: Request, _env: unknown, ctx: { waitUntil(promise: Promise<unknown>): void }) {
    const url = new URL(request.url);
    if (url.hostname === "www.omadesign.app") {
      url.hostname = "omadesign.app";
      return Response.redirect(url.toString(), 301);
    }
    const cacheable =
      request.method === "GET" &&
      !AUTH_PATH.test(url.pathname) &&
      !url.pathname.startsWith("/_serverFn") &&
      !request.headers.has("authorization");
    return cacheable ? cached(request, url, ctx) : handler.fetch(request);
  },
};
