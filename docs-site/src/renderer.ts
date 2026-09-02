import { crepusRenderer } from "@tschk/crepus-moonshine";
import type { RenderContext, Renderer } from "@tschk/moonshine-framework";
import { headHtml } from "./head";

function wrapDocument(html: string): string {
  return html
    .replace("<html>", '<html lang="en">')
    .replace("<head>", `<head>${headHtml}`);
}

export const renderer: Renderer = {
  name: "crepus-head",
  async render(context: RenderContext): Promise<Response> {
    const res = await crepusRenderer.render(context);
    const html = await res.text();
    return new Response(wrapDocument(html), {
      status: res.status,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  },
  async prerender(context: RenderContext): Promise<string> {
    const html = await crepusRenderer.prerender(context);
    return wrapDocument(html);
  },
};
