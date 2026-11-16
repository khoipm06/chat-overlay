import DOMPurify from 'dompurify';
import katex from 'katex';
import MarkdownIt from 'markdown-it';
import markdownItDeflist from 'markdown-it-deflist';
import markdownItFootnote from 'markdown-it-footnote';
import markdownItTaskLists from 'markdown-it-task-lists';
import mermaid from 'mermaid';
import { createHighlighter, type Highlighter } from 'shiki';

mermaid.initialize({
  startOnLoad: false,
  theme: 'dark',
  securityLevel: 'loose',
  fontFamily: 'Inter, system-ui, -apple-system, sans-serif',
});

let highlighterPromise: Promise<Highlighter> | null = null;
let cachedHighlighter: Highlighter | null = null;

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: ['dark-plus'],
      langs: [
        'typescript',
        'javascript',
        'python',
        'rust',
        'bash',
        'sh',
        'json',
        'css',
        'html',
        'markdown',
        'yaml',
        'toml',
        'sql',
        'go',
        'c',
        'cpp',
        'diff',
        'tsx',
        'jsx',
      ],
    }).then((hl) => {
      cachedHighlighter = hl;
      return hl;
    });
  }
  return highlighterPromise;
}

// Pre-warm Shiki in background on renderer boot
void getHighlighter().catch(() => {});

interface ExtractedBlock {
  token: string;
  html: string;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Extracts fenced code blocks (so math protection never touches code) and
 * extracts display `$$...$$` / `\[...\]` and inline `$...$` / `\(...\)` math
 * BEFORE markdown-it runs italics/escaping on underscores and backslashes.
 */
function preprocessMathOutsideCode(src: string): {
  markdown: string;
  mathBlocks: ExtractedBlock[];
} {
  const mathBlocks: ExtractedBlock[] = [];
  let seq = 0;

  const pushMath = (expr: string, displayMode: boolean): string => {
    const token = `@@MATH_TOKEN_${seq++}@@`;
    const rendered = katex.renderToString(expr.trim(), {
      displayMode,
      throwOnError: false,
      strict: false,
    });
    const html = displayMode
      ? `<div class="md-math-block">${rendered}</div>`
      : `<span class="md-math-inline">${rendered}</span>`;
    mathBlocks.push({ token, html });
    return token;
  };

  // Split by fenced code blocks and inline code spans so we don't mangle $ inside code
  const segments = src.split(/(```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]+`)/g);
  const out = segments
    .map((seg, idx) => {
      if (idx % 2 === 1) return seg; // code block or inline code
      let text = seg;
      // Display math $$...$$ or \[...\]
      text = text.replace(/\$\$([\s\S]+?)\$\$/g, (_m, expr) => `\n\n${pushMath(expr, true)}\n\n`);
      text = text.replace(/\\\[([\s\S]+?)\\\]/g, (_m, expr) => `\n\n${pushMath(expr, true)}\n\n`);
      // Inline math \(...\)
      text = text.replace(/\\\((.+?)\\\)/g, (_m, expr) => pushMath(expr, false));
      // Inline math $...$ (avoid currency like $100 followed by space/punctuation if unpaired)
      text = text.replace(/(?<!\\)\$(?!\s)([^\n$]+?)(?<!\s)\$/g, (_m, expr) =>
        pushMath(expr, false),
      );
      return text;
    })
    .join('');

  return { markdown: out, mathBlocks };
}

const CALLOUT_META: Record<string, { label: string; icon: string; cls: string }> = {
  NOTE: { label: 'Note', icon: 'codicon-info', cls: 'md-callout-note' },
  TIP: { label: 'Tip', icon: 'codicon-lightbulb', cls: 'md-callout-tip' },
  IMPORTANT: { label: 'Important', icon: 'codicon-report', cls: 'md-callout-important' },
  WARNING: { label: 'Warning', icon: 'codicon-warning', cls: 'md-callout-warning' },
  CAUTION: { label: 'Caution', icon: 'codicon-flame', cls: 'md-callout-caution' },
};

function createMarkdownInstance(codePlaceholders: ExtractedBlock[]): MarkdownIt {
  const md = new MarkdownIt({
    html: true,
    linkify: true,
    typographer: false,
    breaks: false,
  });

  md.use(markdownItTaskLists, { enabled: false, label: true });
  md.use(markdownItFootnote);
  md.use(markdownItDeflist);

  // GitHub-style Alerts / Callouts: transform blockquote paragraphs starting with [!NOTE], etc.
  md.core.ruler.after('block', 'github_callouts', (state) => {
    const tokens = state.tokens;
    for (let i = 0; i < tokens.length - 4; i++) {
      if (tokens[i].type !== 'blockquote_open') continue;
      const pOpen = tokens[i + 1];
      const inline = tokens[i + 2];
      if (!pOpen || pOpen.type !== 'paragraph_open' || !inline || inline.type !== 'inline') {
        continue;
      }
      const match = inline.content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(?:\r?\n)?/i);
      if (!match) continue;
      const kind = match[1].toUpperCase();
      const meta = CALLOUT_META[kind];
      if (!meta) continue;

      tokens[i].attrJoin('class', `md-callout ${meta.cls}`);
      inline.content = inline.content.slice(match[0].length);
      if (inline.children && inline.children.length > 0) {
        const raw = inline.children.map((c) => c.content).join('');
        if (raw.startsWith(match[0])) {
          // Strip marker from inline children cleanly
          let remaining = match[0].length;
          for (const child of inline.children) {
            if (remaining <= 0) break;
            if (child.content.length <= remaining) {
              remaining -= child.content.length;
              child.content = '';
            } else {
              child.content = child.content.slice(remaining);
              remaining = 0;
            }
          }
        }
      }

      // Insert callout header token right after blockquote_open
      const titleToken = new state.Token('html_block', '', 0);
      titleToken.content = `<div class="md-callout-title"><i class="codicon ${meta.icon}"></i><span>${meta.label}</span></div>\n`;
      tokens.splice(i + 1, 0, titleToken);
    }
  });

