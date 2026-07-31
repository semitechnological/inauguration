import type { CrepusIr, CrepusNode, StyleMap } from "@tschk/crepus-moonshine";

const FONT = '"JetBrains Mono", ui-monospace, SFMono-Regular, monospace';

const C = {
  bg: "#09090b",
  fg: "#f4f4f5",
  text: "#d4d4d8",
  zinc100: "#f4f4f5",
  zinc200: "#e4e4e7",
  zinc400: "#a1a1aa",
  zinc500: "#71717a",
  zinc800: "#27272a",
  zinc900: "#18181b",
  blue400: "#60a5fa",
  white: "#ffffff",
};

type S = StyleMap;

function text(content: string, style?: S): CrepusNode {
  return { kind: "text", content, style };
}

function col(children: CrepusNode[], style?: S): CrepusNode {
  return { kind: "stack", axis: "column", children, style };
}

function row(children: CrepusNode[], style?: S): CrepusNode {
  return { kind: "stack", axis: "horizontal", children, style };
}

function block(children: CrepusNode[], style?: S): CrepusNode {
  return { kind: "stack", axis: "column", children, style: { display: "block", ...style } };
}

function image(src: string, style?: S): CrepusNode {
  return { kind: "image", src, alt: "", style };
}

function link(
  href: string,
  content: string | CrepusNode[],
  style?: S,
  extra?: { target?: string; rel?: string },
): CrepusNode {
  const isArr = Array.isArray(content);
  return {
    kind: "link",
    href,
    ...(isArr ? { children: content } : { content: content as string }),
    style,
    target: extra?.target,
    rel: extra?.rel,
  };
}

function dot(): CrepusNode {
  return text("", { display: "inline-block", width: 8, height: 8, borderRadius: 999, background: C.zinc800 });
}

function navLink(href: string, label: string, extra?: { target?: string; rel?: string }): CrepusNode {
  return link(href, label, { textDecoration: "none", color: C.zinc400, fontSize: 12, transition: "color 0.2s" }, extra);
}

function groupHeading(label: string): CrepusNode {
  return text(label, {
    color: C.blue400,
    fontWeight: 600,
    fontSize: 12,
    textTransform: "uppercase",
    letterSpacing: "0.1em",
    borderBottom: `1px solid ${C.zinc800}`,
    paddingBottom: 8,
    marginBottom: 12,
    display: "block",
  });
}

type DocItem = { href: string; name: string; desc: string };

function docLink(item: DocItem): CrepusNode {
  return link(
    item.href,
    [
      text(item.name, { color: C.zinc100, fontWeight: 600, fontFamily: FONT }),
      text(item.desc, { color: C.zinc500 }),
    ],
    { display: "block", textDecoration: "none", color: C.zinc400, transition: "all 0.2s ease-in-out" },
  );
}

function docGroup(heading: string, items: DocItem[]): CrepusNode {
  return col([
    groupHeading(heading),
    col(items.map(docLink), { gap: 12 }),
  ]);
}

function featureCard(title: string, body: string): CrepusNode {
  return col(
    [
      text(title, {
        color: C.zinc100,
        fontWeight: 600,
        fontSize: 12,
        textTransform: "uppercase",
        letterSpacing: "0.05em",
        marginBottom: 8,
      }),
      block([text(body)], { color: C.zinc400, fontSize: 12, lineHeight: 1.7, margin: 0 }),
    ],
    { border: `1px solid ${C.zinc800}`, borderRadius: 6, padding: 16, background: "rgba(24,24,27,0.5)", flex: "1 1 0", minWidth: 200 },
  );
}

const header: CrepusNode = row(
  [
    row(
      [
        image("/static/favicon.svg", {
          width: 32,
          height: 32,
          borderRadius: 4,
          background: C.zinc900,
          border: `1px solid ${C.zinc800}`,
          padding: 4,
        }),
        col(
          [
            text("inauguration", { color: C.zinc100, fontSize: 16, fontWeight: 600, letterSpacing: "0.05em" }),
            text("ultrafast hybrid compile · hot reload · JIT", { fontSize: 12, color: C.zinc500, marginTop: 2 }),
          ],
          { gap: 2 },
        ),
      ],
      { gap: 12, alignItems: "center" },
    ),
    row(
      [
        navLink("/docs/in-language.html", "Docs"),
        navLink("https://github.com/semitechnological/inauguration", "GitHub", { target: "_blank", rel: "noopener noreferrer" }),
        navLink("https://crates.io/crates/inauguration", "Crates.io", { target: "_blank", rel: "noopener noreferrer" }),
      ],
      { gap: 16 },
    ),
  ],
  {
    alignItems: "center",
    justifyContent: "space-between",
    borderBottom: `1px solid ${C.zinc800}`,
    paddingBottom: 24,
    animation: "in-docs-fade-up 0.45s ease-out both",
    animationDelay: "0.02s",
  },
);

