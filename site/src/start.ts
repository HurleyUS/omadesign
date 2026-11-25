import { clerkMiddleware } from "@clerk/tanstack-react-start/server";
import { createMiddleware, createStart } from "@tanstack/react-start";

/** Signed-in pages; every other page renders signed out and stays cacheable. */
export const AUTH_PATH = /^\/(?:account|cloud|compete|project)(?:\/|$)/;

const clerk = clerkMiddleware();
const clerkOnAuthPages = createMiddleware().server((args) =>
  AUTH_PATH.test(new URL(args.request.url).pathname)
    ? clerk.options.server!(args as never)
    : args.next(),
);

export const startInstance = createStart(() => ({
  requestMiddleware: [clerkOnAuthPages],
}));
