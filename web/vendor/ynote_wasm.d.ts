/* tslint:disable */
/* eslint-disable */

/**
 * The document's display title (front-matter `title`, else first H1), for the
 * file list / tab label. Empty string when the document has no title.
 */
export function doc_title(markdown: string): string;

/**
 * Full text of the section under `slug` (heading line included). Lets the UI
 * (and later the extension) target one section without rewriting the file.
 */
export function extract_section(markdown: string, slug: string): string;

/**
 * The document outline as a JSON array of `{level, text, slug, line}` — feeds
 * the sidebar and click-to-jump. Slugs match the in-document anchor ids.
 */
export function outline_json(markdown: string): string;

/**
 * The CSS the preview fragment needs: the default page/typography theme plus
 * class-based syntax-highlighting rules for the current syntect theme.
 * Injected once into a `<style>` tag; stable for the session.
 */
export function preview_css(): string;

/**
 * Render markdown to a `<div class="ynote">…</div>` fragment for live
 * preview. No embedded CSS — the page supplies it via [`preview_css`].
 *
 * Local images are **not** inlined here: the browser holds files behind a
 * folder handle, not a path, so relative-image resolution is done in JS
 * (rewrite `src` to a blob URL) before/after this call. Phase-1 gap.
 */
export function render_fragment(markdown: string): string;

/**
 * Render markdown to a full standalone, self-contained HTML document
 * (embedded theme CSS). This is the HTML *export* — identical pipeline to the
 * preview, only wrapped. Suitable for writing back to the folder or download.
 */
export function render_standalone(markdown: string, title?: string | null): string;

/**
 * Replace the whole section under `slug` with `content` (which must include
 * the heading line). Returns the new full document text.
 */
export function replace_section(markdown: string, slug: string, content: string): string;

/**
 * Install a panic hook so Rust panics show up as readable console errors.
 * Called once from JS at startup.
 */
export function start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly doc_title: (a: number, b: number, c: number) => void;
    readonly extract_section: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly outline_json: (a: number, b: number, c: number) => void;
    readonly preview_css: (a: number) => void;
    readonly render_fragment: (a: number, b: number, c: number) => void;
    readonly render_standalone: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly replace_section: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly start: () => void;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
