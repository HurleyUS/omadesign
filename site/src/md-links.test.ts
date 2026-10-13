import { expect, it } from "vitest";
import { documentationUrl } from "./md";

it("keeps the brush-library link on the published site and under a Pages base", () => {
  const source = "docs/blog/nightly-brushes-follow-your-hand.md";
  expect(documentationUrl("/brushes", source, "/")).toBe("/brushes/");
  expect(documentationUrl("/brushes", source, "/omadesign/")).toBe("/omadesign/brushes/");
});
