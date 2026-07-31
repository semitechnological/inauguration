import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { RenderContext, RouteArtifact } from "@tschk/moonshine-framework";
import { pageIr } from "./ir";
import { renderer } from "./renderer";

const route: RouteArtifact = {
  id: "index",
  path: "/",
  file: "",
  mode: "static",
  runtime: "bun",
  decision: "static",
  clientEntries: [],
};

async function main(): Promise<void> {
  const ctx: RenderContext = {
    request: new Request("http://localhost/"),
    route,
    params: {},
    data: pageIr,
    signal: new AbortController().signal,
  };
  const html = await renderer.prerender(ctx);
  const outDir = join(import.meta.dir, "..", "dist");
  await mkdir(outDir, { recursive: true });
  await writeFile(join(outDir, "index.html"), html);
  console.log(`wrote ${join(outDir, "index.html")}`);
}

main();
