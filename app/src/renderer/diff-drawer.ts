import type { GitDiffReport, GitFileDiff } from '../shared/types';
import { escapeHtmlText } from './tool-presenter';

function serializeFilePatch(file: GitFileDiff): string {
  const out: string[] = [`--- a/${file.filePath}`, `+++ b/${file.filePath}`];
  for (const h of file.hunks) {
    out.push(h.header);
    for (const line of h.lines) {
      out.push(line.text);
    }
  }
  return out.join('\n');
}

function statusBadge(status: GitFileDiff['status']): { label: string; cls: string } {
  switch (status) {
    case 'added':
      return { label: 'A', cls: 'diff-badge-add' };
    case 'deleted':
      return { label: 'D', cls: 'diff-badge-del' };
    case 'untracked':
      return { label: '?', cls: 'diff-badge-untracked' };
    default:
      return { label: 'M', cls: 'diff-badge-mod' };
  }
}

export async function renderGitDiffDrawer(opts: {
  container: HTMLElement;
  workspaceDir: string;
  initialFile?: string;
  onReviewFiles: (files: string[]) => void;
  onRestored: () => Promise<void>;
  onNotice: (msg: string) => void;
}): Promise<void> {
  opts.container.innerHTML =
    '<div class="diff-empty"><i class="codicon codicon-loading codicon-modifier-spin"></i> Loading Git diffs...</div>';

  let report: GitDiffReport;
  try {
    report = await window.agentOverlay.getGitDiffReport(opts.workspaceDir);
  } catch (err) {
    opts.container.innerHTML = `<div class="diff-empty">${escapeHtmlText(
      err instanceof Error ? err.message : 'Failed to load Git diffs.',
    )}</div>`;
    return;
  }

  if (!report.isRepo) {
    opts.container.innerHTML =
      '<div class="diff-empty">Current workspace is not a Git repository.</div>';
    return;
  }

  opts.container.innerHTML = '';

  const topBar = document.createElement('div');
  topBar.className = 'diff-topbar';
  const branchEl = document.createElement('div');
  branchEl.className = 'diff-branch-summary';
  const totalAdd = report.files.reduce((acc, f) => acc + f.additions, 0);
  const totalDel = report.files.reduce((acc, f) => acc + f.deletions, 0);
  branchEl.innerHTML = `<i class="codicon codicon-git-branch"></i> <strong>${escapeHtmlText(report.branch || 'HEAD')}</strong> · ${report.files.length} file(s) <span class="diff-stat-add">+${totalAdd}</span> <span class="diff-stat-del">-${totalDel}</span>`;

  const topActions = document.createElement('div');
  topActions.className = 'diff-top-actions';
  const refreshBtn = document.createElement('button');
  refreshBtn.type = 'button';
  refreshBtn.className = 'mini-action-btn';
  refreshBtn.innerHTML = '<i class="codicon codicon-refresh"></i>';
  refreshBtn.title = 'Refresh diffs';
  refreshBtn.addEventListener('click', () => {
    void renderGitDiffDrawer(opts);
  });

  if (report.files.length > 0) {
    const askBtn = document.createElement('button');
    askBtn.type = 'button';
    askBtn.className = 'mini-action-btn primary';
    askBtn.textContent = 'Ask Agent Review';
    askBtn.addEventListener('click', () => {
      opts.onReviewFiles(report.files.map((f) => f.filePath));
    });
    topActions.appendChild(askBtn);
  }
  topActions.appendChild(refreshBtn);
  topBar.append(branchEl, topActions);
  opts.container.appendChild(topBar);

  if (report.files.length === 0) {
    const clean = document.createElement('div');
    clean.className = 'diff-empty';
    clean.textContent = 'Working tree is clean — no modified or untracked files.';
    opts.container.appendChild(clean);
    return;
  }

  const list = document.createElement('div');
  list.className = 'diff-file-list';

  report.files.forEach((file, idx) => {
    const card = document.createElement('div');
    card.className = 'diff-file-card';
    const expandedByDefault = opts.initialFile
      ? file.filePath === opts.initialFile
      : idx === 0;

    const header = document.createElement('div');
    header.className = 'diff-file-header';

    const left = document.createElement('button');
    left.type = 'button';
    left.className = 'diff-file-toggle';
    const badge = statusBadge(file.status);
    left.innerHTML = `<i class="codicon codicon-${expandedByDefault ? 'chevron-down' : 'chevron-right'}"></i>
      <span class="diff-status-badge ${badge.cls}">${badge.label}</span>
      <span class="diff-file-path" title="${escapeHtmlText(file.filePath)}">${escapeHtmlText(file.filePath)}</span>
      <span class="diff-file-counts"><span class="diff-stat-add">+${file.additions}</span> <span class="diff-stat-del">-${file.deletions}</span></span>`;

    const right = document.createElement('div');
    right.className = 'diff-file-actions';

    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'icon-btn';
    copyBtn.title = 'Copy unified patch';
    copyBtn.innerHTML = '<i class="codicon codicon-copy"></i>';
    copyBtn.addEventListener('click', () => {
      void navigator.clipboard.writeText(serializeFilePatch(file));
      opts.onNotice(`Copied patch for ${file.filePath}`);
    });

    const restoreBtn = document.createElement('button');
    restoreBtn.type = 'button';
    restoreBtn.className = 'icon-btn';
    restoreBtn.title = 'Discard changes / restore file to HEAD';
    restoreBtn.innerHTML = '<i class="codicon codicon-discard"></i>';
    restoreBtn.addEventListener('click', async () => {
      restoreBtn.disabled = true;
      const res = await window.agentOverlay.restoreGitFile(file.filePath, opts.workspaceDir);
      opts.onNotice(res.message);
      await opts.onRestored();
      void renderGitDiffDrawer({ ...opts, initialFile: undefined });
    });

    right.append(copyBtn, restoreBtn);
    header.append(left, right);

    const body = document.createElement('div');
    body.className = `diff-file-body${expandedByDefault ? '' : ' hidden'}`;

    for (const hunk of file.hunks) {
      const hunkHeader = document.createElement('div');
      hunkHeader.className = 'diff-hunk-header';
      hunkHeader.textContent = hunk.header;
      body.appendChild(hunkHeader);

      const codeBox = document.createElement('div');
      codeBox.className = 'diff-lines-box';
      for (const line of hunk.lines) {
        const row = document.createElement('div');
        row.className = `diff-line diff-line-${line.kind}`;
        row.textContent = line.text;
        codeBox.appendChild(row);
      }
      body.appendChild(codeBox);
    }

    if (file.truncated) {
      const trunc = document.createElement('div');
      trunc.className = 'diff-truncated-note';
      trunc.textContent = 'Diff truncated for large file preview.';
      body.appendChild(trunc);
    }

    left.addEventListener('click', () => {
      const collapsed = body.classList.toggle('hidden');
      const icon = left.querySelector('.codicon');
      if (icon) {
        icon.className = `codicon codicon-${collapsed ? 'chevron-right' : 'chevron-down'}`;
      }
    });

    card.append(header, body);
    list.appendChild(card);
  });

  opts.container.appendChild(list);
}