const hero: CrepusNode = row(
  [
    col(
      [
        text("THE STACK: INLANG + CREPUSCULARITY", {
          color: C.blue400,
          fontWeight: 600,
          letterSpacing: "0.1em",
          fontSize: 12,
          textTransform: "uppercase",
          marginBottom: 8,
        }),
        text("Ultrafast compiler pipeline for OOP and systems languages", {
          fontSize: 30,
          fontWeight: 600,
          color: C.zinc100,
          letterSpacing: "-0.025em",
          lineHeight: 1.3,
          marginBottom: 16,
        }),
        block(
          [
            text("A single unified Core IR → MIR → native JIT emitter. No LLVM, no bytecode VM. Experience sub-millisecond compile loops, live code injection, and seamless GUI integration using the ", { color: C.zinc400, lineHeight: 1.7 }),
            text("inlang (.in)", { color: C.zinc200, fontWeight: 600 }),
            text(" systems language coupled with ", { color: C.zinc400, lineHeight: 1.7 }),
            text("crepuscularity", { color: C.zinc200, fontWeight: 600 }),
            text(" for hardware-accelerated declarative UI everywhere.", { color: C.zinc400, lineHeight: 1.7 }),
          ],
          { color: C.zinc400, lineHeight: 1.7, marginBottom: 24 },
        ),
        row(
          [
            link("/docs/in-language.html", "Get Started", {
              textDecoration: "none",
              display: "inline-flex",
              alignItems: "center",
              padding: "8px 16px",
              background: C.zinc100,
              color: C.bg,
              fontSize: 12,
              fontWeight: 600,
              borderRadius: 6,
              transition: "background 0.2s",
            }),
            link("https://github.com/semitechnological/inauguration", "View GitHub", {
              textDecoration: "none",
              display: "inline-flex",
              alignItems: "center",
              padding: "8px 16px",
              border: `1px solid ${C.zinc800}`,
              color: C.zinc400,
              fontSize: 12,
              fontWeight: 600,
              borderRadius: 6,
              transition: "all 0.2s",
            }),
          ],
          { gap: 12 },
        ),
      ],
      { flex: 1, maxWidth: "42rem" },
    ),
    col(
      [
        row(
          [
            text("inauguration-shell", { fontSize: 12, color: C.zinc500, fontFamily: FONT }),
            row([dot(), dot(), dot()], { gap: 4 }),
          ],
          { alignItems: "center", justifyContent: "space-between", padding: 12, borderBottom: `1px solid ${C.zinc800}`, background: C.bg },
        ),
        block(
          [
            text("# Run an .in script\n", { color: C.zinc500 }),
            text("$ in eval hello.in\n", { color: C.zinc400 }),
            text("\n", {}),
            text("# Run the test suite\n", { color: C.zinc500 }),
            text("$ in test\n", { color: C.zinc400 }),
            text("\n", {}),
            text("# Update the compiler toolchain\n", { color: C.zinc500 }),
            text("$ in self-update", { color: C.zinc400 }),
          ],
          {
            padding: 16,
            margin: 0,
            fontFamily: FONT,
            fontSize: 12,
            lineHeight: 1.7,
            color: C.zinc400,
            overflowX: "auto",
            whiteSpace: "pre",
          },
        ),
      ],
      {
        width: "100%",
        maxWidth: 350,
        flexShrink: 0,
        border: `1px solid ${C.zinc800}`,
        borderRadius: 6,
        background: C.zinc900,
        overflow: "hidden",
        animation: "in-docs-terminal-glow 5s ease-in-out infinite",
      },
    ),
  ],
  {
    gap: 32,
    alignItems: "flex-start",
    justifyContent: "space-between",
    paddingTop: 16,
    paddingBottom: 16,
    flexWrap: "wrap",
    animation: "in-docs-fade-up 0.45s ease-out both",
    animationDelay: "0.06s",
  },
);

const techSpecs: CrepusNode = col(
  [
    text("PIPELINE CAPABILITIES", { color: C.zinc100, fontWeight: 600, letterSpacing: "0.05em", marginBottom: 24 }),
    row(
      [
        featureCard(
          "1. Multi-Frontend Parser",
          "Uses Tree-sitter polyglot fronts to ingest Swift, C, Rust, Go, Python, Zig, and Java. Lowers source files directly into a unified Core IR AST, mapping structural semantics cleanly.",
        ),
        featureCard(
          "2. MIR & Offset Assembly",
          "Translates Core IR to MIR (Mid IR). MIR defers relative address offsets, performing label resolution and instruction alignment before assembling to binary buffers.",
        ),
        featureCard(
          "3. Native JIT Emitters",
          "Direct machine code emission for AArch64 (ARM64) and x86_64. Native page allocation maps binary targets to executable memory, running JIT compiles in microseconds.",
        ),
      ],
      { gap: 24, flexWrap: "wrap" },
    ),
  ],
  { borderTop: `1px solid ${C.zinc800}`, paddingTop: 32, animation: "in-docs-fade-up 0.45s ease-out both", animationDelay: "0.1s" },
);