  // Custom responsive table wrapper
  const defaultTableOpen =
    md.renderer.rules.table_open ||
    ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));
  const defaultTableClose =
    md.renderer.rules.table_close ||
    ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));
  md.renderer.rules.table_open = (tokens, idx, options, env, self) =>
    `<div class="md-table-wrap">${defaultTableOpen(tokens, idx, options, env, self)}`;
  md.renderer.rules.table_close = (tokens, idx, options, env, self) =>
    `${defaultTableClose(tokens, idx, options, env, self)}</div>`;

  // Fenced code blocks & Mermaid diagrams stored as post-DOMPurify placeholders
  md.renderer.rules.fence = (tokens, idx) => {
    const tok = tokens[idx];
    const info = tok.info.trim();
    const lang = (info.split(/\s+/)[0] || '').toLowerCase();
    const code = tok.content;
    const tokenKey = `@@CODE_BLOCK_${codePlaceholders.length}@@`;

    if (lang === 'mermaid') {
      const encoded = escapeHtml(code);
      const html = `<div class="md-mermaid-card" data-mermaid-source="${encoded}">
        <div class="md-code-header">
          <span class="md-code-lang">MERMAID</span>
          <div class="md-code-actions">
            <button type="button" class="md-code-btn md-mermaid-toggle" title="Toggle source / diagram">Source</button>
            <button type="button" class="md-code-btn md-copy-code-btn" data-code="${encoded}" title="Copy diagram source"><i class="codicon codicon-copy"></i><span>Copy</span></button>
          </div>
        </div>
        <div class="md-mermaid-preview"><div class="md-mermaid-loading"><i class="codicon codicon-loading codicon-modifier-spin"></i><span>Rendering diagram...</span></div></div>
        <pre class="md-mermaid-raw hidden"><code>${escapeHtml(code)}</code></pre>
      </div>`;
      codePlaceholders.push({ token: tokenKey, html });
      return `\n${tokenKey}\n`;
    }

    const displayLang = lang ? lang.toUpperCase() : 'TEXT';
    let highlightedHtml = `<pre class="shiki dark-plus"><code>${escapeHtml(code)}</code></pre>`;
    if (cachedHighlighter && lang) {
      const loaded = cachedHighlighter.getLoadedLanguages();
      const targetLang = loaded.includes(lang) ? lang : 'text';
      if (targetLang !== 'text') {
        highlightedHtml = cachedHighlighter.codeToHtml(code, {
          lang: targetLang,
          theme: 'dark-plus',
        });
      }
    }

    const encodedCode = escapeHtml(code);
    const html = `<div class="md-code-card" data-lang="${escapeHtml(lang)}" data-raw-code="${encodedCode}">
      <div class="md-code-header">
        <span class="md-code-lang">${escapeHtml(displayLang)}</span>
        <button type="button" class="md-code-btn md-copy-code-btn" data-code="${encodedCode}" title="Copy code"><i class="codicon codicon-copy"></i><span>Copy</span></button>
      </div>
      <div class="md-code-body">${highlightedHtml}</div>
    </div>`;
    codePlaceholders.push({ token: tokenKey, html });
    return `\n${tokenKey}\n`;
  };

  return md;
}

