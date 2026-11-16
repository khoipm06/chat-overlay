// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { renderMarkdownHtml } from '../src/renderer/markdown';

describe('VS Code Markdown Renderer', () => {
  it('renders KaTeX inline and display math without leaking raw dollar syntax', () => {
    const html = renderMarkdownHtml(
      'Inline $a^2 + b^2 = c^2$ and display:\n\n$$\\int_{a}^{b} f(x)\\,dx = F(b) - F(a)$$',
    );
    expect(html).toContain('class="katex"');
    expect(html).toContain('class="md-math-block"');
    expect(html).not.toContain('@@MATH_TOKEN');
  });

  it('renders GitHub callouts, nested blockquotes, task lists, tables, mermaid, and details', () => {
    const src = [
      '> [!NOTE]',
      '> Important callout with **bold**.',
      '',
      '> Level 1',
      '>> Level 2',
      '',
      '- [x] Completed item',
      '- [ ] Pending item',
      '',
      '| Left | Center |',
      '|:-----|:------:|',
      '| `code` | **yes** |',
      '',
      '```mermaid',
      'graph TD',
      '  A --> B',
      '```',
      '',
      '<details><summary>Expand me</summary>Hidden text</details>',
    ].join('\n');

    const html = renderMarkdownHtml(src);
    expect(html).toContain('md-callout-note');
    expect(html).toContain('<blockquote>');
    expect(html).toContain('task-list-item-checkbox');
    expect(html).toContain('md-table-wrap');
    expect(html).toContain('md-mermaid-card');
    expect(html).toContain('<details><summary>Expand me</summary>');
  });
});