const col1: CrepusNode = col(
  [
    docGroup("Language & Core IR", [
      { href: "/docs/in-language.html", name: "in-language.md", desc: " — learn the brace + line-oriented syntax and type surface of .in" },
      { href: "/docs/languages.html", name: "languages.md", desc: " — authoritative matrix of supported language fronts and capabilities" },
      { href: "/docs/multi-frontend-ir.html", name: "multi-frontend-ir.md", desc: " — design of the unified Core IR representation" },
      { href: "/docs/core-ir-extensions.html", name: "core-ir-extensions.md", desc: " — design specs for SIL representation and VM execution" },
      { href: "/docs/parser-surface.html", name: "parser-surface.md", desc: " — extension resolution and shebang registry details" },
    ]),
    docGroup("Compiler Implementation", [
      { href: "/docs/general-compiler.html", name: "general-compiler.md", desc: " — orchestration stages, compiler entry points, and FFI bindings" },
      { href: "/docs/native-backend.html", name: "native-backend.md", desc: " — binary emission mechanics for AArch64 and x86_64 JIT" },
      { href: "/docs/orchestration-compiler.html", name: "orchestration-compiler.md", desc: " — driver pipeline and multi-front orchestration contract" },
      { href: "/docs/contributing-hybrid-mirror.html", name: "contributing-hybrid-mirror.md", desc: " — workflow to develop in-cli and rust-driver concurrently" },
    ]),
  ],
  { gap: 24, flex: "1 1 0", minWidth: 280 },
);

const col2: CrepusNode = col(
  [
    docGroup("Benchmarks & Performance", [
      { href: "/docs/jit.html", name: "jit.md", desc: " — latency comparison between interpretive and machine code runtimes" },
      { href: "/docs/swift-vs-in.html", name: "swift-vs-in.md", desc: " — performance benchmarks compiling swift files via .in" },
      { href: "/docs/polyglot-compilers.html", name: "polyglot-compilers.md", desc: " — execution metrics across diverse tree-sitter parser targets" },
      { href: "/docs/self-host-vs-native.html", name: "self-host-vs-native.md", desc: " — comparisons of compiled artifacts and compiler self-hosting" },
    ]),
    docGroup("Roadmaps & Planning", [
      { href: "/docs/roadmap-execution-plan.html", name: "roadmap-execution-plan.md", desc: " — development phases, targets, and language specifications" },
      { href: "/docs/universal-compiler-roadmap.html", name: "universal-compiler-roadmap.md", desc: " — plan for multi-frontend and cross-architecture expansion" },
      { href: "/docs/interop-roadmap.html", name: "interop-roadmap.md", desc: " — roadmap for direct C/Rust FFI and interoperability mechanics" },
      { href: "/docs/local-mvp-runbook.html", name: "local-mvp-runbook.md", desc: " — developer testing protocols and verification checklist" },
      { href: "/docs/conformance-matrix.html", name: "conformance-matrix.md", desc: " — validation test suite and feature conformance mapping" },
      { href: "/docs/future-work-roadmap.html", name: "future-work-roadmap.md", desc: " — future type assertions and native bridge improvements" },
    ]),
  ],
  { gap: 24, flex: "1 1 0", minWidth: 280 },
);

const docsDirectory: CrepusNode = col(
  [
    text("DOCUMENTATION DIRECTORY", { color: C.zinc100, fontWeight: 600, letterSpacing: "0.05em", marginBottom: 24 }),
    row([col1, col2], { gap: 32, flexWrap: "wrap" }),
  ],
  { borderTop: `1px solid ${C.zinc800}`, paddingTop: 32, animation: "in-docs-fade-up 0.45s ease-out both", animationDelay: "0.14s" },
);

const footer: CrepusNode = row(
  [
    row(
      [
        image("/static/favicon.svg", {
          width: 24,
          height: 24,
          borderRadius: 4,
          opacity: 0.5,
          background: C.zinc900,
          border: `1px solid ${C.zinc800}`,
          padding: 4,
        }),
        text("inauguration docs-site · built with crepuscularity", { color: C.zinc500, fontSize: 12 }),
      ],
      { gap: 8, alignItems: "center" },
    ),
    row(
      [
        navLink("https://github.com/semitechnological/inauguration", "GitHub", { target: "_blank", rel: "noopener noreferrer" }),
        navLink("https://crates.io/crates/inauguration", "Crates.io", { target: "_blank", rel: "noopener noreferrer" }),
      ],
      { gap: 16 },
    ),
  ],
  {
    borderTop: `1px solid ${C.zinc800}`,
    paddingTop: 32,
    paddingBottom: 48,
    justifyContent: "space-between",
    alignItems: "center",
    gap: 16,
    fontSize: 12,
    color: C.zinc500,
    flexWrap: "wrap",
    animation: "in-docs-fade-up 0.45s ease-out both",
    animationDelay: "0.2s",
  },
);

export const pageIr: CrepusIr = {
  version: 1,
  root: [
    col(
      [col([header, hero, techSpecs, docsDirectory, footer], { maxWidth: "56rem", marginInline: "auto", gap: 40 })],
      {
        minHeight: "100vh",
        backgroundColor: C.bg,
        color: C.text,
        padding: 24,
        fontSize: 14,
        lineHeight: 1.7,
        fontFamily: FONT,
      },
    ),
  ],
};