export function renderMarkdownHtml(source: string): string {
  const { markdown, mathBlocks } = preprocessMathOutsideCode(source || '');
  const codePlaceholders: ExtractedBlock[] = [];
  const md = createMarkdownInstance(codePlaceholders);

  const rawHtml = md.render(markdown);
  let safeHtml = DOMPurify.sanitize(rawHtml, {
    ADD_TAGS: ['details', 'summary', 'input'],
    ADD_ATTR: ['type', 'checked', 'disabled', 'open', 'class', 'target', 'rel'],
  });

  // Re-inject pre-rendered safe KaTeX and Shiki/Mermaid containers
  for (const item of mathBlocks) {
    safeHtml = safeHtml.replaceAll(item.token, item.html);
  }
  for (const item of codePlaceholders) {
    safeHtml = safeHtml.replaceAll(item.token, item.html);
  }

  return safeHtml;
}

let mermaidSeq = 1;

/**
 * Hydrates Mermaid diagrams and lazy-highlighted Shiki code blocks inside a rendered element.
 */
export async function hydrateRenderedMarkdown(
  container: HTMLElement,
  isStreaming: boolean,
): Promise<void> {
  // 1. Hydrate any un-highlighted code blocks if Shiki just finished loading
  const codeCards = container.querySelectorAll<HTMLElement>('.md-code-card[data-lang]');
  if (codeCards.length > 0) {
    try {
      const hl = await getHighlighter();
      const loaded = hl.getLoadedLanguages();
      for (const card of codeCards) {
        if (card.dataset.highlighted === '1') continue;
        const lang = card.dataset.lang || '';
        const raw = card.dataset.rawCode || '';
        if (lang && loaded.includes(lang)) {
          const body = card.querySelector<HTMLElement>('.md-code-body');
          if (body) {
            body.innerHTML = hl.codeToHtml(raw, { lang, theme: 'dark-plus' });
            card.dataset.highlighted = '1';
          }
        }
      }
    } catch {
      // keep plain pre fallback
    }
  }

  // 2. Render Mermaid SVGs (skip throwing errors on partial diagrams mid-stream)
  const mermaidCards = container.querySelectorAll<HTMLElement>('.md-mermaid-card');
  for (const card of mermaidCards) {
    const src = card.dataset.mermaidSource || '';
    if (!src.trim()) continue;
    if (card.dataset.mermaidRendered === src) continue;

    const preview = card.querySelector<HTMLElement>('.md-mermaid-preview');
    if (!preview) continue;

    try {
      const id = `mmd_${mermaidSeq++}`;
      const { svg } = await mermaid.render(id, src);
      preview.innerHTML = svg;
      card.dataset.mermaidRendered = src;
    } catch {
      if (isStreaming) {
        preview.innerHTML = `<div class="md-mermaid-loading"><i class="codicon codicon-loading codicon-modifier-spin"></i><span>Streaming diagram...</span></div>`;
      } else {
        preview.innerHTML = `<div class="md-mermaid-fallback">Unable to parse diagram syntax — click <strong>Source</strong> to inspect.</div>`;
      }
    }
  }
}
